<p align="center">
  <img src="assets/hero.png" alt="codex-browser-bridge" width="720">
</p>

<p align="center">
  <h1 align="center">codex-browser-bridge</h1>
  <p align="center">
    Let Claude Code and other MCP agents control your existing Chrome browser through Codex Desktop's browser bridge.
    <br>48 MCP tools. Pure Rust. Single binary. Zero config.
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

No browser profile copying. No WebDriver. No remote setup. It connects to the Codex browser named pipe that already exists on your machine, speaks the same JSON-RPC protocol, and exposes 48 MCP tools for browser automation.

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
- Run raw CDP commands (Chrome DevTools Protocol escape hatch)
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

For Cursor, OpenClaw, Hermes Agent — see [examples/](examples/).

> 💡 **Agent skill available.** This repo includes a project-level skill at [`skills/codex-browser/SKILL.md`](skills/codex-browser/SKILL.md) that teaches LLM agents how to use all 48 tools effectively. Symlink or copy it into your agent's skills directory (`~/.claude/skills/`, `~/.codex/skills/`, etc.).

## All 48 MCP Tools

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

### DOM & Accessibility `[DOM]`
| Tool | Description |
|------|-------------|
| `codex_dom_snapshot` | Full accessibility tree with node IDs |
| `codex_dom_get_visible` | Human-readable visible DOM tree |
| `codex_dom_click` | Click by accessibility node ID |
| `codex_find_element` | Find elements by ARIA role + name |
| `codex_click_element` | Click element from `codex_find_element` result |

### Page Inspection `[Page]`
| Tool | Description |
|------|-------------|
| `codex_screenshot` | Capture viewport PNG screenshot |
| `codex_bring_to_front` | Activate a background tab (fixes screenshot timeouts) |
| `codex_evaluate` | Execute JavaScript, return JSON result |
| `codex_page_assets` | List page resources (images, CSS, JS, fonts) |
| `codex_dialog` | Handle alert / confirm / prompt |

### Input & Interaction `[Input]`
| Tool | Description |
|------|-------------|
| `codex_click` | Click by CSS selector (JS click) |
| `codex_fill` | Fill input by CSS selector |
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

### CDP Escape Hatch `[CDP]`
| Tool | Description |
|------|-------------|
| `codex_execute_cdp` | Execute any CDP command (allowlist-protected) |

### Session `[Session]`
| Tool | Description |
|------|-------------|
| `codex_name_session` | Name the current session |
| `codex_finalize` | Clean up tabs, release resources |
| `codex_get_info` | Get extension backend metadata |
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
codex-browser-bridge --mode mcp --profile basic     # 32 tools
codex-browser-bridge --mode mcp --profile network   # 46 tools
codex-browser-bridge --mode mcp --profile full      # all 48 (default)
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
- Cookie values redacted by default; CDP allowlist blocks dangerous domains

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

- `codex_network_monitor` — request/response inspection
- `codex_emulate_device` — mobile viewport emulation
- `codex_storage` — localStorage / sessionStorage access
- v2.0.0: Cross-platform (macOS / Linux via Unix domain sockets)

## Related

- [examples/](examples/) — MCP configs for Claude Code, Cursor, OpenClaw, Hermes Agent
- [skills/codex-browser/](skills/codex-browser/SKILL.md) — Agent skill (LLM usage guide)
- [ROADMAP.md](ROADMAP.md) — Full roadmap with SUPER scores
- [CHANGELOG.md](CHANGELOG.md) — Release history
- [CONTRIBUTING.md](CONTRIBUTING.md) — Dev setup and conventions

## License

MIT. Maintained independently from Codex / Anthropic / Google.

## Acknowledgments

Thanks to [LINUX DO](https://linux.do/) for community support and feedback.
