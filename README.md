<p align="center">
  <img src="assets/hero.png" alt="codex-browser-bridge" width="720">
</p>

<p align="center">
  <h1 align="center">codex-browser-bridge</h1>
  <p align="center">
    Let Claude Code and other MCP agents control your existing browser profile through the ChatGPT desktop app's browser extension.
    <br>64 MCP tools. Pure Rust. Single binary.
  </p>
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
  <a href="README.zh-CN.md">中文</a>
</p>

---

## What It Does

`codex-browser-bridge` turns your **local ChatGPT desktop app + browser extension** into an MCP server that any agent can control.

No browser profile copying. No WebDriver. No remote setup. It discovers the
`codex-browser-use` local transports that already exist on your machine, identifies
each live browser-extension instance, speaks the same JSON-RPC protocol, and
exposes 64 MCP tools for browser automation.

This README describes v1.12.0: 64 MCP tools, with Windows and Linux GNU
packages built from the same tagged source.

The goal is complete browser capability access for non-Codex agents. Coverage
is tracked by method and semantics in [the parity matrix](docs/official-parity.md);
a tool count or an alternative CLI does not establish full parity. App-private
services and remaining locator differences are explicitly recorded there.

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

Last live verification: 2026-10-24 with ChatGPT desktop app (`OpenAI.Codex`
package) `26.1016.8120.0` and Edge extension `1.26.1020.11845`.

| Surface | Status |
|---------|--------|
| ChatGPT desktop app | Current channel live-tested |
| ChatGPT browser extension | Current channel live-tested |
| Chrome / Edge | Supported; Edge live-tested in this audit |
| Brave / Opera / Vivaldi | Expected to use the same Chromium/CDP pipeline, but not yet live-tested |
| MCP `2024-11-05` … `2025-11-25` | Supported legacy lifecycle |
| MCP `2026-07-28` | Supported stateless lifecycle and discovery |
| Windows x64 | v1.12.0 passed the 18-check synthetic-tab Edge smoke; this is not full-surface parity |
| Windows arm64 | Published binary; not live-tested for this release |
| Linux GNU x64 | Full native Rust/Node gates, release build, and no-browser doctor passed on Ubuntu 24.04 / glibc 2.39; extension not live-tested |
| Linux GNU arm64 | `cargo build --release --locked` passed under arm64 QEMU; no arm64 runtime or extension live test |
| macOS | Unix code compilation alone does not establish runtime support; no published binary or live verification |

The bridge does not install a second browser extension or copy your browser
profile. One MCP server session controls one browser-extension instance at a
time; tabs from multiple browser profiles are not merged into a single list.

Linux source discovers `/tmp/codex-browser-use/*.sock`, checks private socket
ownership/permissions and peer UID, and accepts the official shared sticky
directory mode `1777` with socket mode `0600`. Doctor reports transport kind and
path while retaining legacy `pipe_*` fields for existing consumers. A Linux/WSL
process requires a socket in that environment; it does not discover a Windows
pipe across the OS boundary automatically.

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

The initial connection is fail-closed: if exactly one live instance exists it
is selected automatically; if several exist, browser tools return an ambiguity
error until the session is bound. Reconnects stay pinned to the selected
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

The plugin defaults to the full 64-tool surface because Claude Code enables MCP
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
client config. The suggested config starts with the 45-tool `basic` profile;
switch to `network` or `full` only when needed. File upload requires
`CODEX_BRIDGE_UPLOAD_BASE`; use the templates in [examples/](examples/) for a
full config.

Codex and ChatGPT desktop already control the ChatGPT browser extension
directly. Do not install this bridge as a Codex plugin: it would add a duplicate
MCP path to the same browser. The bridge exists for Claude Code and other
non-Codex MCP clients that need to reuse that existing browser session.

For Cursor, OpenClaw, Hermes Agent — see [examples/](examples/). The npm
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
2. config file
3. environment variables
4. built-in defaults

The default config file is `.codex-browser-bridge.toml` in the current working directory. Set `CODEX_BRIDGE_CONFIG` to use an explicit path; when it is set, the bridge does not fall back to the working-directory config:

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
- Both settings are clamped to an 8 MiB hard ceiling. Truncated text includes an
  explicit marker with the original byte count; oversized images return a text
  summary instead of invalid partial base64.
- The same limits can be set in `.codex-browser-bridge.toml` as
  `max_text_bytes` and `max_image_bytes`, or via CLI flags
  `--max-text-bytes` and `--max-image-bytes`.

`codex://tabs` is available as an MCP resource for clients that support resources. It returns tabs owned by the current bridge session, not every Chrome tab. The prompt templates `login` and `extract-table` are also exposed for clients that support MCP prompts.

## All 64 MCP Tools

### Browser Selection `[Browser]`
| Tool | Description |
|------|-------------|
| `codex_list_browsers` | List live extension instances with `browser_id`, family, profile, version, capabilities, and pipe transport |
| `codex_bind_browser` | Bind this session to one exact `browser_id`; family-only binding requires one live match |
| `codex_browser_history` | Read browser history with queries, date bounds, and a result limit |
| `codex_browser_management` | Use the official windows/tabs/tabGroups/bookmarks management capability and audit trail |
| `codex_browser_viewport` | Set or reset the official browser viewport override |
| `codex_browser_visibility` | Query or set browser visibility when the extension advertises that capability |

### Tab Management `[Tabs]`
| Tool | Description |
|------|-------------|
| `codex_list_tabs` | List tabs owned by this session |
| `codex_create_tab` | Create a new blank tab |
| `codex_close_tab` | Close a tab by ID |
| `codex_user_tabs` | List all browser tabs (including unclaimed) |
| `codex_claim_tab` | Claim an existing user tab |
| `codex_mark_tab` | Mark an owned tab as a deliverable or later-turn handoff |

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
| `codex_bring_to_front` | Activate a background tab (fixes screenshot timeouts) |
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

CUA pointer moves use a bounded no-ack window because current extension builds
apply but do not acknowledge `mouseMoved`; drag latency therefore follows the
number of supplied path points.

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

## CLI Usage

Additional current-source tools are `codex_clipboard` (session virtual
clipboard), `codex_webmcp` (experimental top-document registration list/invoke),
and `codex_content_export` (bounded local GSuite/YouTube export).

```bash
# MCP mode (default)
codex-browser-bridge --mode mcp

# List active pipes
codex-browser-bridge --mode discover

# Interactive REPL for debugging
codex-browser-bridge --mode cli

# With tool profiles
codex-browser-bridge --mode mcp --profile basic     # 45 tools
codex-browser-bridge --mode mcp --profile network   # 63 tools
codex-browser-bridge --mode mcp --profile full      # all 64 (default)

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
share one turn, or `--keep-turn` to deliberately leave the turn open. Ctrl-C and
SIGTERM attempt the same cleanup once. CLI JSON results default to 16384 bytes
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

All 64 tools carry behavior annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`) for client approval UX. See [CHANGELOG.md](CHANGELOG.md) for details.

## Maintenance

This project is intentionally a thin compatibility adapter, not a second browser runtime. Upstream desktop-app and extension upgrades do not require a bridge release by themselves. After an upstream upgrade, run this smoke sequence:

1. `codex-browser-bridge --mode doctor`
2. `codex_list_browsers`
3. `codex_bind_browser` when more than one instance is live
4. `codex_get_info`
5. Claim one disposable tab and read its title or DOM

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

Source layout:

```
src/
  mcp/          MCP server (mod, lifecycle, types, schema, handlers, profiles)
  browser.rs    CDP + browser operations
  client.rs     Local transport + sticky attach
  security.rs   URL + file path validation
  doctor.rs     Local transport diagnostics
  cli.rs        Shell commands and interactive debug REPL
  discovery.rs  Local transport auto-discovery
  protocol.rs   Length-prefixed JSON-RPC frames
```

## Roadmap

See [ROADMAP.md](ROADMAP.md). Highlights:

- optional live E2E harness for the ChatGPT desktop app + Chrome/Edge (`scripts/live-e2e.ps1`)
- typed tool result schemas
- Linux Unix socket transport and GNU x64/arm64 packages ship in v1.12.0; live extension verification remains pending. macOS needs a real runtime test.

The official capability matrix and drift baseline live in
[docs/official-parity.md](docs/official-parity.md) and
[docs/official-surface-baseline.json](docs/official-surface-baseline.json).

## Related

- [examples/](examples/) — MCP configs for Claude Code, Cursor, OpenClaw, Hermes Agent
- [.claude-plugin/](.claude-plugin/plugin.json) — Claude Code plugin and marketplace metadata
- [skills/codex-browser/](skills/codex-browser/SKILL.md) — Agent skill (LLM usage guide)
- [ROADMAP.md](ROADMAP.md) — Release history and roadmap
- [CHANGELOG.md](CHANGELOG.md) — Release history
- [CONTRIBUTING.md](CONTRIBUTING.md) — Dev setup and conventions
- [docs/release-process.md](docs/release-process.md) — Tag, changelog, GitHub Release, and npm publishing rules

## License

MIT. Maintained independently from OpenAI, Anthropic, and Google.

## Acknowledgments

Thanks to [LINUX DO](https://linux.do/) for community support and feedback.
