<p align="center">
  <img src="https://raw.githubusercontent.com/DeliciousBuding/codex-browser-bridge/main/assets/hero.png" alt="codex-browser-bridge" width="720">
</p>

<h1 align="center">codex-browser-bridge</h1>

<p align="center">
  Let Claude Code and other MCP agents control your existing browser profile through the ChatGPT desktop app's browser extension.
  <br>66 browser tools plus opt-in native Computer Use. Single Rust binary.
</p>

<p align="center">
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License">
  </a>
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/releases">
    <img src="https://img.shields.io/github/v/release/DeliciousBuding/codex-browser-bridge?style=flat-square" alt="Latest Release">
  </a>
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/actions">
    <img src="https://img.shields.io/github/actions/workflow/status/DeliciousBuding/codex-browser-bridge/ci.yml?style=flat-square" alt="CI">
  </a>
  <a href="https://codecov.io/gh/DeliciousBuding/codex-browser-bridge">
    <img src="https://img.shields.io/codecov/c/github/DeliciousBuding/codex-browser-bridge?style=flat-square" alt="Coverage">
  </a>
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/README.zh-CN.md">中文</a>
</p>

---

## What It Does

`codex-browser-bridge` turns your **local ChatGPT desktop app + browser extension** into an MCP server that any agent can control.

No browser profile copying. No WebDriver. No remote setup. It discovers the
`codex-browser-use` local transports that already exist on your machine, identifies
each live browser-extension instance, speaks the same JSON-RPC protocol, and
exposes 66 MCP tools for browser automation.

The browser surface has 66 MCP tools (`basic` 46 / `network` 65 /
`full` 66), distributed as Windows and Linux GNU packages.

Upgrading from 1.x? Read [the upgrade guide](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/upgrade-v2.md) before updating
an installation that uses saved continuation files or numeric-only tab claims.

The goal is complete browser capability access for non-Codex agents. Coverage
is tracked by method and semantics in [the parity matrix](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/official-parity.md),
including app-private services and remaining locator differences.

For desktop automation, `--surface computer` connects to the installed Codex
`cua_repl` MCP service: it preserves the host's launch configuration,
environment policy, and tool filters, forwards native application approval
prompts (`elicitation/create`), and cleans up the owned worker process tree on
exit. The default Claude plugin remains browser-only; see
[Native Computer Use](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/native-computer-use.md) and
[examples/native-computer-use.json](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/examples/native-computer-use.json) to opt in.

> **Naming note:** OpenAI now documents this surface as the **ChatGPT browser extension**. The underlying `codex-browser-use` pipe and JSON-RPC method names still use the Codex-era identifiers, so this project keeps its `codex_*` tool surface for compatibility.

**Your agent can:**

- Open, close, and switch browser tabs
- Navigate pages, go back/forward, wait for loads
- Capture screenshots (viewport PNG)
- Read DOM / accessibility trees (including ARIA role+name search)
- Click, type, scroll — via CSS selectors, coordinates, or accessibility node IDs
- Execute arbitrary JavaScript in the page context
- Upload files to `<input type=file>` elements
- Handle JavaScript dialogs (alert / confirm / prompt)
- Read and set browser cookies
- Run explicitly allowlisted CDP diagnostics (Chrome DevTools Protocol escape hatch)
- Self-diagnose with `codex_doctor`

Useful when an agent needs to work with pages that require a real browser session — dashboards, logged-in web apps, local dev servers, documentation sites.

## Quick Install

Claude Code plugin (recommended):

```bash
claude plugin install codex-browser --marketplace DeliciousBuding/codex-browser-bridge
```

The plugin fetches the pinned npm package and platform binary through `npx` on
first use. A global npm install is optional.

Standalone CLI or manual MCP configuration:

```bash
npm i -g @delicious233/codex-browser-bridge
```

Or download the binary from [GitHub Releases](https://github.com/DeliciousBuding/codex-browser-bridge/releases).

**Requires:** a local supported extension transport · Node.js 18+ for the npm/npx path · a supported Chromium browser · ChatGPT desktop app/browser-extension service

## Compatibility

Last full browser audit: 2026-10-24 with ChatGPT desktop app (`OpenAI.Codex`
package) `26.1016.8120.0` and Edge extension `1.26.1020.11845`.
Native Computer Use verification is tracked separately in the
[native guide](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/native-computer-use.md).

| Surface | Status |
|---------|--------|
| ChatGPT desktop app | Current channel live-tested |
| ChatGPT browser extension | Current channel live-tested |
| Chrome / Edge | Supported; Edge live-tested in this audit |
| Brave / Opera / Vivaldi | Expected to use the same Chromium/CDP pipeline, but not yet live-tested |
| MCP `2024-11-05` … `2025-11-25` | Supported legacy lifecycle |
| MCP `2026-07-28` | Supported stateless lifecycle and discovery |
| Windows x64 | Current source passes the synthetic Edge parity smoke; not full-surface parity |
| Windows arm64 | Published binary; not live-tested for this release |
| Linux GNU x64 | Full native Rust/Node gates, release build, and no-browser doctor passed on Ubuntu 24.04 / glibc 2.39; extension not live-tested |
| Linux GNU arm64 | Native Rust tests/clippy/release and synthetic Chrome extension parity smoke passed; not full-surface parity |
| macOS | Source builds and CI tests pass; no prebuilt release binary |

The 2026-10-24 Windows Edge and Linux arm64 Chrome runs passed the 20-check
parity smoke and the 9-check observation/locator/file-result smoke. File chooser
capture and permission-denial cleanup passed; successful file upload requires
the extension's file-access permission and was not accepted in these runs.

The bridge does not install a second browser extension or copy your browser
profile. One MCP server session controls one browser-extension instance at a
time; tabs from multiple browser profiles are not merged into a single list.

Linux source discovers `CODEX_BRIDGE_SOCKET_DIR` when set, otherwise
`$XDG_RUNTIME_DIR/codex-browser-use`, then `/tmp/codex-browser-use`. It checks
private socket ownership/permissions and peer UID, and accepts the official
shared sticky directory mode `1777` with socket mode `0600`. Doctor reports
transport kind and path while retaining legacy `pipe_*` fields for existing
consumers. A Linux/WSL process requires a socket in that environment; it does
not discover a Windows pipe across the OS boundary automatically.

### Browser identity and selection

`Chrome` and `Edge` share the same Chromium automation backend, but they are
not interchangeable instances. Two Edge profiles can also have the same
`name` and `family` while exposing different tabs, cookies, extensions, and
signed-in state.

The installed `browser` and `chrome` Codex plugin packages also share the same
Browser client/service implementation; their manifests are product wrappers.
`browserFamily` chooses the extension/profile backend, not a separate engine.

The bridge therefore treats `metadata.extensionInstanceId` from `getInfo` as
the stable `browser_id`. A pipe name is transport metadata, not browser
identity. Extensions that do not report an instance ID are excluded from
discovery; use `--pipe` only as an explicit legacy transport override.

Use:

1. `codex_list_browsers` to inspect every live instance.
2. `codex_bind_browser` with an exact `browser_id` when more than one instance
   is live.

The initial connection requires a complete inventory for automatic and
family-only selection. Known disconnected sockets and app-private IAB/MCP Apps
routes are ignored. If a probe times out or an endpoint remains unidentified,
retry discovery or bind an exact `browser_id`: a single response does not
prove only one browser is running. A complete inventory with multiple matching
instances also requires an exact ID. Reconnects stay pinned to the selected
`extensionInstanceId`; they never rotate silently to another browser/profile.

For process-level pinning, use `--browser-family` plus
`--browser-instance`, `CODEX_BRIDGE_BROWSER_FAMILY` plus
`CODEX_BRIDGE_BROWSER_INSTANCE`, or the matching TOML keys. A family selector is
accepted only when that family has one live instance. `--pipe` remains an
explicit low-level transport override and cannot be combined with a browser
selector.

Rebinding while the current session owns tabs requires `codex_finalize` first,
or explicit `end_previous_turn: true` on `codex_bind_browser`. This performs the
official previous-turn cleanup before switching; it does not silently abandon
the old session. Finalization preserves handoff/deliverable marks.

## 30-Second Setup (Claude Code)

The one-command install above adds the skill and MCP server configuration
together; no manual MCP JSON is needed. Claude Code 2.1.292 or later can add the
marketplace and install the plugin in one step. For a local checkout, use
`claude --plugin-dir .`.

The first MCP start can take a few seconds while `npx` fills its cache. After
that, the plugin reuses the cached package, or the globally installed command
when one is already on `PATH`.

The plugin defaults to the full 66-tool surface because Claude Code enables MCP
tool search by default: only server instructions and tool names enter the
initial context, and schemas are loaded on demand. `codex_doctor` is the only
schema marked `alwaysLoad`. The companion skill then loads one reference file
for the task instead of reading the entire tool manual.

For clients without tool search, or if you prefer manual MCP wiring, install
the npm package globally and add:

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["--mode", "mcp"],
      "transport": "stdio",
      "env": {
        "CODEX_BRIDGE_PROFILE": "basic"
      }
    }
  }
}
```

Restart Claude Code. Then ask:

```
List my open browser tabs.
Open https://example.com and take a screenshot.
Find the login button and click it.
```

If your MCP client is launched from a GUI or a scheduler, prefer the absolute
`command` path printed by npm `postinstall`. If you need to recover it later,
run the out-of-band CLI doctor from npm's global install path:

```powershell
$bridge = Join-Path (npm prefix -g) "codex-browser-bridge.cmd"
& $bridge --mode doctor
```

Paste `install.suggested_mcp_config.mcpServers.codex-browser.command` into the
client config. The suggested config starts with the 46-tool `basic` profile;
switch to `network` or `full` only when needed. File upload requires
`CODEX_BRIDGE_UPLOAD_BASE`; use the templates in [examples/](examples/) for a
full config.

Codex and ChatGPT desktop already control the ChatGPT browser extension
directly. Do not install this bridge as a Codex plugin: it would add a duplicate
MCP path to the same browser. The bridge exists for Claude Code and other
non-Codex MCP clients that need to reuse that existing browser session.

For Antigravity, Cursor, OpenClaw, Hermes Agent — see [examples/](examples/). The npm
package also ships these examples under `examples/`.

### Install the agent skill only (fallback)

The plugin already installs the skill. Clients without plugin support can copy
the npm-package skill instead:

```bash
# Git Bash / WSL: global install → Claude Code skills
cp -r "$(npm root -g)/@delicious233/codex-browser-bridge/skills/codex-browser" ~/.claude/skills/

# Git Bash / WSL: or into a local project
cp -r node_modules/@delicious233/codex-browser-bridge/skills/codex-browser .claude/skills/
```

```powershell
# PowerShell: global install -> Claude Code skills
New-Item -ItemType Directory -Force "$env:USERPROFILE\.claude\skills" | Out-Null
Copy-Item -Recurse -Force "$env:APPDATA\npm\node_modules\@delicious233\codex-browser-bridge\skills\codex-browser" "$env:USERPROFILE\.claude\skills\"

# PowerShell: or into a local project
New-Item -ItemType Directory -Force ".claude\skills" | Out-Null
Copy-Item -Recurse -Force "node_modules\@delicious233\codex-browser-bridge\skills\codex-browser" ".claude\skills\"
```

**👤 For agents reading this:** to self-install, run the copy command above
against your own skills directory. The skill is at
`skills/codex-browser/SKILL.md` inside the installed npm package, and
multi-client MCP config templates are under `examples/`.

## Configuration

Configuration precedence is:

1. CLI flags
2. environment variables
3. config file
4. built-in defaults

The default config file is `.codex-browser-bridge.toml` in the current working directory. Set `CODEX_BRIDGE_CONFIG` to use an explicit path; when it is set, the bridge does not fall back to the working-directory config:

Only an absent default file is optional. Explicit unreadable/missing files,
malformed TOML, unknown keys, empty selectors, invalid tool profiles and response
limits outside 1024..=8388608 bytes fail before browser work. Family and instance
form one selector group: the highest-priority source specifying either replaces
the whole lower-priority group. `--mode doctor` reports the effective selector
and source. `basic/network/full` select tools, not browser user profiles.

```toml
profile = "basic"                # recommended; basic | network | full
browser_family = "edge"          # chrome | edge | brave | opera | vivaldi
browser_instance = "..."         # extensionInstanceId; required if >1 match
upload_base = "C:/Users/me/Downloads"
max_text_bytes = 1048576
max_image_bytes = 3145728
```

The same settings can be provided in MCP client config:

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["--mode", "mcp", "--profile", "network"],
      "transport": "stdio",
      "env": {
        "CODEX_BRIDGE_BROWSER_FAMILY": "edge",
        "CODEX_BRIDGE_BROWSER_INSTANCE": "extension-instance-id",
        "CODEX_BRIDGE_UPLOAD_BASE": "C:\\Users\\me\\Downloads",
        "CODEX_BRIDGE_MAX_TEXT_BYTES": "1048576",
        "CODEX_BRIDGE_MAX_IMAGE_BYTES": "3145728"
      }
    }
  }
}
```

`CODEX_BRIDGE_UPLOAD_BASE` enables `codex_file_input` and limits uploads to a specific directory. File upload is disabled until this is set explicitly; MCP clients do not always launch servers from a predictable working directory.

Large MCP responses are bounded so agents do not receive multi-megabyte DOM,
JavaScript, CDP, or screenshot payloads by accident:

- `CODEX_BRIDGE_MAX_TEXT_BYTES` caps each text content item. Default: `1048576`.
- `CODEX_BRIDGE_MAX_IMAGE_BYTES` caps each base64 image content item. Default: `3145728`.
- Values outside `1024..=8388608` are rejected at startup. Truncated text
  includes an explicit marker with the original byte count; oversized images
  return a text summary instead of invalid partial base64.
- The same limits can be set in `.codex-browser-bridge.toml` as
  `max_text_bytes` and `max_image_bytes`, or via CLI flags
  `--max-text-bytes` and `--max-image-bytes`. `network_policy` is the TOML key
  for the attach-time network policy below.
- `CODEX_BRIDGE_NETWORK_POLICY` (or `--network-policy`) sets the Browser Use
  network policy sent with every tab attach. The value is inline JSON in the
  extension's own shape: `{"timeout_ms":1..30000,"constraints":[{"kind":"domains"|"origins","mode":"allowlist"|"denylist","patterns":[...]}]}`.
  Omit it for the documented empty policy (allow-all, no rules installed). It is
  validated at startup and again before each attach, because a policy the
  extension rejects fails the attach after the debugger was already detached.
  The policy is installed per attached tab; changing it applies to attaches made
  after the change, not to tabs already attached.
- `CODEX_BRIDGE_SOCKET_DIR` overrides Unix socket discovery. Without it,
  `$XDG_RUNTIME_DIR/codex-browser-use` is tried before `/tmp/codex-browser-use`.
- Attachments enable `Emulation.setFocusEmulationEnabled`, matching the native
  Codex Browser client. Input and screenshot calls do not activate tabs
  or focus browser windows. If setup fails, no input is sent and the error is
  reported. `codex_bring_to_front` is available for an explicit request to view
  a tab. The former `CODEX_BRIDGE_NO_AUTO_FOREGROUND` workaround is no longer
  needed and has no effect.
- Scrolling keeps the native extension's `Input.synthesizeScrollGesture` path.
  Some background renderers delay that gesture and its acknowledgment. A
  timeout is an unknown outcome: inspect the page before another scroll, since
  the original gesture can arrive later. The bridge never retries the input or
  activates the tab automatically. Browser debug indicators and page dialogs
  can still be visible; desktop automation has separate focus requirements.
- Background screenshots can still time out or lose a renderer attachment.
  Focus emulation is not a guarantee of capture availability; errors are reported.

`codex://tabs` is available as an MCP resource for clients that support resources. It returns tabs owned by the current bridge session, not every Chrome tab. The prompt templates `login` and `extract-table` are also exposed for clients that support MCP prompts.

## All 66 MCP Tools

### Browser Selection `[Browser]`
| Tool | Description |
|------|-------------|
| `codex_list_browsers` | List live extension instances with `browser_id`, family, profile, version, capabilities, and pipe transport |
| `codex_bind_browser` | Bind this session to one exact `browser_id`; family-only binding requires one live match |
| `codex_browser_history` | Read browser history with queries, date bounds, and a result limit |
| `codex_browser_management` | Use the official windows/tabs/tabGroups/bookmarks management capability and audit trail; remove/delete/clear/close/discard require `confirm=true` |
| `codex_browser_viewport` | Set or reset the official browser viewport override |
| `codex_browser_visibility` | Query or set browser visibility when the extension advertises that capability |

### Tab Management `[Tabs]`

`codex_observe` starts bounded tab/document observation before an action and
returns frames, dialog IDs, event/log cursors and explicit gaps. Navigation
composites arm before acting. `codex_file_chooser` captures chooser IDs/multiple;
uploads require configured `CODEX_BRIDGE_UPLOAD_BASE` and explicit
`colocated_browser=true`. Remote browser-host uploads and completed-download paths
are unsupported. File exports report `path_scope=bridge_host`, not browser
downloads. See [parity contracts](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/official-parity.md).

| Tool | Description |
|------|-------------|
| `codex_list_tabs` | List tabs owned by this session |
| `codex_create_tab` | Create a new blank tab |
| `codex_close_tab` | Close a tab by ID |
| `codex_user_tabs` | List all browser tabs (including unclaimed) |
| `codex_claim_tab` | Claim a user tab with expected title/URL from a fresh enumeration |
| `codex_mark_tab` | Mark an owned tab as a deliverable or later-turn handoff |
| `codex_observe` | Arm bounded tab/document observation and return frames, dialog cursors, events/log cursors and explicit gaps |
| `codex_file_chooser` | Capture a modal file chooser and its document-bound ID; uploads still require the configured upload root and a colocated browser |

### Navigation `[Navigation]`
| Tool | Description |
|------|-------------|
| `codex_navigate` | Navigate to URL |
| `codex_reload` | Reload current page |
| `codex_navigate_back` | Go back one history entry |
| `codex_navigate_forward` | Go forward one history entry |
| `codex_wait_for_load` | Poll `document.readyState` until complete |
| `codex_nav_and_wait` | Navigate + wait (1 call instead of 2) |
| `codex_wait_for_element` | Poll a CSS selector until it matches (SPAs) |
| `codex_wait_for_url` | Poll until URL contains a substring (SPA routes) |

### DOM & Accessibility `[DOM]`
| Tool | Description |
|------|-------------|
| `codex_dom_snapshot` | Full accessibility tree; actions use `backendDOMNodeId`, not AX `nodeId`; large responses may be truncated |
| `codex_dom_get_visible` | Human-readable visible DOM tree |
| `codex_dom_click` | Click by accessibility node ID |
| `codex_find_element` | Find by ARIA role + name; `node_id` is a backend DOM ID, and AX-only entries have `actionable=false` |
| `codex_click_element` | Click element from `codex_find_element` result |
| `codex_locator` | Strict CSS/role/text/label/placeholder/test-id reads and actions, including double-click; iframe/chained-locator parity remains partial |

### Page Inspection `[Page]`
| Tool | Description |
|------|-------------|
| `codex_get_url` | Current tab URL |
| `codex_get_title` | Current page title |
| `codex_evaluate` | Execute unrestricted page JavaScript and return bounded JSON; bridge-specific, not official Playwright read-only evaluate |
| `codex_page_assets` | List page resources; optionally fetch bounded known-size content |
| `codex_console_logs` | Capture console output for a window |
| `codex_emulate_device` | Emulate mobile viewport (`reset=true` to clear) |
| `codex_screenshot` | Capture viewport screenshot; oversized images return a summary |
| `codex_screenshot_element` | Capture a single element by selector; oversized images return a summary |
| `codex_print_pdf` | Render page to PDF via bounded CDP stream; returns a size summary, not embedded PDF bytes |
| `codex_bring_to_front` | Explicitly activate a tab when the user wants to view it |
| `codex_dialog` | Handle alert / confirm / prompt |
| `codex_performance_metrics` | DOM nodes, JS heap, event listeners (Performance) |

### Input & Interaction `[Input]`
| Tool | Description |
|------|-------------|
| `codex_click` | Resolve the first CSS match and click with a trusted CDP pointer |
| `codex_fill` | Fill input by CSS selector |
| `codex_hover` | Resolve a CSS match and move a trusted CDP pointer over it |
| `codex_select_option` | Select an exact existing option through the shared locator engine |
| `codex_drag` | CUA path drag through all supplied points; legacy from/to maps to two points |
| `codex_cua_click` | CUA coordinate click with button `1..5`, held modifiers, and `click_count=2` double-click |
| `codex_cua_move` | CUA pointer move with optional held modifiers |
| `codex_cua_type` | Type text at current focus |
| `codex_cua_keypress` | Press key sequence (Enter, Ctrl+C, etc.) |
| `codex_cua_scroll` | CUA scroll with `scrollX/scrollY` and optional modifiers |
| `codex_click_and_wait` | Click + wait for load (1 call) |
| `codex_form_fill` | Fill multiple fields from `{selector: value}` map |
| `codex_file_input` | Upload files to `<input type=file>` |

CUA pointer moves require an acknowledgment within the bounded operation window
and are never replayed; drag latency follows the number of supplied path points.

### Network `[Network]`
| Tool | Description |
|------|-------------|
| `codex_network_cookies` | Read cookies (values redacted by default) |
| `codex_network_set_cookie` | Set a browser cookie |
| `codex_delete_cookies` | Delete cookies by name |
| `codex_storage` | Get/set localStorage |
| `codex_network_monitor` | Pair request↔response into structured list |

### CDP Escape Hatch `[CDP]`
| Tool | Description |
|------|-------------|
| `codex_execute_cdp` | Execute explicitly allowlisted CDP diagnostics |

### Session `[Session]`
| Tool | Description |
|------|-------------|
| `codex_name_session` | Name the current session |
| `codex_finalize` | Send official `turnEnded`; extension cleanup preserves marked handoff/deliverable tabs |
| `codex_get_info` | Get bridge runtime + extension backend metadata |
| `codex_doctor` | Self-diagnostics (pipe health, latency, version) |

### Bridge State & Artifacts
| Tool | Description |
|------|-------------|
| `codex_clipboard` | Session-virtual clipboard with bounded data and paste integration |
| `codex_webmcp` | Experimental top-document registration list and invocation |
| `codex_content_export` | Bounded local GSuite/YouTube exports with bridge-host path scope |

## CLI Usage

```bash
# MCP mode (default)
codex-browser-bridge --mode mcp

# List active pipes
codex-browser-bridge --mode discover

# Interactive REPL for debugging
codex-browser-bridge --mode cli

# With tool profiles
codex-browser-bridge --mode mcp --profile basic     # 46 tools
codex-browser-bridge --mode mcp --profile network   # 65 tools
codex-browser-bridge --mode mcp --profile full      # all 66 (default)

# Bound large MCP outputs
codex-browser-bridge --mode mcp --max-text-bytes 1048576 --max-image-bytes 3145728
```

The source also provides shell-friendly commands through the same schemas,
profiles, handlers, and safety checks. `tools` lists names without connecting;
`tools NAME` returns one schema.

```bash
codex-browser-bridge tools codex_locator
codex-browser-bridge call codex_list_browsers '{}'
codex-browser-bridge --max-output-bytes 4096 batch <<'JSONL'
{"name":"codex_user_tabs","arguments":{}}
JSONL
```

A `call` is one process and one browser turn. It ends that turn on exit, even
after a failure, so an unmarked tab can never be left owned by a process that no
longer exists; `deliverable` and `handoff` marks survive. `batch` reads one
`{name, arguments}` object per stdin line, keeps one turn, and stops on the
first error. Use `--session-file <path>` when several separate commands must
share one turn, or `--keep-turn` to deliberately leave the turn open.
`--session-file` is valid only with `call`/`batch`; MCP stdio and the interactive
CLI reject it. The v2 file binds the turn to an exact extension instance and is
locked for each invocation; concurrent use fails promptly. Its parent must exist.
Legacy/corrupt files fail rather than create a new session. Preserve such files
and recover/finalize the original turn before starting a new continuation.
The empty `.lock` sidecar is retained to prevent inode races; process exit releases
the OS lock. Same-profile cookies/storage are still shared. Ctrl-C and SIGTERM attempt cleanup only when the command is configured to finalize;
`--keep-turn` and an unfinalized `--session-file` intentionally leave the turn
open. CLI JSON results default to 16384 bytes
each and summarize images without base64; this byte limit is separate from MCP
content limits. The CLI is an optional entrypoint, not a separate capability
implementation.

## Architecture

```
MCP Client (Claude Code / Cursor / OpenClaw)
        │ stdio JSON-RPC
        ▼
codex-browser-bridge (Rust binary)
        │ length-prefixed JSON-RPC frames
        ▼
Local transport (Windows named pipe / Linux Unix socket)
        │
        ▼
ChatGPT desktop app → ChatGPT browser extension → Chromium browser tabs
```

## MCP Protocol Support

The bridge is a **dual-era MCP server**:

| Era | Revisions | Behavior |
|-----|-----------|----------|
| Legacy | `2024-11-05` … `2025-11-25` | Classic stdio lifecycle with `initialize` version negotiation (byte-compatible with older clients) |
| Modern | `2026-07-28` | Stateless requests: version carried per-request in `_meta`, `server/discover` probe, `resultType` + cache hints (`ttlMs`/`cacheScope`) on results, `UnsupportedProtocolVersionError` (-32022) |

All 66 tools carry behavior annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`) for client approval UX. See [CHANGELOG.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/CHANGELOG.md) for details.

## Maintenance

This project is intentionally a thin compatibility adapter, not a second browser runtime. Upstream desktop-app and extension upgrades do not require a bridge release by themselves. After an upstream upgrade, run this smoke sequence:

1. `codex-browser-bridge --mode doctor`
2. `codex_list_browsers`
3. `codex_bind_browser` when more than one instance is live
4. `codex_get_info`
5. `codex_user_tabs`
6. Claim one disposable tab and read its title or DOM

Release a patch only when the pipe contract changes, compatibility needs documenting, or a real regression is fixed. Do not add Playwright or another browser extension as a fallback layer.

## Security

This tool gives an agent access to your active browser session.

- Never expose to a network port
- Only run for trusted MCP clients
- Review agent actions before allowing sensitive operations
- Avoid using on pages with passwords, payments, or admin consoles
- Redact tab titles, URLs, DOM text, screenshots before sharing output
- `codex_file_input` enforces path traversal prevention (canonicalize + prefix check, 10 MB limit)
- Navigation only accepts `http://` and `https://` URLs
- Cookie values redacted by default; raw CDP is allowlist-protected and blocks sensitive browser, target, debugger, navigation, cookie, screenshot, PDF, file upload, event-producing enable calls, arbitrary Runtime JS, page-resource content, and destructive storage operations

## Development

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge

cargo check --locked
cargo test --locked
cargo clippy --locked -- -D warnings
cargo build --locked --release
```

See [ARCHITECTURE.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/ARCHITECTURE.md)
for the current module map and design boundaries.

## Roadmap

See [ROADMAP.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/ROADMAP.md). Highlights:

- optional live E2E harness for the ChatGPT desktop app + Chrome/Edge (`scripts/live-e2e.ps1`)
- typed tool result schemas
- Linux Unix socket transport and GNU x64/arm64 packages have shipped since v1.12.0; Linux arm64 synthetic extension smoke is verified, not full-surface parity. macOS needs a real runtime test.

The official capability matrix and drift baseline live in
[docs/official-parity.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/official-parity.md) and
[docs/official-surface-baseline.json](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/official-surface-baseline.json).

## Related

- [examples/](https://github.com/DeliciousBuding/codex-browser-bridge/tree/main/examples) — MCP configs for Claude Code, Antigravity, Cursor, OpenClaw, Hermes Agent
- [.claude-plugin/](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/.claude-plugin/plugin.json) — Claude Code plugin and marketplace metadata
- [skills/codex-browser/](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/skills/codex-browser/SKILL.md) — Agent skill (LLM usage guide)
- [ROADMAP.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/ROADMAP.md) — Release history and roadmap
- [CHANGELOG.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/CHANGELOG.md) — Release history
- [CONTRIBUTING.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/CONTRIBUTING.md) — Dev setup and conventions
- [docs/release-process.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/release-process.md) — Tag, changelog, GitHub Release, and npm publishing rules

## License

MIT. Maintained independently from OpenAI, Anthropic, and Google.

## Acknowledgments

Thanks to [LINUX DO](https://linux.do/) for community support and feedback.
