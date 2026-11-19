# codex-browser-bridge

MCP server and CLI for reusing the installed Codex **Browser Use** (`node_repl`) and **Computer Use** (`cua_repl`) runtimes from Claude Code, Antigravity, Cursor, OpenClaw, Hermes Agent, and shell scripts, with a compatibility Chromium extension adapter (`--surface browser`).

[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/LICENSE)
[![Latest Release](https://img.shields.io/github/v/release/DeliciousBuding/codex-browser-bridge?style=flat-square)](https://github.com/DeliciousBuding/codex-browser-bridge/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/DeliciousBuding/codex-browser-bridge/ci.yml?style=flat-square)](https://github.com/DeliciousBuding/codex-browser-bridge/actions)
[![Coverage](https://img.shields.io/codecov/c/github/DeliciousBuding/codex-browser-bridge?style=flat-square)](https://codecov.io/gh/DeliciousBuding/codex-browser-bridge)

[Website](https://deliciousbuding.github.io/codex-browser-bridge/) · [中文文档](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/README.zh-CN.md) · [Native Setup](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/native-computer-use.md) · [Upgrading to 3.0](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/upgrade-v3.md)

## Choose an entry

| Entry | Runtime | Use it for |
|---|---|---|
| `--surface native-browser` | Codex `node_repl` | Installed Codex Browser/Chrome plugin JavaScript API (`agent.browsers`, `tab.playwright`, `tab.dom_cua`). |
| `--surface computer` | Codex `cua_repl` | Installed Computer Use `cua` API for desktop windows and unified browser targets. |
| `--surface browser` | Chromium extension pipe | Compatibility only: structured `codex_*` tools for existing 2.x workflows or hosts without native Codex runtimes. |

See [Native setup](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/native-computer-use.md) for launch discovery, host environment policy, and elicitation approvals. Browser Use and Computer Use have separate MCP registrations and worker state.

## Quick Start

### Claude Code plugin

```bash
claude plugin install codex-browser --marketplace DeliciousBuding/codex-browser-bridge
```

The plugin registers both native MCP services (`codex-browser` and `codex-computer`). Install and configure the native Codex runtimes first; the plugin does not bundle them. If only one runtime is installed on your machine, the other entry failing to start is expected—use a single-entry manual configuration if you only want one.

### Manual MCP configuration

```bash
npm i -g @delicious233/codex-browser-bridge   # or download a release binary
```

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["--surface", "native-browser", "--mode", "mcp"]
    },
    "codex-computer": {
      "command": "codex-browser-bridge",
      "args": ["--surface", "computer", "--mode", "mcp"]
    }
  }
}
```

Use only the entry you need if a client needs one service or if only one runtime is installed. Client templates are in [examples/](https://github.com/DeliciousBuding/codex-browser-bridge/tree/main/examples).

### CLI

```bash
codex-browser-bridge --surface native-browser --mode doctor
codex-browser-bridge --surface computer --mode doctor
codex-browser-bridge --surface computer call js '{"code":"await cua.listWindows();"}'
```

Use `batch` for several native JS calls that share one worker. Use `--surface browser` explicitly for an existing `codex_*` script; bare CLI invocations without `--surface` keep `--surface browser` as their default for backwards compatibility.

**Requires:** the configured native runtime for each native entry · Node.js 18+ for the npm/npx path.

## Upgrading

- **From 2.x to 3.0:** read [docs/upgrade-v3.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/upgrade-v3.md) (plugin and client templates switched from `codex_*` to native `node_repl` + `cua_repl`).
- **From 1.x to 2.x:** read [docs/upgrade-v2.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/upgrade-v2.md).

## Compatibility

Last live extension audit: 2026-10-24, ChatGPT desktop app (`OpenAI.Codex` package) `26.1016.8120.0` with Edge extension `1.26.1020.11845` (source/protocol parity baseline pinned to `26.1111.21308`). Native `node_repl` and `cua_repl` verification is tracked in [docs/native-computer-use.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/native-computer-use.md).

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

## Compatibility Extension Adapter (`--surface browser`)

The sections below apply **only** when explicitly running `--surface browser` (the direct Chromium extension pipe adapter). Native `node_repl` and `cua_repl` entries ignore `.codex-browser-bridge.toml` and `CODEX_BRIDGE_*` browser profiles.

### Browser Selection

- `codex_list_browsers` shows every live instance; `codex_bind_browser` pins one exact `browser_id` (`metadata.extensionInstanceId`). Pipe names are transport metadata, not identity.
- Automatic or family-only selection needs a complete inventory with exactly one live match. Timeouts and unidentified endpoints fail closed.
- Reconnects stay pinned to the selected instance. Rebinding while tabs are owned requires `codex_finalize` first or `end_previous_turn: true`.
- Process-level pinning: `--browser-family` / `--browser-instance`, the matching `CODEX_BRIDGE_*` variables, or TOML keys. `--pipe` is a low-level override and cannot be combined with a selector.

### Configuration (`--surface browser` only)

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

### Compatibility MCP Tools (`--surface browser`)

Profiles: `basic` 46 tools, `network` 65 tools, `full` 66 tools. Every tool carries MCP behavior annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`).

<details>
<summary>Expand all 66 compatibility tools</summary>

#### Browser Selection `[Browser]`

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

</details>

### Compatibility CLI (`--surface browser`)

```bash
codex-browser-bridge --surface browser --mode mcp          # MCP server over stdio
codex-browser-bridge --surface browser --mode discover     # list live extension pipes
codex-browser-bridge --surface browser --mode cli          # legacy interactive REPL
codex-browser-bridge --surface browser tools codex_locator # one schema, no connection
codex-browser-bridge --surface browser --mode doctor       # pipe diagnostics
```

`call` and `batch` reuse the same schemas, profiles, handlers, and safety checks as MCP. `batch` reads one `{name, arguments}` object per stdin line and stops on the first error. `--session-file <path>` shares one turn across separate commands: the v2 file binds the turn to an exact extension instance, takes an OS lock per invocation, and fails rather than resetting a legacy or corrupt file. `--keep-turn` deliberately leaves a turn open. CLI JSON results default to 16384 bytes and summarize images instead of emitting base64; that budget is separate from the MCP content limits.

## Architecture

See [Architecture](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/ARCHITECTURE.md) for the shared native stdio relay and separate compatibility extension transport.

## MCP Protocol Support (`--surface browser`)

- **Legacy** (`2024-11-05` … `2025-11-25`): classic stdio lifecycle with `initialize` version negotiation, byte-compatible with older clients.
- **Modern** (`2026-07-28`): stateless requests with the version in `_meta`, `server/discover`, `resultType`, and `ttlMs`/`cacheScope` hints. Unsupported versions return `-32022`.
- `codex://tabs`, `login`, and `extract-table` are exposed for clients that support MCP resources and prompts.
- Native relays (`--surface native-browser` and `--surface computer`) forward MCP frames directly without normalizing or re-negotiating the host handshake.

## Maintenance

Upstream Codex desktop app or browser extension upgrades do not require a bridge release by themselves. After an upgrade, verify the native relays first:

1. `codex-browser-bridge --surface native-browser --mode doctor`
2. `codex-browser-bridge --surface computer --mode doctor`

If you also use the `--surface browser` compatibility adapter, run `codex-browser-bridge --surface browser --mode doctor`, check `codex_list_browsers` / `codex_user_tabs`, and claim a disposable tab to read its title or DOM.

Ship a patch only for a real transport/launcher contract change or a fixed regression. The compatibility capability matrix lives in [docs/official-parity.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/official-parity.md) and the drift baseline in [docs/official-surface-baseline.json](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/official-surface-baseline.json).

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
