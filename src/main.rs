use anyhow::Context;
use clap::{Parser, ValueEnum};
use codex_browser_bridge::registry::BrowserSelector;
use codex_browser_bridge::{cli, client, config, discovery, doctor, logging, mcp};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Mode {
    Mcp,
    Cli,
    Discover,
    Doctor,
}

#[derive(Debug, Parser)]
#[command(name = "codex-browser-bridge")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Bridge MCP clients to the ChatGPT desktop app's browser-extension pipe")]
struct Args {
    #[arg(long, value_enum, default_value_t = Mode::Mcp)]
    mode: Mode,

    #[arg(long)]
    pipe: Option<String>,

    #[arg(long)]
    browser_family: Option<String>,

    #[arg(long)]
    browser_instance: Option<String>,

    #[arg(long, value_enum)]
    profile: Option<Profile>,

    #[arg(long)]
    upload_base: Option<String>,

    #[arg(long)]
    max_text_bytes: Option<usize>,

    #[arg(long)]
    max_image_bytes: Option<usize>,

    /// Maximum JSON bytes per command result (not a token count).
    #[arg(long, global = true, default_value = "16384", value_parser = cli::parse_output_budget)]
    max_output_bytes: usize,

    /// Reuse one browser session across separate shell commands.
    ///
    /// Each process otherwise starts a fresh session, so a tab created by one
    /// `call` cannot be used by the next. Commands write the active identifiers
    /// here and read them on the next invocation; `--finalize` deletes the file.
    #[arg(long, global = true)]
    session_file: Option<std::path::PathBuf>,

    /// End the browser turn after a `call` or `batch`, even on failure.
    ///
    /// This is the default for one-shot commands and preserves marked
    /// handoff/deliverable tabs. It is ignored when `--session-file` is used,
    /// because that combination means "continue this turn later".
    #[arg(long, global = true)]
    finalize: bool,

    /// Leave the browser turn open after the command exits.
    ///
    /// A tab created by a one-shot process stays owned by a session that no
    /// longer exists, so prefer `batch` or `--session-file` for multi-step work.
    #[arg(long = "keep-turn", global = true)]
    keep_turn: bool,

    #[command(subcommand)]
    command: Option<cli::Command>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Profile {
    Basic,
    Network,
    Full,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();
    let args = Args::parse_from(normalized_args(std::env::args()));
    if args.command.is_some() && !matches!(args.mode, Mode::Mcp) {
        anyhow::bail!("commands cannot be combined with --mode; omit --mode for tools/call/batch");
    }
    let config = config::Config::load();
    // Documented precedence is CLI flags > environment > config file > default.
    let browser_family = args
        .browser_family
        .clone()
        .or_else(|| env_string("CODEX_BRIDGE_BROWSER_FAMILY"))
        .or_else(|| config.browser_family.clone());
    let browser_instance = args
        .browser_instance
        .clone()
        .or_else(|| env_string("CODEX_BRIDGE_BROWSER_INSTANCE"))
        .or_else(|| config.browser_instance.clone());
    let selector = BrowserSelector::new(browser_family, browser_instance);
    selector.validate()?;
    if args.pipe.is_some() && !selector.is_empty() {
        anyhow::bail!("--pipe cannot be combined with a browser family or instance selector");
    }

    match args.mode {
        Mode::Discover => {
            let pipes = discovery::discover_codex_pipes().await?;
            if pipes.is_empty() {
                anyhow::bail!(
                    "No codex-browser-use pipes found. Is the ChatGPT desktop app running with a supported browser extension connected?"
                );
            }
            println!("{}", serde_json::to_string_pretty(&pipes)?);
        }
        Mode::Mcp => {
            let session_ids = args.session_file.as_deref().and_then(cli::load_session);
            let client = client::Client::lazy_with_selector_and_ids(
                args.pipe.clone(),
                selector,
                session_ids,
            );
            if let Some(base) = args
                .upload_base
                .clone()
                .or_else(|| env_string("CODEX_BRIDGE_UPLOAD_BASE"))
                .or_else(|| config.upload_base.clone())
            {
                std::env::set_var("CODEX_BRIDGE_UPLOAD_BASE", base);
            }
            if let Some(max_text_bytes) = args
                .max_text_bytes
                .or_else(|| env_usize("CODEX_BRIDGE_MAX_TEXT_BYTES"))
                .or(config.max_text_bytes)
            {
                std::env::set_var("CODEX_BRIDGE_MAX_TEXT_BYTES", max_text_bytes.to_string());
            }
            if let Some(max_image_bytes) = args
                .max_image_bytes
                .or_else(|| env_usize("CODEX_BRIDGE_MAX_IMAGE_BYTES"))
                .or(config.max_image_bytes)
            {
                std::env::set_var("CODEX_BRIDGE_MAX_IMAGE_BYTES", max_image_bytes.to_string());
            }
            let profile = args.profile.or_else(profile_from_env).or_else(|| {
                match config.profile.as_deref() {
                    Some("basic") => Some(Profile::Basic),
                    Some("network") => Some(Profile::Network),
                    Some("full") => Some(Profile::Full),
                    Some(other) => {
                        tracing::warn!(profile = other, "unknown profile in config file, ignoring");
                        None
                    }
                    None => None,
                }
            });
            let server = match profile {
                Some(Profile::Basic) => mcp::Server::new_with_profile(
                    client,
                    codex_browser_bridge::mcp::profiles::ToolProfile::Basic,
                ),
                Some(Profile::Network) => mcp::Server::new_with_profile(
                    client,
                    codex_browser_bridge::mcp::profiles::ToolProfile::Network,
                ),
                Some(Profile::Full) => mcp::Server::new_with_profile(
                    client,
                    codex_browser_bridge::mcp::profiles::ToolProfile::Full,
                ),
                None => mcp::Server::new(client),
            };
            if let Some(command) = args.command {
                let finalize = !args.keep_turn && (args.finalize || args.session_file.is_none());
                run_until_interrupted(
                    &server,
                    cli::run_command(
                        &server,
                        command,
                        args.max_output_bytes,
                        args.session_file.as_deref(),
                        finalize,
                    ),
                )
                .await?;
            } else {
                server.run_stdio().await?;
                // The client disconnected: end the turn so agent tabs are not
                // left owned by a session that no longer has a reader.
                server.shutdown_turn().await;
            }
        }
        Mode::Cli => {
            let client = client::Client::connect_with_selector(args.pipe.as_deref(), selector)
                .await
                .context("failed to connect to the ChatGPT browser-extension pipe")?;
            cli::run_cli(client.clone()).await?;
            mcp::shutdown_client(&client).await;
        }
        Mode::Doctor => {
            let result = doctor::run_diagnostics().await;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}

fn env_string(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn env_usize(name: &str) -> Option<usize> {
    env_string(name).and_then(|value| value.parse().ok())
}

fn profile_from_env() -> Option<Profile> {
    let value = env_string("CODEX_BRIDGE_PROFILE")?;
    match value.to_ascii_lowercase().as_str() {
        "basic" => Some(Profile::Basic),
        "network" => Some(Profile::Network),
        "full" => Some(Profile::Full),
        other => {
            tracing::warn!(profile = other, "unknown CODEX_BRIDGE_PROFILE, ignoring");
            None
        }
    }
}

/// Run a command until it finishes or the process is asked to stop.
///
/// An interrupted shell command still owns its browser turn, so the same
/// cleanup the normal exit path performs runs here before returning.
async fn run_until_interrupted<F>(server: &mcp::Server, future: F) -> anyhow::Result<()>
where
    F: std::future::Future<Output = anyhow::Result<()>>,
{
    tokio::select! {
        result = future => result,
        _ = shutdown_signal() => {
            server.shutdown_turn().await;
            anyhow::bail!("interrupted; attempted browser turn cleanup")
        }
    }
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut terminate = match signal(SignalKind::terminate()) {
        Ok(stream) => stream,
        Err(err) => {
            tracing::warn!(error = %err, "could not install SIGTERM handler");
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

fn normalized_args<I>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    args.into_iter()
        .map(|arg| match arg.as_str() {
            "-mode" => "--mode".to_string(),
            "-pipe" => "--pipe".to_string(),
            "-browser-family" => "--browser-family".to_string(),
            "-browser-instance" => "--browser-instance".to_string(),
            "-version" => "--version".to_string(),
            _ if arg.starts_with("-mode=") => arg.replacen("-mode=", "--mode=", 1),
            _ if arg.starts_with("-pipe=") => arg.replacen("-pipe=", "--pipe=", 1),
            _ if arg.starts_with("-browser-family=") => {
                arg.replacen("-browser-family=", "--browser-family=", 1)
            }
            _ if arg.starts_with("-browser-instance=") => {
                arg.replacen("-browser-instance=", "--browser-instance=", 1)
            }
            _ => arg,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::normalized_args;

    #[test]
    fn normalizes_go_style_flags_for_rust_cli() {
        let got = normalized_args([
            "bridge".to_string(),
            "-mode=mcp".to_string(),
            "-pipe".to_string(),
            "codex-browser-use\\abc".to_string(),
            "-version".to_string(),
        ]);

        assert_eq!(
            got,
            vec![
                "bridge",
                "--mode=mcp",
                "--pipe",
                "codex-browser-use\\abc",
                "--version"
            ]
        );
    }
}
