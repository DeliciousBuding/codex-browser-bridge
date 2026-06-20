# Project Overview: codex-browser-bridge

## Architecture

```
MCP Client (Claude Code)
    │ stdio JSON-RPC (newline-delimited)
    ▼
codex-browser-bridge (Rust v1.6.0)
    │ length-prefixed JSON-RPC frames (4-byte LE + payload)
    ▼
Windows Named Pipe (\\.\pipe\codex-browser-use-<uuid>)
    ▼
Codex Desktop Extension Host
    ▼
Codex Chrome Extension v1.1.5
    │ Chrome DevTools Protocol (CDP)
    ▼
Chrome Browser
```

## Technology Stack

| Layer | Technology | Version |
|-------|-----------|---------|
| Binary | Rust (rewrite from Go) | 1.6.0 |
| Build | Cargo | 1.85+ |
| Transport | Windows Named Pipes (go-winio → tokio) | — |
| Wire Protocol | Custom: 4-byte LE length + JSON-RPC 2.0 | — |
| MCP Protocol | JSON-RPC 2.0 over stdio (newline-delimited) | 2024-11-05 |
| Browser Control | Chrome DevTools Protocol via `executeCdp` RPC | — |

## Entry Points

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry: `-mode mcp` (default), `-mode discover`, `-mode cli` |
| `src/lib.rs` | Library root |
| `cmd/bridge/main.go` | Go entry (legacy, v1.5.x) |

## Build & Run

```bash
# Rust (current)
cargo build --locked --release          # → target/release/codex-browser-bridge.exe
cargo test --locked
cargo check --locked

# CLI modes
codex-browser-bridge -mode mcp          # MCP server (default)
codex-browser-bridge -mode discover     # List pipes
codex-browser-bridge -mode cli          # Interactive debug
```

## Key Architectural Decisions

1. **No direct CDP**: All CDP goes through Codex extension's `executeCdp` RPC. The bridge does NOT connect to Chrome DevTools directly.
2. **Tab-locked CDP**: Per-tab mutex prevents concurrent CDP operations on the same tab.
3. **Session-based**: Each pipe connection creates a new browser session with unique session_id/turn_id.
4. **Auto-discovery**: Scans `\\.\pipe\codex-browser-use-*` to find Codex pipes.
5. **Single binary**: No runtime dependencies, statically linked (except Windows system DLLs).
