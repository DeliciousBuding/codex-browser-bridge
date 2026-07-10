<p align="center">
  <img src="assets/hero.png" alt="codex-browser-bridge" width="720">
</p>

<p align="center">
  <h1 align="center">codex-browser-bridge</h1>
  <p align="center">
    Let Claude Code and other MCP agents control your existing Chrome browser through Codex Desktop's browser bridge.
    <br>52 MCP tools. Pure Rust. Single binary. Zero config.
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

`codex-browser-bridge` turns your **local Codex Desktop + Chrome** into an MCP server that any agent can control.

No browser profile copying. No WebDriver. No remote setup. It connects to the Codex browser named pipe that already exists on your machine, speaks the same JSON-RPC protocol, and exposes 52 MCP tools for browser automation.

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

```bash
npm i -g @delicious233/codex-browser-bridge
```

Or download from [GitHub Releases](https://github.com/DeliciousBuding/codex-browser-bridge/releases).

**Requires:** Windows · Chrome · Codex Desktop · Codex Chrome Extension

## 30-Second Setup (Claude Code)

Add to your Claude Code MCP settings:

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["--mode", "mcp"],
      "transport": "stdio"
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
client config. File upload requires `CODEX_BRIDGE_UPLOAD_BASE`; use the
templates in [examples/](examples/) for a full config.

For Cursor, OpenClaw, Hermes Agent — see [examples/](examples/). The npm
package also ships these examples under `examples/`.

### Install the agent skill (recommended)

The npm package includes a skill that teaches agents how to use all 52
tools effectively. After install, copy it to your skills directory:

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
profile = "full"                 # basic | network | full
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

## All 52 MCP Tools

### Tab Management `[Tabs]`
| Tool | Description |
|------|-------------|
| `codex_list_tabs` | List tabs owned by this session |
| `codex_create_tab` | Create a new blank tab |
| `codex_close_tab` | Close a tab by ID |
| `codex_user_tabs` | List all browser tabs (including unclaimed) |
| `codex_claim_tab` | Claim an existing user tab |

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
| `codex_dom_snapshot` | Full accessibility tree with node IDs; large responses may be truncated |
| `codex_dom_get_visible` | Human-readable visible DOM tree |
| `codex_dom_click` | Click by accessibility node ID |
| `codex_find_element` | Find elements by ARIA role + name |
| `codex_click_element` | Click element from `codex_find_element` result |

### Page Inspection `[Page]`
| Tool | Description |
|------|-------------|
| `codex_get_url` | Current tab URL |
| `codex_get_title` | Current page title |
| `codex_evaluate` | Execute JavaScript, return bounded JSON result |
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
| `codex_click` | Click by CSS selector (JS click) |
| `codex_fill` | Fill input by CSS selector |
| `codex_hover` | Hover over element (dropdowns, tooltips) |
| `codex_select_option` | Set `<select>` value + fire change |
| `codex_drag` | CDP mouse drag (sliders, sortable lists) |
| `codex_cua_click` | Click at exact coordinates (CDP mouse events) |
| `codex_cua_type` | Type text at current focus |
| `codex_cua_keypress` | Press key sequence (Enter, Ctrl+C, etc.) |
| `codex_cua_scroll` | Scroll at coordinates by delta |
| `codex_click_and_wait` | Click + wait for load (1 call) |
| `codex_form_fill` | Fill multiple fields from `{selector: value}` map |
| `codex_file_input` | Upload files to `<input type=file>` |

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
| `codex_finalize` | Clean up tabs, release resources |
| `codex_get_info` | Get bridge runtime + extension backend metadata |
| `codex_doctor` | Self-diagnostics (pipe health, latency, version) |

## CLI Usage

```bash
# MCP mode (default)
codex-browser-bridge --mode mcp

# List active pipes
codex-browser-bridge --mode discover

# Interactive REPL for debugging
codex-browser-bridge --mode cli

# With tool profiles
codex-browser-bridge --mode mcp --profile basic     # 34 tools
codex-browser-bridge --mode mcp --profile network   # 51 tools
codex-browser-bridge --mode mcp --profile full      # all 52 (default)

# Bound large MCP outputs
codex-browser-bridge --mode mcp --max-text-bytes 1048576 --max-image-bytes 3145728
```

## Architecture

```
MCP Client (Claude Code / Cursor / OpenClaw)
        │ stdio JSON-RPC
        ▼
codex-browser-bridge (Rust binary)
        │ length-prefixed JSON-RPC frames
        ▼
Windows Named Pipe \\.\pipe\codex-browser-use-*
        │
        ▼
Codex Desktop → Chrome Extension → Chrome tabs
```

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
  mcp/          MCP server (mod, types, schema, handlers, profiles)
  browser.rs    CDP + browser operations
  client.rs     Named pipe transport + sticky attach
  security.rs   URL + file path validation
  doctor.rs     Pipe diagnostics
  cli.rs        Interactive debug REPL
  discovery.rs  Pipe auto-discovery
  protocol.rs   Length-prefixed JSON-RPC frames
```

## Roadmap

See [ROADMAP.md](ROADMAP.md). Highlights:

- winget and scoop manifests
- optional live E2E harness for Codex Desktop + Chrome (`scripts/live-e2e.ps1`)
- typed tool result schemas
- v2.0.0: Cross-platform (macOS / Linux via Unix domain sockets)

## Related

- [examples/](examples/) — MCP configs for Claude Code, Cursor, OpenClaw, Hermes Agent
- [skills/codex-browser/](skills/codex-browser/SKILL.md) — Agent skill (LLM usage guide)
- [ROADMAP.md](ROADMAP.md) — Full roadmap with SUPER scores
- [CHANGELOG.md](CHANGELOG.md) — Release history
- [CONTRIBUTING.md](CONTRIBUTING.md) — Dev setup and conventions
- [docs/release-process.md](docs/release-process.md) — Tag, changelog, GitHub Release, and npm publishing rules

## License

MIT. Maintained independently from Codex / Anthropic / Google.

## Acknowledgments

Thanks to [LINUX DO](https://linux.do/) for community support and feedback.
