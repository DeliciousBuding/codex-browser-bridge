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
    let selector = BrowserSelector::new(
        args.browser_family
            .clone()
            .or(config.browser_family.clone())
            .or_else(|| std::env::var("CODEX_BRIDGE_BROWSER_FAMILY").ok()),
        args.browser_instance
            .clone()
            .or(config.browser_instance.clone())
            .or_else(|| std::env::var("CODEX_BRIDGE_BROWSER_INSTANCE").ok()),
    );
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
            let client = client::Client::lazy_with_selector(args.pipe.clone(), selector);
            // Precedence: CLI flags > config file > env > default.
            if let Some(base) = args.upload_base.clone().or(config.upload_base) {
                std::env::set_var("CODEX_BRIDGE_UPLOAD_BASE", base);
            }
            if let Some(max_text_bytes) = args.max_text_bytes.or(config.max_text_bytes) {
                std::env::set_var("CODEX_BRIDGE_MAX_TEXT_BYTES", max_text_bytes.to_string());
            }
            if let Some(max_image_bytes) = args.max_image_bytes.or(config.max_image_bytes) {
                std::env::set_var("CODEX_BRIDGE_MAX_IMAGE_BYTES", max_image_bytes.to_string());
            }
            let profile = args.profile.or_else(|| match config.profile.as_deref() {
                Some("basic") => Some(Profile::Basic),
                Some("network") => Some(Profile::Network),
                Some("full") => Some(Profile::Full),
                Some(other) => {
                    tracing::warn!(profile = other, "unknown profile in config file, ignoring");
                    None
                }
                None => None,
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
                cli::run_command(&server, command, args.max_output_bytes).await?;
            } else {
                server.run_stdio().await?;
            }
        }
        Mode::Cli => {
            let client = client::Client::connect_with_selector(args.pipe.as_deref(), selector)
                .await
                .context("failed to connect to the ChatGPT browser-extension pipe")?;
            cli::run_cli(client).await?;
        }
        Mode::Doctor => {
            let result = doctor::run_diagnostics().await;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
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
