use clap::{Parser, ValueEnum};
use codex_browser_bridge::{cli, logging, native};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Mode {
    Mcp,
    Discover,
    Doctor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Surface {
    NativeBrowser,
    Computer,
}

#[derive(Debug, Parser)]
#[command(name = "codex-browser-bridge")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(
    about = "Thin MCP and CLI relay to the installed Codex native browser and Computer Use runtimes"
)]
struct Args {
    /// Execution mode: `mcp` (stdio server), `discover` (print launch row), or `doctor` (handshake and catalog check).
    #[arg(long, value_enum, default_value_t = Mode::Mcp)]
    mode: Mode,

    /// Target Codex native runtime: `native-browser` (`node_repl`) or `computer` (`cua_repl`).
    #[arg(long, value_enum, default_value_t = Surface::NativeBrowser)]
    surface: Surface,

    /// Read-only MCP JSON containing the selected native service's launch row.
    #[arg(long)]
    native_config: Option<std::path::PathBuf>,

    /// Also save native CLI image content to this directory, keeping JSON data (`call` / `batch`).
    #[arg(long)]
    image_dir: Option<std::path::PathBuf>,

    #[command(subcommand)]
    command: Option<cli::Command>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();
    let args = Args::parse();
    if args.command.is_some() && !matches!(args.mode, Mode::Mcp) {
        anyhow::bail!("commands cannot be combined with --mode; omit --mode for tools/call/batch");
    }
    if args.image_dir.is_some()
        && !matches!(
            args.command.as_ref(),
            Some(cli::Command::Call { .. } | cli::Command::Batch)
        )
    {
        anyhow::bail!("--image-dir requires a native call or batch command");
    }
    let server = match args.surface {
        Surface::NativeBrowser => native::NativeServer::Browser,
        Surface::Computer => native::NativeServer::Computer,
    };
    let launch = native::Launch::discover(server, args.native_config.as_deref())?;
    if let Some(command) = args.command {
        return native::run_cli(launch, command, args.image_dir.as_deref()).await;
    }
    match args.mode {
        Mode::Mcp => native::run_mcp(launch).await,
        Mode::Doctor => native::run_doctor(launch).await,
        Mode::Discover => {
            println!("{}", serde_json::to_string_pretty(&launch.summary())?);
            Ok(())
        }
    }
}
