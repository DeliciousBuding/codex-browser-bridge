# ROADMAP

## Architecture and scope

The primary goal is to let external clients use Codex's installed native
runtimes through standard MCP. Follow the installed service's catalog and
permissions. Register Browser Use and Computer Use separately so each exposes
its original API; clients with only one installed service can register just that one.
Keep the direct extension adapter for clients that rely on its existing
structured tools and for installations without the Codex desktop runtime.

Detailed contracts and status matrices:
[docs/official-parity.md](docs/official-parity.md),
[docs/native-computer-use.md](docs/native-computer-use.md), and
[CHANGELOG.md](CHANGELOG.md).

## Delivered (`v2.0.0` – `v3.0.0`)

- **Native-first dual-runtime relay (`v3.0.0`)**: `--surface native-browser`
  relays the installed Codex `node_repl` Browser runtime alongside
  `--surface computer` (`cua_repl`) through one cancel-safe full-duplex JSON-RPC
  stdio transport; Claude plugin (`mcp.json`) and client templates (`examples/`)
  now register both native services by default, while the bundled skill routes
  concisely and delegates full instructions to each installed runtime.
- **Browser session & transport (`#40`, compatibility `--surface browser`)**:
  deterministic browser selection (`--pipe`, `--browser-family`,
  `--browser-instance`), OS single-owner locks, instance-bound CLI v2 session
  continuation, exact tab-claim snapshots, serialized rebind/finalize, and
  background focus emulation (`Emulation.setFocusEmulationEnabled`) on tab
  attach.
- **Browser observation, locators & files (`#40`, `v2.1.1`)**: opt-in per-tab
  observation with frame/loader state, `commit`/`domcontentloaded`/`load`/`networkidle`
  navigation waits, `url`/`url_glob`/safe-`url_regex` matchers, level-filtered
  event/log cursors, dialog/file-chooser IDs, `codex_list_tabs(selected_only=true)`
  (`Tabs.selected`), `codex_dom_snapshot(include_iframes=true)`,
  `codex_locator(action="element_info")`, padded/annotated
  `codex_screenshot_element`, viewport/full-page/bounded-`clip` screenshots,
  shared declarative locators with actionability/trial/force, trusted
  checked-state transitions, colocated file-chooser uploads, and atomic host
  file output.
- **Native Computer Use adapter (`#57`, `v2.1.0` – `v2.1.1`)**: host launch-row
  discovery, Codex CLI environment and Windows `PATH`/`PATHEXT` parity, unified
  cancel-safe full-duplex JSON-RPC stdio transport across `--mode mcp` and
  CLI/doctor (`call`, `batch`, `tools`, `--image-dir`),
  `enabled_tools`/`disabled_tools` filtering, `turn_ended` execution-context
  cleanup, and Windows JobObject / Unix process-group teardown.
- **Supply chain & distribution**: zero-exception `cargo-deny` gate (`rmcp`
  removed), checksummed Windows/Linux x64 and arm64 binaries with GitHub
  artifact attestations, npm Trusted Publishing, and Claude Code plugin
  packaging.

## Next iterations

### 1. Original Browser and Computer Use

Prioritize native launch, client approvals, session isolation and cleanup across
platforms. The compatibility matrix describes the existing extension tools;
behavior gaps there do not require a parallel implementation of native APIs.

### 2. Native Computer Use surface — runtime & platform coverage

Tracked in [issue #57](https://github.com/DeliciousBuding/codex-browser-bridge/issues/57):

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
