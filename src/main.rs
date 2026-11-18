use anyhow::Context;
use clap::{CommandFactory, FromArgMatches, Parser, ValueEnum};
use codex_browser_bridge::{cli, client, config, discovery, doctor, logging, mcp, native, session};
use std::path::Path;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Mode {
    Mcp,
    Cli,
    Discover,
    Doctor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Surface {
    Browser,
    Computer,
}

#[derive(Debug, Parser)]
#[command(name = "codex-browser-bridge")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Bridge MCP clients to the ChatGPT desktop app's browser-extension pipe")]
struct Args {
    #[arg(long, value_enum, default_value_t = Mode::Mcp)]
    mode: Mode,

    /// Use the installed native cua_repl MCP service for Computer Use.
    #[arg(long, value_enum, default_value_t = Surface::Browser)]
    surface: Surface,

    /// Read-only native host MCP JSON containing mcpServers.cua_repl.
    #[arg(long)]
    native_config: Option<std::path::PathBuf>,

    /// Also save native CLI image content to this directory, keeping JSON data.
    #[arg(long)]
    image_dir: Option<std::path::PathBuf>,

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

    /// Browser Use network policy JSON applied to every tab attach.
    ///
    /// Shape is the extension's own: {"timeout_ms":1..30000,"constraints":[
    /// {"kind":"domains"|"origins","mode":"allowlist"|"denylist","patterns":[]}]}.
    /// Omit for the documented empty policy (allow all, no rules installed).
    #[arg(long = "network-policy")]
    network_policy: Option<String>,

    /// Maximum JSON bytes per command result (not a token count).
    #[arg(long, global = true, default_value = "16384", value_parser = cli::parse_output_budget)]
    max_output_bytes: usize,

    /// Reuse one browser session across separate `call` or `batch` commands.
    ///
    /// Each process otherwise starts a fresh session, so a tab created by one
    /// `call` cannot be used by the next. Commands write the active identifiers
    /// here with exact browser identity, and lock the file for each invocation;
    /// `--finalize` deletes it. Legacy/corrupt tokens are rejected, not reset.
    /// MCP stdio owns its turn for the process lifetime and does not use this.
    #[arg(long, global = true)]
    session_file: Option<std::path::PathBuf>,

    /// End the browser turn after a `call` or `batch`, even on failure.
    ///
    /// This is the default for one-shot commands and preserves marked
    /// handoff/deliverable tabs. With `--session-file`, cleanup is off by
    /// default; pass this flag to end the turn and delete the session file.
    #[arg(long, global = true, conflicts_with = "keep_turn")]
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
    let matches = Args::command().get_matches_from(normalized_args(std::env::args()));
    let output_budget_explicit =
        matches.value_source("max_output_bytes") == Some(clap::parser::ValueSource::CommandLine);
    let args = Args::from_arg_matches(&matches)?;
    if args.command.is_some() && !matches!(args.mode, Mode::Mcp) {
        anyhow::bail!("commands cannot be combined with --mode; omit --mode for tools/call/batch");
    }
    if args.surface == Surface::Computer {
        if args.session_file.is_some() || args.keep_turn {
            anyhow::bail!("Computer Use objects live in one native worker; use batch, not --session-file or --keep-turn");
        }
        if args.image_dir.is_some()
            && !matches!(
                args.command.as_ref(),
                Some(cli::Command::Call { .. } | cli::Command::Batch)
            )
        {
            anyhow::bail!("--image-dir requires a Computer Use call or batch command");
        }
        if args.pipe.is_some()
            || args.browser_family.is_some()
            || args.browser_instance.is_some()
            || args.profile.is_some()
            || args.upload_base.is_some()
            || args.max_text_bytes.is_some()
            || args.max_image_bytes.is_some()
            || args.network_policy.is_some()
            || args.finalize
            || output_budget_explicit
        {
            anyhow::bail!("browser selection, profiles, output limits, network policy and --finalize require --surface browser; native results are preserved without browser output limits");
        }
        let launch = native::Launch::discover(args.native_config.as_deref())?;
        if let Some(command) = args.command {
            return native::run_cli(launch, command, args.image_dir.as_deref()).await;
        }
        return match args.mode {
            Mode::Mcp => native::run_mcp(launch).await,
            Mode::Cli => anyhow::bail!("Computer Use CLI requires tools, call, or batch"),
            Mode::Doctor => native::run_doctor(launch).await,
            Mode::Discover => {
                println!("{}", serde_json::to_string_pretty(&launch.summary())?);
                Ok(())
            }
        };
    }
    if args.native_config.is_some() || args.image_dir.is_some() {
        anyhow::bail!("--native-config and --image-dir require --surface computer");
    }
    let owns_turn = matches!(
        args.command.as_ref(),
        Some(cli::Command::Call { .. }) | Some(cli::Command::Batch)
    );
    if (args.session_file.is_some() || args.finalize || args.keep_turn) && !owns_turn {
        anyhow::bail!(
            "--session-file, --finalize, and --keep-turn require a call or batch command"
        );
    }
    let config = config::Config::load()?;
    let environment = config::Config::from_env()?;
    if let Some(limit) = args.max_text_bytes {
        config::validate_content_bytes("--max-text-bytes", limit)?;
    }
    if let Some(limit) = args.max_image_bytes {
        config::validate_content_bytes("--max-image-bytes", limit)?;
    }
    if let Some(policy) = args.network_policy.as_deref() {
        codex_browser_bridge::network_policy::NetworkPolicy::from_json(policy)?;
    }
    if args
        .upload_base
        .as_ref()
        .is_some_and(|base| base.trim().is_empty())
    {
        anyhow::bail!("--upload-base must not be empty");
    }
    // Documented precedence is CLI flags > environment > config file > default.
    let browser_selection = config.browser_selection(
        args.browser_family.clone(),
        args.browser_instance.clone(),
        &environment,
    )?;
    let selector = browser_selection.selector.clone();
    if args.pipe.is_some() && !selector.is_empty() {
        anyhow::bail!("--pipe cannot be combined with a browser family or instance selector");
    }

    // Resolve once, before any connection: an invalid policy must fail here
    // rather than on the first attach, which already detached the debugger.
    let network_policy = args
        .network_policy
        .clone()
        .or_else(|| environment.network_policy.clone())
        .or_else(|| config.network_policy.clone())
        .map(|text| codex_browser_bridge::network_policy::NetworkPolicy::from_json(&text))
        .transpose()?;

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
            let session_lock = args
                .session_file
                .as_deref()
                .map(session::SessionLock::acquire)
                .transpose()?;
            let session_path = session_lock.as_ref().map(|lock| lock.path.as_path());
            let saved = session_path
                .map(session::load_session)
                .transpose()?
                .flatten();
            let selector = match &saved {
                Some(saved) => saved.browser.selector(&selector)?,
                None => selector,
            };
            let session_ids = saved
                .as_ref()
                .map(|saved| (saved.session_id.clone(), saved.turn_id.clone()));
            let client = match (&args.pipe, session_path) {
                (Some(pipe), Some(_)) => client::Client::lazy_with_verified_endpoint_and_ids(
                    pipe.clone(),
                    selector,
                    session_ids,
                ),
                _ => client::Client::lazy_with_selector_and_ids(
                    args.pipe.clone(),
                    selector,
                    session_ids,
                ),
            };
            if let Some(path) = session_path {
                client.set_continuation_path(path);
            }
            if let Some(base) = args
                .upload_base
                .clone()
                .or_else(|| environment.upload_base.clone())
                .or_else(|| config.upload_base.clone())
            {
                std::env::set_var("CODEX_BRIDGE_UPLOAD_BASE", base);
            }
            if let Some(max_text_bytes) = args
                .max_text_bytes
                .or(environment.max_text_bytes)
                .or(config.max_text_bytes)
            {
                std::env::set_var("CODEX_BRIDGE_MAX_TEXT_BYTES", max_text_bytes.to_string());
            }
            if let Some(max_image_bytes) = args
                .max_image_bytes
                .or(environment.max_image_bytes)
                .or(config.max_image_bytes)
            {
                std::env::set_var("CODEX_BRIDGE_MAX_IMAGE_BYTES", max_image_bytes.to_string());
            }
            if let Some(policy) = network_policy.clone() {
                client.set_network_policy(policy);
            }
            let profile = args.profile.or_else(|| {
                match environment.profile.as_deref().or(config.profile.as_deref()) {
                    Some("basic") => Some(Profile::Basic),
                    Some("network") => Some(Profile::Network),
                    Some("full") => Some(Profile::Full),
                    _ => None, // Config has already validated and normalized these values.
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
            server.set_browser_selection(browser_selection.clone());
            if let Some(command) = args.command {
                let finalize = !args.keep_turn && (args.finalize || args.session_file.is_none());
                run_until_interrupted(
                    &server,
                    cli::run_command(
                        &server,
                        command,
                        args.max_output_bytes,
                        session_path,
                        finalize,
                    ),
                    finalize,
                    session_path,
                )
                .await?;
            } else {
                let result = tokio::select! {
                    result = server.run_stdio() => result,
                    _ = shutdown_signal() => {
                        let _ = server.shutdown_turn().await;
                        anyhow::bail!("interrupted; attempted browser turn cleanup")
                    }
                };
                // EOF and stdio errors both release the turn this process owned.
                server.shutdown_turn().await;
                result?;
            }
        }
        Mode::Cli => {
            let client = client::Client::connect_with_selector(args.pipe.as_deref(), selector)
                .await
                .context("failed to connect to the ChatGPT browser-extension pipe")?;
            if let Some(policy) = network_policy.clone() {
                client.set_network_policy(policy);
            }
            let result = tokio::select! {
                result = cli::run_cli(client.clone()) => result,
                _ = shutdown_signal() => {
                    let _ = mcp::shutdown_client(&client).await;
                    anyhow::bail!("interrupted; attempted browser turn cleanup")
                }
            };
            mcp::shutdown_client(&client).await;
            result?;
        }
        Mode::Doctor => {
            let mut result = serde_json::to_value(doctor::run_diagnostics().await)?;
            result["browserSelection"] = serde_json::to_value(&browser_selection)?;
            result["transportMode"] = serde_json::json!(if args.pipe.is_some() {
                "explicit_endpoint"
            } else {
                "discovered_instance"
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}

/// Run a command until it finishes or the process is asked to stop.
///
/// An interrupted shell command may own its browser turn. Cleanup follows the
/// same finalize policy as a normal exit; session-file and --keep-turn preserve
/// the turn and its session file.
async fn run_until_interrupted<F>(
    server: &mcp::Server,
    future: F,
    cleanup_on_interrupt: bool,
    session_file: Option<&Path>,
) -> anyhow::Result<()>
where
    F: std::future::Future<Output = anyhow::Result<()>>,
{
    tokio::select! {
        result = future => result,
        _ = shutdown_signal() => {
            if cleanup_on_interrupt {
                let ended = server.shutdown_turn().await;
                if ended {
                    if let Some(path) = session_file {
                        if let Err(err) = cli::clear_session(path) {
                            anyhow::bail!(
                                "interrupted; browser turn cleanup completed, but the session file could not be removed: {err:#}"
                            );
                        }
                    }
                    anyhow::bail!("interrupted; browser turn cleanup completed")
                } else {
                    anyhow::bail!("interrupted; browser turn cleanup was not confirmed")
                }
            } else {
                tracing::warn!(
                    "interrupted; leaving the browser turn open because --keep-turn or --session-file is active"
                );
                anyhow::bail!("interrupted; browser turn left open")
            }
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
