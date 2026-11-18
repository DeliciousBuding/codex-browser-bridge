<p align="center">
  <img src="https://raw.githubusercontent.com/DeliciousBuding/codex-browser-bridge/main/assets/hero.png" alt="codex-browser-bridge" width="720">
</p>

<h1 align="center">codex-browser-bridge</h1>

<p align="center">
  Unlock <b>Codex Browser Use</b> and <b>Codex Native Computer Use</b> for Claude Code, Antigravity, Cursor, OpenClaw, Hermes Agent, and shell scripts.
  <br>One Rust binary: 66 <code>codex_*</code> tools over your logged-in Chromium session, plus the installed <code>cua_repl</code> desktop runtime.
</p>

<p align="center">
  <a href="https://deliciousbuding.github.io/codex-browser-bridge/"><b>Website</b></a> ·
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/README.zh-CN.md">中文</a> ·
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License"></a>
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/releases"><img src="https://img.shields.io/github/v/release/DeliciousBuding/codex-browser-bridge?style=flat-square" alt="Latest Release"></a>
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/actions"><img src="https://img.shields.io/github/actions/workflow/status/DeliciousBuding/codex-browser-bridge/ci.yml?style=flat-square" alt="CI"></a>
  <a href="https://codecov.io/gh/DeliciousBuding/codex-browser-bridge"><img src="https://img.shields.io/codecov/c/github/DeliciousBuding/codex-browser-bridge?style=flat-square" alt="Coverage"></a>
</p>

---

## Two Surfaces, One Binary

`codex-browser-bridge` is a thin MCP adapter with two entry points. The browser surface reuses the Chromium session you are already signed into. The computer surface reuses the Codex desktop runtime already installed on the machine. No WebDriver, no profile copying, no second browser extension.

| | `--surface browser` (default) | `--surface computer` (opt-in) |
|---|---|---|
| Transport | Direct Chromium extension native-messaging pipe/socket (`codex-browser-use`). No Codex desktop app process required. | Launches and relays the installed Codex `cua_repl` service (`[mcp_servers.cua_repl]` or `unified-computer-use`). |
| Interface | 66 declarative MCP tools (`basic` 46 / `network` 65 / `full` 66) plus CLI (`call`, `batch`, `--session-file`). | Stateful `js` REPL: `cua.listApps()`, `cua.listWindows()`, `cua.getApp({windowId})`, `cua.listBrowsers()`, `cua.getTab()`, `getAXStateAndScreenshot()`, `click()`, `typeText()`, `pressKey()`, `scroll()`, `drag()`. |
| Behavior | Background focus emulation (`Emulation.setFocusEmulationEnabled`); input and screenshots do not steal window focus. | Forwards native `elicitation/create` app approval prompts; `turn_ended` plus owned process-tree cleanup (Windows Job Object / Unix process group). |

Use the browser surface for web work: tabs, navigation, DOM/AX trees, trusted clicks and typing, screenshots, uploads, dialogs, cookies, network capture, and allowlisted CDP diagnostics against pages that need your real session. Use the computer surface for desktop apps and cross-application flows. They run as separate MCP servers and do not share a turn.

## Quick Start

### Claude Code plugin

```bash
claude plugin install codex-browser --marketplace DeliciousBuding/codex-browser-bridge
```

One command installs the skill and the MCP server; no JSON editing. The plugin starts on the 66-tool full profile because Claude Code loads schemas on demand through tool search, and `codex_doctor` is the only always-loaded schema. The first start pulls the pinned npm package through `npx`; later starts reuse the cache or a global binary.

The plugin registers the browser surface only. Add `codex-computer` from the config below to enable Computer Use.

### Manual MCP configuration

```bash
npm i -g @delicious233/codex-browser-bridge   # or download a release binary
```

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["--mode", "mcp"],
      "env": { "CODEX_BRIDGE_PROFILE": "basic" }
    },
    "codex-computer": {
      "command": "codex-browser-bridge",
      "args": ["--surface", "computer", "--mode", "mcp"]
    }
  }
}
```

Clients without tool search should start at the 46-tool `basic` profile; `network` adds cookies/network tools, `full` adds the remainder. Templates for Antigravity, Cursor, OpenClaw, and Hermes Agent live in [examples/](https://github.com/DeliciousBuding/codex-browser-bridge/tree/main/examples).

### CLI

```bash
codex-browser-bridge call codex_list_browsers '{}'
codex-browser-bridge call codex_navigate '{"tab_id":"1","url":"https://example.com"}'
codex-browser-bridge --surface computer call js '{"code":"await cua.listWindows();"}'
codex-browser-bridge --surface computer call js '{"code":"await cua.getTab();"}'
```

Every `call` is one process and one browser turn, finalized on exit. Use `--session-file` to continue one turn across commands, or `batch` to run JSONL steps on a single turn.

**Requires:** the ChatGPT browser extension with its local `codex-browser-use` service · an installed Codex desktop runtime for the computer surface · Node.js 18+ for the npm/npx path.

## Compatibility

Last full browser audit: 2026-10-24, ChatGPT desktop app (`OpenAI.Codex` package) `26.1016.8120.0` with Edge extension `1.26.1020.11845`. Computer Use verification is tracked in [docs/native-computer-use.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/native-computer-use.md).

| Surface | Status |
|---------|--------|
| ChatGPT desktop app + browser extension | Current channel, live-tested |
| Chrome / Edge | Supported; Edge live-tested in this audit |
| Brave / Opera / Vivaldi | Same Chromium/CDP pipeline expected, not live-tested |
| MCP `2024-11-05` … `2025-11-25` | Legacy lifecycle supported |
| MCP `2026-07-28` | Stateless lifecycle and discovery supported |
| Windows x64 / arm64 | Published binaries; x64 passes the synthetic Edge parity smoke |
| Linux GNU x64 / arm64 | Published binaries; native gates and synthetic extension smokes pass, not full-surface parity |
| macOS | Source builds and CI tests pass; no prebuilt binary |

One MCP session controls one extension instance at a time; tabs from different browser profiles are not merged. Linux discovers `CODEX_BRIDGE_SOCKET_DIR`, then `$XDG_RUNTIME_DIR/codex-browser-use`, then `/tmp/codex-browser-use`, and validates socket ownership, mode, and peer UID. A Linux/WSL process needs a socket in that environment; it does not find a Windows pipe across the OS boundary.

## Browser Selection

- `codex_list_browsers` shows every live instance; `codex_bind_browser` pins one exact `browser_id` (`metadata.extensionInstanceId`). Pipe names are transport metadata, not identity.
- Automatic or family-only selection needs a complete inventory with exactly one live match. Timeouts and unidentified endpoints fail closed.
- Reconnects stay pinned to the selected instance. Rebinding while tabs are owned requires `codex_finalize` first or `end_previous_turn: true`.
- Process-level pinning: `--browser-family` / `--browser-instance`, the matching `CODEX_BRIDGE_*` variables, or TOML keys. `--pipe` is a low-level override and cannot be combined with a selector.

## Configuration

Precedence: CLI flags > environment variables > config file > built-in defaults. The default file is `.codex-browser-bridge.toml` in the working directory; `CODEX_BRIDGE_CONFIG` pins an explicit path and disables that fallback.

```toml
profile = "basic"                # basic | network | full
browser_family = "edge"          # chrome | edge | brave | opera | vivaldi
browser_instance = "..."         # extensionInstanceId; required when >1 match
upload_base = "C:/Users/me/Downloads"
max_text_bytes = 1048576
max_image_bytes = 3145728
```

- Only an absent default file is optional. Unreadable files, malformed TOML, unknown keys, empty selectors, invalid profiles, and limits outside `1024..=8388608` fail before any browser work. `--mode doctor` reports the effective selector and its source.
- `CODEX_BRIDGE_UPLOAD_BASE` enables `codex_file_input` and confines uploads to that directory (canonicalized prefix check, 10 MB limit). Uploads stay disabled until it is set.
- `CODEX_BRIDGE_MAX_TEXT_BYTES` / `CODEX_BRIDGE_MAX_IMAGE_BYTES` bound each MCP content item. Truncated text carries the original byte count; oversized images return a text summary.
- `CODEX_BRIDGE_NETWORK_POLICY` (or `--network-policy`) sends the Browser Use network policy with every attach. Inline JSON: `{"timeout_ms":1..30000,"constraints":[{"kind":"domains"|"origins","mode":"allowlist"|"denylist","patterns":[...]}]}`. Omit it for allow-all. The policy applies to later attaches, not tabs already attached.
- Attachments enable focus emulation, so input and screenshots do not activate tabs or focus windows. `codex_bring_to_front` is the explicit way to show a tab. Scrolling keeps `Input.synthesizeScrollGesture`; a timeout is an unknown outcome, so inspect the page before scrolling again.
- `codex://tabs` exposes the session's owned tabs as an MCP resource. The `login` and `extract-table` prompts cover the two common workflows.

## All 66 MCP Tools

Profiles: `basic` 46 tools, `network` 65 tools, `full` 66 tools. Every tool carries MCP behavior annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`).

### Browser Selection `[Browser]`

| Tool | Description |
|------|-------------|
| `codex_list_browsers` | List live instances with `browser_id`, family, profile, version, capabilities, and transport |
| `codex_bind_browser` | Bind the session to one exact `browser_id`; family-only binding needs a single live match |
| `codex_browser_history` | Read history with queries, date bounds, and a limit |
| `codex_browser_management` | Official windows/tabs/tabGroups/bookmarks management with an audit trail; destructive calls require `confirm=true` |
| `codex_browser_viewport` | Set or reset the official viewport override |
| `codex_browser_visibility` | Query or set browser visibility when the extension advertises it |

### Tabs `[Tabs]`

`codex_observe` arms bounded tab/document observation before an action and returns frames, dialog IDs, event/log cursors, and explicit gaps. `codex_file_chooser` captures chooser IDs and `multiple`; uploads need the configured root and `colocated_browser=true`.

| Tool | Description |
|------|-------------|
| `codex_list_tabs` | List owned tabs; `selected_only=true` resolves the active owned tab (`Tabs.selected`) |
| `codex_create_tab` | Create a blank tab |
| `codex_close_tab` | Close a tab by ID |
| `codex_user_tabs` | List all browser tabs, including unclaimed ones |
| `codex_claim_tab` | Claim a user tab with expected title/URL from a fresh enumeration |
| `codex_mark_tab` | Mark an owned tab as a deliverable or a later-turn handoff |
| `codex_observe` | Arm bounded observation; `wait_until=networkidle`, `url_glob`/`url_regex` for `wait_navigation`, `levels` filters logs |
| `codex_file_chooser` | Capture a modal file chooser and its document-bound ID |

### Navigation `[Navigation]`

| Tool | Description |
|------|-------------|
| `codex_navigate` | Navigate to a URL |
| `codex_reload` | Reload the page |
| `codex_navigate_back` | Go back one history entry |
| `codex_navigate_forward` | Go forward one history entry |
| `codex_wait_for_load` | Wait for `document.readyState` to complete |
| `codex_nav_and_wait` | Navigate and wait in one call |
| `codex_wait_for_element` | Poll a CSS selector until it matches |
| `codex_wait_for_url` | Poll until an exact `url`, substring `pattern`, `url_glob`, or `url_regex` matches |

### DOM & Accessibility `[DOM]`

| Tool | Description |
|------|-------------|
| `codex_dom_snapshot` | Accessibility tree; `include_iframes=true` adds bounded same-origin frame text; actions use `backendDOMNodeId` |
| `codex_dom_get_visible` | Human-readable visible DOM tree |
| `codex_dom_click` | Click by accessibility node ID |
| `codex_find_element` | Find by ARIA role and name; AX-only entries report `actionable=false` |
| `codex_click_element` | Click an element returned by `codex_find_element` |
| `codex_locator` | Strict CSS/role/text/label/test-id reads and actions with `and`/`or`/`has`/`has_not`, `element_info`, double-click, `force`, and `trial`; same-origin frames only |

### Page Inspection `[Page]`

| Tool | Description |
|------|-------------|
| `codex_get_url` | Current tab URL |
| `codex_get_title` | Current page title |
| `codex_evaluate` | Run page JavaScript and return bounded JSON; bridge-specific, not Playwright's read-only evaluate |
| `codex_page_assets` | List page resources; optionally fetch bounded cached content |
| `codex_console_logs` | Capture console output for a window |
| `codex_emulate_device` | Emulate a mobile viewport (`reset=true` clears) |
| `codex_screenshot` | Capture the viewport; oversized images return a summary |
| `codex_screenshot_element` | Capture one element with optional `padding` (0..256 CSS px) and `include_info=true` for `elementInfo` metadata |
| `codex_print_pdf` | Render through a bounded CDP stream; returns a size summary, not PDF bytes |
| `codex_bring_to_front` | Explicitly activate a tab for viewing |
| `codex_dialog` | Handle alert / confirm / prompt |
| `codex_performance_metrics` | DOM nodes, JS heap, and event listener counts |

### Input & Interaction `[Input]`

CUA pointer moves need an acknowledgment inside the bounded operation window and are never replayed. Drag time follows the number of path points.

| Tool | Description |
|------|-------------|
| `codex_click` | Resolve the first CSS match and click with a trusted CDP pointer |
| `codex_fill` | Fill an input by CSS selector |
| `codex_hover` | Move a trusted CDP pointer over the first CSS match |
| `codex_select_option` | Select an exact existing option through the shared locator engine |
| `codex_drag` | Drag through every supplied CUA path point; legacy `from`/`to` maps to two points |
| `codex_cua_click` | CUA coordinate click with button `1..5`, held modifiers, and `click_count=2` |
| `codex_cua_move` | CUA pointer move with optional held modifiers |
| `codex_cua_type` | Type text at the current focus |
| `codex_cua_keypress` | Press a key sequence such as Enter or Ctrl+C |
| `codex_cua_scroll` | CUA scroll with `scrollX`/`scrollY` and optional modifiers |
| `codex_click_and_wait` | Click and wait for load in one call |
| `codex_form_fill` | Fill multiple fields from a `{selector: value}` map |
| `codex_file_input` | Upload files to `<input type=file>` |

### Network `[Network]`

| Tool | Description |
|------|-------------|
| `codex_network_cookies` | Read cookies; values are redacted by default |
| `codex_network_set_cookie` | Set a cookie |
| `codex_delete_cookies` | Delete cookies by name |
| `codex_storage` | Get or set localStorage |
| `codex_network_monitor` | Pair requests and responses into a structured list |

### CDP Escape Hatch `[CDP]`

| Tool | Description |
|------|-------------|
| `codex_execute_cdp` | Run explicitly allowlisted CDP diagnostics |

### Session `[Session]`

| Tool | Description |
|------|-------------|
| `codex_name_session` | Name the current session |
| `codex_finalize` | Send the official `turnEnded`; cleanup preserves marked handoff/deliverable tabs |
| `codex_get_info` | Read bridge runtime and extension backend metadata |
| `codex_doctor` | Self-diagnostics for transport health, latency, and version |

### Bridge State & Artifacts

| Tool | Description |
|------|-------------|
| `codex_clipboard` | Session-virtual clipboard with bounded data and paste integration |
| `codex_webmcp` | Experimental top-document registration list and invocation |
| `codex_content_export` | Bounded local GSuite/YouTube exports with `bridge_host` path scope |

## CLI

```bash
codex-browser-bridge --mode mcp                      # MCP server over stdio
codex-browser-bridge --mode discover                 # list live transports
codex-browser-bridge --mode cli                      # interactive REPL
codex-browser-bridge tools codex_locator             # one schema, no connection
codex-browser-bridge --mode doctor                   # diagnostics
```

`call` and `batch` reuse the same schemas, profiles, handlers, and safety checks as MCP. `batch` reads one `{name, arguments}` object per stdin line and stops on the first error. `--session-file <path>` shares one turn across separate commands: the v2 file binds the turn to an exact extension instance, takes an OS lock per invocation, and fails rather than resetting a legacy or corrupt file. `--keep-turn` deliberately leaves a turn open. CLI JSON results default to 16384 bytes and summarize images instead of emitting base64; that budget is separate from the MCP content limits.

## Architecture

```
MCP client (Claude Code / Cursor / OpenClaw)
        │ stdio JSON-RPC
        ▼
codex-browser-bridge (Rust binary)
        │ length-prefixed JSON-RPC frames
        ▼
codex-browser-use pipe/socket  ──▶  ChatGPT extension  ── CDP ──▶  Chromium tabs
        │ (--surface computer)
        ▼
cua_repl worker  ──▶  desktop apps + browser targets
```

Upgrading from 1.x? Read [docs/upgrade-v2.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/upgrade-v2.md) before updating an installation with saved continuation files or numeric-only tab claims.

## MCP Protocol Support

- **Legacy** (`2024-11-05` … `2025-11-25`): classic stdio lifecycle with `initialize` version negotiation, byte-compatible with older clients.
- **Modern** (`2026-07-28`): stateless requests with the version in `_meta`, `server/discover`, `resultType`, and `ttlMs`/`cacheScope` hints. Unsupported versions return `-32022`.
- `codex://tabs`, `login`, and `extract-table` are exposed for clients that support MCP resources and prompts.

## Maintenance

The bridge is a compatibility adapter, not a second browser runtime. Upstream desktop-app or extension upgrades do not require a bridge release by themselves. After an upgrade, run:

1. `codex-browser-bridge --mode doctor`
2. `codex_list_browsers`
3. `codex_bind_browser` when more than one instance is live
4. `codex_get_info`
5. `codex_user_tabs`
6. Claim a disposable tab and read its title or DOM

Ship a patch only for a real pipe-contract change or a fixed regression. Do not add Playwright or another extension as a fallback. The capability matrix lives in [docs/official-parity.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/official-parity.md) and the drift baseline in [docs/official-surface-baseline.json](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/official-surface-baseline.json).

## Security

This tool acts inside your live browser and desktop session.

- Never expose it on a network port; run it only for trusted MCP clients.
- Review agent actions before allowing sensitive operations; avoid password, payment, and admin pages.
- Redact tab titles, URLs, DOM text, and screenshots before sharing output.
- `codex_file_input` canonicalizes paths, enforces the configured root, and caps uploads at 10 MB.
- Navigation accepts only `http://` and `https://`. Cookie values are redacted by default. Raw CDP is allowlist-only and blocks browser, target, debugger, navigation, cookie, screenshot, PDF, upload, event-enable, arbitrary Runtime JS, page-resource content, and destructive storage operations.

## Development

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
cargo check --locked
cargo test --locked
cargo clippy --locked -- -D warnings
cargo build --locked --release
```

Read [ARCHITECTURE.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/ARCHITECTURE.md) for the module map and design boundaries, and [CONTRIBUTING.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/CONTRIBUTING.md) for conventions. Release steps are in [docs/release-process.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/release-process.md).

## Related

- [examples/](https://github.com/DeliciousBuding/codex-browser-bridge/tree/main/examples) — MCP configs for Claude Code, Antigravity, Cursor, OpenClaw, Hermes Agent, and native Computer Use
- [skills/codex-browser/](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/skills/codex-browser/SKILL.md) — progressive agent skill
- [ARCHITECTURE.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/ARCHITECTURE.md) — module map and design decisions
- [ROADMAP.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/ROADMAP.md) · [CHANGELOG.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/CHANGELOG.md)
- [docs/official-parity.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/official-parity.md) — capability matrix and drift baseline

## License

MIT. Maintained independently from OpenAI, Anthropic, and Google.

Thanks to [LINUX DO](https://linux.do/) for community support and feedback.
