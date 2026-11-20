# Rust Rewrite Design

The Rust rewrite keeps the public contract unchanged: same executable name, same CLI flags, same MCP tool names, same extension RPC wire format, same npm package entrypoint.

## Current Contract

The bridge has three external surfaces:

- CLI: `--mode mcp`, `--mode cli`, `--mode discover`, `--pipe <name>`, `--version`
- MCP stdio: line-delimited JSON-RPC requests and line-delimited JSON-RPC responses
- Codex extension pipe: 4-byte little-endian frame length followed by JSON-RPC payload

The extension RPC layer injects `session_id` and `turn_id` into every request. `executeCdp` uses the nested target shape:

```json
{
  "target": { "tabId": 123 },
  "method": "Runtime.evaluate",
  "commandParams": {}
}
```

`claimUserTab` sends integer `tabId`. `getUserTabs` accepts both a bare array and `{ "tabs": [...] }` for compatibility.

## Rust Module Layout

- `src/main.rs`: CLI entrypoint and mode dispatch
- `src/discovery.rs`: Windows pipe enumeration and pipe-name parsing
- `src/pipe.rs`: Windows named pipe client
- `src/protocol.rs`: framed JSON-RPC encoding and decoding
- `src/client.rs`: extension RPC client, pending map, request timeouts, per-tab CDP locks
- `src/browser.rs`: high-level browser operations
- `src/mcp.rs`: MCP stdio server and tool handlers
- `src/error.rs`: stable errors for CLI, MCP, and tests

## Parallel Work Plan

Tasks use disjoint write sets so agents can work at the same time.

| Task | Owner | Write Set | Output |
|---|---|---|---|
| Protocol parity | worker-protocol | `src/protocol.rs`, `tests/protocol_parity.rs` | Go-compatible frame tests |
| Pipe discovery | worker-discovery | `src/discovery.rs`, `src/pipe.rs`, `tests/discovery_parity.rs` | Old/new pipe parsing and Windows dial behavior |
| Extension client | worker-client | `src/client.rs`, `tests/client_rpc.rs` | pending map, timeouts, health check, CDP retry |
| Browser API | worker-browser | `src/browser.rs`, `tests/browser_api.rs` | tab, navigation, CUA, DOM, screenshot parity |
| MCP server | worker-mcp | `src/mcp.rs`, `tests/mcp_parity.rs` | all tool schemas and handler outputs |
| npm packaging | worker-npm | `npm/`, `.github/workflows/`, release docs | Rust binary download and install path |
| Docs | worker-docs | `README.md`, `README.zh-CN.md`, `docs/rust-rewrite/` | concise Rust build and migration docs |

## Stability Rules

- Keep Go tests as the contract source until Rust parity tests cover the same behavior.
- Add Rust tests before replacing npm release artifacts.
- Keep per-tab CDP serialization. Improve attach reuse only after parity passes.
- Treat exact JSON shape as API. Tool descriptions can improve, tool names and argument names stay fixed.

## Performance Targets

Baseline and compare these before switching npm to Rust:

- cold startup
- idle private working set
- `discover` duration
- one `getInfo`
- ten sequential `Runtime.evaluate`
- parallel operations across two tabs
- screenshot capture duration

The first expected gains are lower idle memory and faster startup. Browser action latency mainly depends on extension and Chrome CDP round trips.
