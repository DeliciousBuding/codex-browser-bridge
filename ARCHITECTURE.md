# Architecture

A contributor's guide to how `codex-browser-bridge` fits together and why it
is shaped the way it is. For *what* each piece does, read the source; this
file records the *decisions*.

## Overview

A single-process MCP (Model Context Protocol) server that bridges MCP clients
(Claude Code, agents) to Codex Desktop's Chrome browser. It speaks stdio
JSON-RPC to the client and a length-prefixed JSON wire protocol over a Windows
named pipe to the Codex Chrome extension, which then drives Chrome via the
Chrome DevTools Protocol (CDP).

```
MCP client ──stdio JSON-RPC──▶ bridge ──named pipe──▶ Codex Chrome ext ──CDP──▶ Chrome
```

The shipped binary is Windows-only (named pipes). The **library** compiles on
other platforms with stub `pipe`/`discovery` modules — this exists so the
reconnect and protocol logic can be tested with `tokio::io::duplex()` mocks.

## Module map (`src/`)

| Module | Responsibility |
|---|---|
| `main.rs` | CLI entry. `--mode mcp\|cli\|discover\|doctor`; loads config |
| `client.rs` | The pipe `Client`: requests, `execute_cdp`, reconnect, event subscriptions |
| `pipe.rs` | `PipeStream` type + `dial_named_pipe` (Windows `NamedPipeClient`; `DuplexStream` stub elsewhere) |
| `protocol.rs` | Length-prefixed frame encode/decode, `Request`/`Response`, session-param merge |
| `discovery.rs` | Enumerate `codex-browser-use-*` pipes (PowerShell) |
| `browser.rs` | The 52 tool implementations over `Client` (navigate, dom, screenshot, network_monitor, …) |
| `mcp/` | MCP server: JSON-RPC dispatch, 52 tool handlers, schema, profiles, resources/prompts |
| `security.rs` | URL scheme + file-path validation (path-traversal defense) |
| `config.rs` | Optional TOML config (profile, upload_base) |
| `doctor.rs` | Pipe connectivity diagnostics (`--mode doctor`) |
| `error.rs` | `BridgeError` (`Connection`, `PipeIo`, `Cdp`, `Rpc`, `Protocol`, …) |

## Key design decisions

### Single Client, request-driven reconnect
One `Client` holds the pipe connection. The `read_loop` routes responses to
pending requests and CDP events to subscribers. If the read loop dies (pipe
break / Codex restart / extension refresh) the client **no longer bricks**:

- read-loop exit sets `alive=false`, drains pending, reclaims the writer
- the next `send_request` runs `ensure_alive` → `reconnect_locked` (discover +
  dial, 3-attempt backoff 0/100/250 ms, 5 s cooldown on full failure) → swaps
  the writer, restarts the read loop, clears `attached_tabs`
- connection-level errors (`Connection`, `PipeIo`) trigger one reconnect+retry;
  protocol / RPC / CDP errors do not

In-flight requests at the moment of disconnect are sacrificed once — the caller
gets a `Connection` error and can retry. This is simpler and more robust than
replaying pending requests by id (whose responses would never arrive).

### Injectable connection factory
Reconnect must dial a fresh pipe, but dialing is platform-specific. The factory
(`Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<PipeStream>>>>>`) lets
production use real discovery while tests inject a `duplex()` mock. Deliberately
**not** a trait — one use, no `async_trait`, no abstraction layer (the project
previously removed a `BridgeClient` trait for being unused ceremony).

### Sticky CDP attach
Each tab needs a CDP debugger session. `attached_tabs` caches known-attached
tabs to skip the detach+attach round-trip on repeated calls. A 20 s fast-timeout
on the sticky path falls through to a full re-attach if a background-throttled
tab goes silent, instead of burning the whole 60 s budget.

### CDP event subscription
Frames carrying a `method` and no `id` are server-pushed events. `read_loop`
routes the whole frame to subscribers matched by method prefix
(`network_monitor`, `console_logs`). Subscribers get the entire frame so they
can dispatch on the exact method themselves.

### Tool profiles
52 tools, filtered by profile — `basic` (34) / `network` (51) / `full` (52) —
via `CODEX_BRIDGE_PROFILE` env, `--profile` flag, or config file. Keeps the
tool list small for agents that don't need the network/debugging surface.

### Config precedence
CLI flags > config file (`.codex-browser-bridge.toml`) > env > built-in
default. A missing default config file is silent; a malformed one warns and is
ignored. If `CODEX_BRIDGE_CONFIG` is set, that path is authoritative and the
bridge does not fall back to a working-directory config.

### MCP resources + prompts
- **resources**: `codex://tabs` — an on-demand snapshot of open tabs
- **prompts**: `login`, `extract-table` — workflow templates that cite the
  concrete tools to call, so the agent picks the right path first time

Subscribe / list-changes is intentionally omitted: these are on-demand
snapshots, not a live feed.

## Data flow — a tool call

1. MCP client sends `tools/call` (e.g. `codex_navigate`) over stdio
2. `mcp/handlers.rs` dispatches to `handle_navigate`
3. `browser.rs::navigate` calls `client.execute_cdp(tab, "Page.navigate", …)`
4. `Client` attaches a CDP session (if not cached) and writes an `executeCdp`
   frame to the pipe
5. The Codex extension runs the CDP command in Chrome and returns a result frame
6. `read_loop` routes the response to the pending request's oneshot channel
7. `browser.rs` parses the result and returns `Content` to the MCP client

## Testing

- **Windows** (CI `rust` job): lib unit tests, integration tests, clippy
  `--all-targets`, coverage, x64 + arm64 release build
- **Ubuntu** (CI `test-lib` job): reconnect + resources/prompts tests under
  `cfg(not(windows))` — `PipeStream` is `DuplexStream` there, so a mock pipe is
  just a `tokio::io::duplex()` pair
- **benches/protocol.rs**: criterion baseline for frame encode/decode

Pure logic (`pair_network_events`, `runtime_value_string`, `check_cdp_error`)
is extracted into standalone functions and unit-tested directly — never via
`#[path]` source-tree embedding (that anti-pattern broke the v1.8/v1.9
releases).

## Supply chain

`cargo-deny` (`deny.toml`): advisories + licenses (tight allow list — a new
dependency under an unlisted license fails the check and forces review) + bans
+ sources. Runs on every push/PR plus a weekly schedule. `dependabot` covers
`cargo` (grouped, weekly) and `github-actions`.

## Release

Tagging `v*` triggers `release.yml`: test → build x64 + arm64 → checksums and
attestations → draft GitHub Release → npm publish (`--provenance`) → publish
the GitHub Release. The npm package embeds release checksums and downloads the
matching binary on install.
