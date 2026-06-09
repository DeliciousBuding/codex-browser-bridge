# Rust Rewrite Subagent Plan

This plan is the handoff point for parallel work on `rewrite/rust-full`.

## Coordination Rules

- Work on the assigned files only.
- Do not revert changes outside your write set.
- Keep Go behavior as the compatibility baseline.
- Add Rust tests next to the module or under `tests/`.
- Report changed files and commands run.

## Worker Prompts

### worker-protocol

Own `src/protocol.rs` and `tests/protocol_parity.rs`.

Implement and test framed JSON-RPC parity:

- 4-byte little-endian length prefix
- max frame size `10 * 1024 * 1024`
- zero-length frame error
- request JSON shape with `jsonrpc`, `id`, `method`, `params`
- response parsing where `id` can be missing for notifications
- error formatting with newlines escaped

### worker-discovery

Own `src/discovery.rs`, `src/pipe.rs`, and `tests/discovery_parity.rs`.

Implement and test pipe discovery parity:

- `codex-browser-use-<uuid>`
- `codex-browser-use\<uuid>`
- ignore unrelated pipe names
- return `\\.\pipe\<name>` paths
- PowerShell enumeration timeout
- Windows named pipe dial timeout

### worker-client

Own `src/client.rs` and `tests/client_rpc.rs`.

Implement extension RPC behavior:

- pending response map
- writer serialization
- request timeout cleanup
- health check during auto-discovery
- per-tab CDP lock
- `detach -> attach -> executeCdp`
- one retry when `executeCdp` reports debugger not attached

### worker-browser

Own `src/browser.rs` and `tests/browser_api.rs`.

Implement high-level browser parity:

- tabs, user tabs, claim tab, close tab
- navigate, back, forward, reload, wait for load
- DOM snapshot with text fallback
- screenshot base64 extraction
- click, fill, evaluate
- CUA click, type, keypress, scroll
- DOM visible tree and DOM node click

### worker-mcp

Own `src/mcp.rs` and `tests/mcp_parity.rs`.

Implement MCP parity:

- initialize
- ping
- notifications ignored
- tools/list all existing tools
- tools/call handler outputs and error envelope
- image content for screenshot
- strict argument validation matching Go behavior

### worker-npm

Own `npm/`, `.github/workflows/`, `CHANGELOG.md`, and release notes.

Prepare packaging for Rust binaries:

- install script downloads Rust-built executable
- Windows x64 and arm64 asset naming
- checksum handling
- local npm install tests
- CI build matrix

### worker-docs

Own `README.md`, `README.zh-CN.md`, `CONTRIBUTING.md`, and `docs/rust-rewrite/`.

Update docs after implementation reaches parity. Keep language direct:

- state commands first
- lead with the project state, command, or decision
- keep each paragraph focused on one job
- use concrete nouns for build outputs, tests, release assets, and compatibility gates
