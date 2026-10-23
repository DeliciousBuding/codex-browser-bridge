# Architecture

A contributor's guide to how `codex-browser-bridge` fits together and why it
is shaped the way it is. For *what* each piece does, read the source; this
file records the *decisions*.

## Overview

A single-process MCP (Model Context Protocol) server that bridges MCP clients
(Claude Code, agents) to the ChatGPT desktop app's browser (formerly Codex
Desktop; Chrome and Edge). It speaks stdio
JSON-RPC to the client and a length-prefixed JSON wire protocol over a local
Windows named pipe or Linux Unix socket to the ChatGPT browser extension, which then drives the browser via
the Chrome DevTools Protocol (CDP).

```
Claude plugin marketplace ──installs──▶ skill + MCP config
MCP client ──stdio JSON-RPC──▶ bridge ──local transport──▶ ChatGPT extension ──CDP──▶ Chromium browser
```

v1.12.0 publishes Windows x64/arm64 and Linux GNU x64/arm64 binaries
(glibc >= 2.39 for the Linux release builds). Linux discovery uses
`/tmp/codex-browser-use/*.sock`, with native Rust/Node gates on Ubuntu 24.04 x64
and arm64. Unix runtime support has not been verified against a live Linux
extension service, and a compiling macOS target does not establish macOS
support.

The repository is both the Claude plugin source and the generic MCP server
source. A Claude plugin marketplace distributes the manifest, progressive
skill, and MCP launch configuration; it does not wrap or duplicate the 64-tool
protocol surface. Codex and ChatGPT desktop already have native browser access
and are not plugin targets for this bridge.

## Module map (`src/`)

| Module | Responsibility |
|---|---|
| `main.rs` | CLI entry. `--mode mcp\|cli\|discover\|doctor`, `tools`, `call`, `batch`; loads config |
| `client.rs` | The pipe `Client`: requests, `execute_cdp`, reconnect, event subscriptions |
| `pipe.rs` | Platform transport and dialing; Windows named pipe or validated Unix stream; `dial_named_pipe` remains a compatibility entrypoint |
| `protocol.rs` | Length-prefixed frame encode/decode, `Request`/`Response`, session-param merge |
| `discovery.rs` | Enumerate Windows pipes or Linux `/tmp/codex-browser-use/*.sock` endpoints |
| `registry.rs` | Probe live extension instances, expose stable `extensionInstanceId` identity, and fail closed on ambiguous browser selection |
| `browser.rs` | Browser tool implementations over `Client` (navigate, DOM, screenshot, management, history, network_monitor, …) |
| `input.rs` | Bounded failure-safe keyboard chords; shared modifier parsing |
| `pointer.rs` | CUA pointer click/move/scroll/drag, official button/modifier/path fields, no-replay mutation path |
| `clipboard.rs` | Bounded session-virtual clipboard and paste integration; never reads the OS clipboard |
| `webmcp.rs` | Top-document same-origin WebMCP registry with document epochs and stale-handle rejection |
| `page_assets.rs` | Document-bound asset inventory, cached content, atomic local bundle/manifest output |
| `export.rs` | Bounded atomic local GSuite and YouTube transcript exports |
| `mcp/` | MCP server: JSON-RPC dispatch, 64 tool handlers, schema, profiles, resources/prompts, dual-era lifecycle |
| `cli.rs` | Shell commands reuse MCP dispatch; bounded JSON output and same-session JSONL batch; legacy debug REPL |
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

### Browser identity and pinned reconnect

The existence of many `codex-browser-use-*` pipes is normal: stale transport
names, multiple profiles, and multiple browser families can coexist. Pipe names
are not browser identity.

`registry.rs` probes every reachable pipe through `getInfo` and exposes
`metadata.extensionInstanceId` as `browser_id`, together with family, name,
version, profile, capabilities, and transport diagnostics. An unbound session
auto-selects only when exactly one live instance exists. Multiple instances
require `--browser-instance`, config/env pinning, or `codex_bind_browser`.

Reconnects are pinned to the selected extension instance and fail if that
instance disappears. They never rotate to another healthy Edge/Chrome/profile,
because doing so would silently change cookies, tabs, and user state.

### Sticky CDP attach
Each tab needs a CDP debugger session. `attached_tabs` caches known-attached
tabs to skip the detach+attach round-trip on repeated calls. A 20 s fast-timeout
on the sticky path falls through to a full re-attach if a background-throttled
tab goes silent, instead of burning the whole 60 s budget.

### Turn lifecycle and capability mapping

`codex_finalize` detaches cached CDP sessions and sends the official `turnEnded`
request. The extension decides tab cleanup and preserves tabs marked for
handoff or delivery. Closing a pipe alone is not successful turn cleanup.
Rebinding with owned tabs requires explicit `end_previous_turn=true` or prior
finalization; failed cleanup prevents switching browsers.

`codex_locator` provides strict selector/role/text/label/test-id operations,
while iframe and chained-locator behavior remains partial. Accessibility actions
use backend DOM IDs; AX-only nodes are reported as non-actionable. Method coverage
and app-private dependencies are maintained in [docs/official-parity.md](docs/official-parity.md).

Pointer clicks, CUA movement, scrolling, dragging and CSS convenience actions
share `pointer.rs`. A pressed button is released after a partial failure, and
state-changing CDP is not replayed. Current extension builds do not acknowledge
`mouseMoved`, so movement uses a bounded no-ack window; drag elapsed time follows
the number of supplied path points.

Shell `tools`, `call`, and `batch` reuse the same MCP registry and handlers.
Batch owns one session and stops on error; independent calls never share a
session. Optional `--finalize` attempts cleanup on success or failure. CLI output
has a separate byte budget and summarizes images instead of emitting base64.
This entrypoint does not change the parity boundary.

### CDP event subscription
Frames carrying a `method` and no `id` are server-pushed events. `read_loop`
routes the whole frame to subscribers matched by method prefix
(`network_monitor`, `console_logs`). Subscribers get the entire frame so they
can dispatch on the exact method themselves.

### Tool profiles
64 tools, filtered by profile — `basic` (45) / `network` (63) / `full` (64) —
via `CODEX_BRIDGE_PROFILE` env, `--profile` flag, or config file. Keeps the
tool list small for agents that don't need the network/debugging surface.

### Claude plugin distribution and progressive loading

The Claude plugin surface is intentionally small:

- `mcp.json` launches the npm package through a version-pinned `npx` entry.
  The first start downloads the package and platform binary; a global npm
  install is optional and keeps later starts on the existing command.
- `.claude-plugin/plugin.json` and `.claude-plugin/marketplace.json` make the
  repository a Claude Code plugin and marketplace. The manifest reuses root
  `mcp.json`; a second `.mcp.json` would make Claude auto-load the same server
  as project MCP and create a duplicate process.

The plugin does not bundle platform executables. `check-plugin-surface.js`
keeps the `npx` package pin equal to the plugin and npm versions.

There is deliberately no `plugin.json`, `.codex-plugin/`, or Codex marketplace.
Codex / ChatGPT desktop already controls its browser extension natively; the
bridge exists for Claude and other non-Codex MCP clients that need to reuse the
same logged-in browser session.

The plugin profile is `full` because Claude Code supports MCP tool search. It
loads server instructions and tool names first, then discovers individual
schemas on demand. `codex_doctor` alone carries
`_meta["anthropic/alwaysLoad"] = true` so connectivity recovery remains
immediate; eagerly loading more tools would defeat deferral. Manual MCP clients
that do not support tool search keep the `basic` profile as their documented
fallback.

The skill follows the same progressive-disclosure boundary. `SKILL.md` contains
only routing, the minimal workflow, and safety rules; task references for tabs,
DOM/input, network/session, official parity, and recovery are read one at a
time.

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

### Dual-era MCP lifecycle (`mcp/lifecycle.rs`)
The 2026-07-28 revision removed the `initialize` handshake and protocol
sessions; every request is stateless and carries its protocol version in
`_meta`. The bridge serves **both eras from one binary**:

- **Era detection is per request**: the presence of
  `_meta["io.modelcontextprotocol/protocolVersion"]` selects modern handling;
  anything else follows the legacy lifecycle. Legacy `_meta` (e.g.
  `progressToken`) without the version key stays legacy.
- **Legacy byte-compatibility is a hard requirement**: legacy envelopes pass
  through untouched; modern framing (`resultType`, `_meta` serverInfo,
  `ttlMs`/`cacheScope`) is applied only to requests that opted in. Tests pin
  this (`legacy_tools_list_stays_byte_compatible`).
- **Version negotiation**: `initialize` echoes a supported legacy revision or
  offers the newest one; a modern request asking for an unsupported revision
  gets `UnsupportedProtocolVersionError` (-32022) with the supported list.
  A modern version requested through `initialize` negotiates down to legacy
  semantics.
- **`server/discover`** is always answered, even without version metadata —
  that is the stdio compatibility probe.
- **`ping` is legacy-only**: the 2026-07-28 revision removed it, so modern
  requests get `-32601`.
- **Lenient `clientCapabilities`**: the schema requires the field on modern
  requests, but the bridge needs no client capabilities (no sampling /
  elicitation / roots), so a missing value is treated as empty instead of
  returning `MissingRequiredClientCapabilityError` (-32021).

## Data flow — a tool call

1. MCP client sends `tools/call` (e.g. `codex_navigate`) over stdio
2. `mcp/handlers.rs` dispatches to `handle_navigate`
3. `browser.rs::navigate` calls `client.execute_cdp(tab, "Page.navigate", …)`
4. `Client` attaches a CDP session (if not cached) and writes an `executeCdp`
   frame to the pipe
5. The ChatGPT browser extension runs the CDP command in the browser and returns a result frame
6. `read_loop` routes the response to the pending request's oneshot channel
7. `browser.rs` parses the result and returns `Content` to the MCP client

## Testing

- **Windows** (CI `rust` job): lib unit tests, integration tests, clippy
  `--all-targets`, coverage, x64 + arm64 release build
- **Linux GNU x64 / arm64** (source CI `test-lib` matrix): native full tests,
  clippy, release builds, Unix socket validation, plus reconnect/resource/prompt
  tests using `tokio::io::duplex()` injected mocks. CI configuration is not a
  completed CI run or live extension verification.
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

Tagging `v*` triggers `release.yml`: test → build Windows and Linux x64 + arm64 → checksums and
attestations → draft GitHub Release → npm publish (`--provenance`) → publish
the GitHub Release. The npm package embeds release checksums and downloads the
matching binary on install. CI and release also validate that all plugin
manifests, MCP files, marketplace entries, skill references, and npm versions
remain one coherent distribution surface.
