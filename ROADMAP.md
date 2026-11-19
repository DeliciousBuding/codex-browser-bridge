# ROADMAP

## Architecture and scope

The goal is to let external clients use Codex's installed native runtimes
through standard MCP. The bridge discovers the selected launch row, owns one
worker, and relays MCP frames unchanged. Follow the installed service's catalog
and permissions. Register Browser Use and Computer Use separately so each
exposes its original API; clients with only one installed service can register
just that one.

The relay is intentionally thin. Browser automation, desktop control, DOM and
accessibility handling, locators, and observation belong to the installed
runtime; the bridge does not implement a parallel surface.

Current contracts and verification status:
[docs/native-computer-use.md](docs/native-computer-use.md) and
[CHANGELOG.md](CHANGELOG.md).

## Delivered (`v2.0.0` – `v3.0.2`)

- **Native-only relay (`v3.0.1`)**: `--surface native-browser` (default)
  relays the installed Codex `node_repl` Browser runtime and `--surface
  computer` relays `cua_repl` through one cancel-safe full-duplex JSON-RPC
  stdio transport. The Claude plugin (`mcp.json`) and client templates
  (`examples/`) register both native services; the bundled skill routes
  concisely and delegates full instructions to each installed runtime.
- **Relay contract**: host-owned handshake and pagination, launch-row tool
  filters (`enabled_tools` before `disabled_tools`), Codex correlation
  metadata fill-in, `elicitation/create` approval passthrough, bounded frame
  and backlog limits, and `turn_ended` cleanup on EOF/`SIGINT`/`SIGTERM`.
- **Native Computer Use adapter (`#57`, `v2.1.0` – `v2.1.1`)**: host launch-row
  discovery, Codex CLI environment and Windows `PATH`/`PATHEXT` parity, unified
  cancel-safe transport across `--mode mcp` and CLI/doctor (`call`, `batch`,
  `tools`, `--image-dir`), and Windows JobObject / Unix process-group teardown.
- **Removed in 3.0.1**: the legacy Chromium extension adapter, its structured
  `codex_*` tools and profiles, the interactive REPL, session files, the
  method-parity matrix, and the agent-surface drift checker.
- **Supply chain & distribution**: zero-exception `cargo-deny` gate, checksummed
  Windows/Linux/macOS x64 and arm64 binaries with GitHub artifact attestations, npm
  Trusted Publishing, and Claude Code plugin packaging.
- **Native alignment (`v3.0.2`)**: acknowledged turn cleanup, structured CLI
  errors and legacy protocol validation, native CUA entry selection and tab
  retention, and explicit receiving-host token/UI boundaries.

## Next iterations

### 1. Original Browser and Computer Use

Prioritize native launch, client approvals, session isolation and cleanup across
platforms. Behavior gaps in the installed runtimes do not require a parallel
implementation in this repository.

### 2. Native Computer Use surface — runtime & platform coverage

Tracked in [issue #57](https://github.com/DeliciousBuding/codex-browser-bridge/issues/57):

- **Desktop interaction edge cases**: physical held-input release verification
  on interrupt, user takeover during active runs, and multi-display / DPI /
  clipboard coverage on Windows x64 and Linux X11.
- **Platform acceptance matrix**: live GUI verification on Windows ARM64,
  Linux x64, Linux Wayland, and macOS GUI/TCC.

## Out of scope

- Reimplementing OpenAI's proprietary GUI executor (`@oai/sky` / `cua-sparkle`),
  app-private OAuth/Guardian tokens, cloud takeover, telemetry, or remote
  artifact storage.
- Any bridge-side CDP, DOM, locator, observation, or permission layer that
  duplicates the installed runtime.
- Backend-authorized browser download/upload streaming (`BrowserUseAPI` host
  endpoints that depend on Codex app-private backend services).
- Automatic cross-OS bridging between WSL and Windows.
