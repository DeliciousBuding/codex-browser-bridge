use anyhow::Context;
use clap::{Parser, ValueEnum};
use codex_browser_bridge::{cli, client, discovery, logging, mcp};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Mode {
    Mcp,
    Cli,
    Discover,
}

#[derive(Debug, Parser)]
#[command(name = "codex-browser-bridge")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Bridge MCP clients to Codex Desktop's Chrome browser pipe")]
struct Args {
    #[arg(long, value_enum, default_value_t = Mode::Mcp)]
    mode: Mode,

    #[arg(long)]
    pipe: Option<String>,

    #[arg(long, value_enum)]
    profile: Option<Profile>,

    #[arg(long)]
    upload_base: Option<String>,
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

    match args.mode {
        Mode::Discover => {
            let pipes = discovery::discover_codex_pipes().await?;
            if pipes.is_empty() {
                anyhow::bail!("No codex-browser-use pipes found. Is Codex Desktop running?");
            }
            println!("{}", serde_json::to_string_pretty(&pipes)?);
        }
        Mode::Mcp => {
            let client = client::Client::connect(args.pipe.as_deref())
                .await
                .context("failed to connect to Codex browser pipe")?;
            if let Some(ref base) = args.upload_base {
                std::env::set_var("CODEX_BRIDGE_UPLOAD_BASE", base);
            }
            let server = if let Some(profile) = args.profile {
                let p = match profile {
                    Profile::Basic => codex_browser_bridge::mcp::profiles::ToolProfile::Basic,
                    Profile::Network => codex_browser_bridge::mcp::profiles::ToolProfile::Network,
                    Profile::Full => codex_browser_bridge::mcp::profiles::ToolProfile::Full,
                };
                mcp::Server::new_with_profile(client, p)
            } else {
                mcp::Server::new(client)
            };
            server.run_stdio().await?;
        }
        Mode::Cli => {
            let client = client::Client::connect(args.pipe.as_deref())
                .await
                .context("failed to connect to Codex browser pipe")?;
            cli::run_cli(client).await?;
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
            "-version" => "--version".to_string(),
            _ if arg.starts_with("-mode=") => arg.replacen("-mode=", "--mode=", 1),
            _ if arg.starts_with("-pipe=") => arg.replacen("-pipe=", "--pipe=", 1),
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
