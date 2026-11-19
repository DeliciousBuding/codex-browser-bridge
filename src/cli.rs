//! Compact CLI subcommands forwarded directly to the native Codex MCP runtime.

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List tools or fetch one tool's MCP schema from the native runtime.
    Tools { name: Option<String> },
    /// Call a native MCP tool with a JSON object, without the MCP protocol envelope.
    Call {
        name: String,
        #[arg(default_value = "{}")]
        arguments: String,
    },
    /// Read {"name":"...","arguments":{}} JSONL from stdin in one worker; stop on error.
    Batch,
}
