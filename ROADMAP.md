# ROADMAP

## Architecture and scope

`codex-browser-bridge` is a thin transport and protocol adapter with two explicit
surfaces:

- **`--surface browser` (default)**: connects directly to the Codex Browser Use
  Chromium extension native-messaging endpoint (`\\.\pipe\codex-browser-use\*` or
  `/tmp/codex-browser-use/*.sock`) and exposes 66 structured `codex_*` MCP tools
  and CLI commands without requiring the Codex desktop app.
- **`--surface computer`**: discovers and launches the installed Codex `cua_repl`
  MCP service (`[mcp_servers.cua_repl]` or bundled `unified-computer-use`),
  relaying `js`, `js_reset`, `js_add_node_module_dir`, `turn_ended`, and
  bidirectional `elicitation/create` app approvals while owning worker
  process-tree cleanup.

Detailed contracts and status matrices:
[docs/official-parity.md](docs/official-parity.md),
[docs/native-computer-use.md](docs/native-computer-use.md), and
[CHANGELOG.md](CHANGELOG.md).

## Delivered (`v2.0.0` – `v2.1.1`)

- **Browser session & transport (`#40`)**: deterministic browser selection
  (`--pipe`, `--browser-family`, `--browser-instance`), OS single-owner locks,
  instance-bound CLI v2 session continuation, exact tab-claim snapshots,
  serialized rebind/finalize, and background focus emulation
  (`Emulation.setFocusEmulationEnabled`) on tab attach.
- **Browser observation, locators & files (`#40`)**: opt-in per-tab observation
  with frame/loader state, dialog/file-chooser IDs, bounded event/log cursors,
  shared declarative locators with actionability/trial/force, trusted
  checked-state transitions, viewport/full-page/bounded-`clip` screenshots,
  colocated file-chooser uploads, and atomic host file output.
- **Native Computer Use adapter (`#57`, `v2.1.0` – `v2.1.1`)**: host launch-row
  discovery, Codex CLI environment and Windows `PATH`/`PATHEXT` parity, unified
  cancel-safe JSON-RPC stdio transport across `--mode mcp` and CLI/doctor
  (`call`, `batch`, `tools`, `--image-dir`), `enabled_tools`/`disabled_tools`
  filtering, `turn_ended` execution-context cleanup, and Windows JobObject /
  Unix process-group teardown.
- **Supply chain & distribution**: zero-exception `cargo-deny` gate (`rmcp`
  removed), checksummed Windows/Linux x64 and arm64 binaries with GitHub
  artifact attestations, npm Trusted Publishing, and Claude Code plugin
  packaging.

## Next iterations

### 1. Browser surface — official `26.1111.21308` parity gaps

Tracked against [docs/official-parity.md](docs/official-parity.md):

- **`Tabs.selected`**: resolve the currently active owned tab directly instead
  of inferring from `codex_list_tabs`.
- **Element inspection & annotated capture**: `PlaywrightAPI.elementInfo` and
  `PlaywrightAPI.elementScreenshot` (CDP DOM hit-testing, backend node IDs,
  selector generation, and annotated bounding-box screenshots).
- **Frame-expanded DOM & OOPIF routing**: `PlaywrightAPI.domSnapshot`
  iframe-expanded text snapshots, cross-origin/OOPIF frame coordinate routing,
  and fuller ARIA accessible-name computation.
- **Navigation & log filters**: `waitForLoadState("networkidle")`, glob/regex
  matchers in `waitForURL`, and level-filtered console/page error reads.

### 2. Native Computer Use surface — runtime & platform coverage

Tracked in [issue #57](https://github.com/DeliciousBuding/codex-browser-bridge/issues/57):

- **Relay full-duplex write queueing**: decouple client-to-native and
  native-to-client writes so a backpressured peer cannot stall the opposite
  direction up to `WRITE_TIMEOUT` (10s).
- **Desktop interaction edge cases**: physical held-input release verification
  on interrupt, user takeover during active runs, and multi-display / DPI /
  clipboard coverage on Windows x64 and Linux X11.
- **Platform acceptance matrix**: live GUI verification on Windows ARM64,
  Linux x64, and Linux Wayland.

## Out of scope

- Reimplementing OpenAI's proprietary GUI executor (`@oai/sky` / `cua-sparkle`),
  app-private OAuth/Guardian tokens, cloud takeover, telemetry, or remote
  artifact storage.
- Backend-authorized browser download/upload streaming (`BrowserUseAPI` host
  endpoints that depend on Codex app-private backend services).
- Prebuilt macOS release binaries until a Darwin GUI/TCC endpoint is verified.
- Automatic cross-OS pipe/socket bridging between WSL and Windows.
