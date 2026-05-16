<p align="center">
  <h1 align="center">codex-browser-bridge</h1>
  <p align="center">Control your Chrome browser from Claude Code (or any AI agent) via the Codex Chrome Extension.</p>
</p>

<p align="center">
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License"></a>
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/releases"><img src="https://img.shields.io/github/v/release/DeliciousBuding/codex-browser-bridge?style=flat-square" alt="Latest Release"></a>
  <a href="https://github.com/DeliciousBuding/codex-browser-bridge/actions"><img src="https://img.shields.io/github/actions/workflow/status/DeliciousBuding/codex-browser-bridge/ci.yml?style=flat-square" alt="CI"></a>
</p>

---

Single Go binary that connects AI agents to the user's existing Chrome browser through Codex Desktop's Chrome extension infrastructure — named pipes, CDP, and JSON-RPC.

**Why?** Codex Desktop's `browser-client.mjs` depends on `import.meta.__codexNativePipe`, a privileged object injected by Codex's custom ESM loader. Claude Code can't access it. But the underlying protocol is simple (JSON-RPC 2.0 over length-prefixed frames), so this bridge reimplements it.

## Quick Start

### 1. Prerequisites

- **Chrome** with the [Codex Chrome Extension](https://chromewebstore.google.com/detail/codex/hehggadaopoacecdllhhajmbjkdcmajg) installed and enabled
- **Codex Desktop** running (creates the named pipes)

### 2. Install

<details>
<summary><b>Go install</b> (recommended)</summary>

```bash
go install github.com/DeliciousBuding/codex-browser-bridge/cmd/bridge@latest
```
</details>

<details>
<summary><b>Download binary</b></summary>

Download from [GitHub Releases](https://github.com/DeliciousBuding/codex-browser-bridge/releases) and add to your `PATH`.
</details>

<details>
<summary><b>Build from source</b></summary>

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
make build
# binary at bin/codex-browser-bridge
```
</details>

### 3. Register as MCP Server

Add to your Claude Code settings (`~/.claude/settings.json`):

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["-mode", "mcp"],
      "transport": "stdio"
    }
  }
}
```

Or if built from source:

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "D:/path/to/codex-browser-bridge/bin/codex-browser-bridge",
      "args": ["-mode", "mcp"],
      "transport": "stdio"
    }
  }
}
```

### 4. Use

Claude Code will automatically discover the `codex_*` tools. Ask it to:

- "Open example.com in a new tab"
- "Take a screenshot of the current page"
- "What's on my browser right now?"

## Architecture

```
AI Agent (MCP client)
  → codex-browser-bridge (Go binary, stdio JSON-RPC)
    → Windows Named Pipe (\\.\pipe\codex-browser-use-*)
      → extension-host.exe (Codex native messaging host)
        → Chrome Extension
          → Controls Chrome tabs
```

## Tools

| Tool | Description |
|------|-------------|
| `codex_create_tab` | Create a new browser tab |
| `codex_list_tabs` | List tabs in current session |
| `codex_user_tabs` | List all open tabs across browser windows |
| `codex_claim_tab` | Take control of an existing tab |
| `codex_close_tab` | Close a tab |
| `codex_navigate` | Navigate a tab to a URL |
| `codex_reload` | Reload a tab |
| `codex_screenshot` | Capture screenshot (base64 PNG) |
| `codex_dom_snapshot` | Get accessibility tree snapshot |
| `codex_evaluate` | Run JavaScript in the page |
| `codex_click` | Click an element by CSS selector |
| `codex_fill` | Fill a form input |
| `codex_cua_click` | Click at screen coordinates |
| `codex_cua_type` | Type text at current focus |
| `codex_cua_keypress` | Press keyboard keys |
| `codex_cua_scroll` | Scroll at coordinates |
| `codex_dom_get_visible` | Get visible DOM tree |
| `codex_dom_click` | Click a DOM node by ID |
| `codex_name_session` | Name the browser session |
| `codex_finalize` | Clean up tabs after session |
| `codex_get_info` | Get backend info |

## CLI Mode

For debugging and testing:

```bash
# List active pipes
codex-browser-bridge -mode discover

# Interactive CLI
codex-browser-bridge -mode cli

# Connect to a specific pipe
codex-browser-bridge -mode cli -pipe "codex-browser-use\<uuid>"
```

CLI commands: `tabs`, `create`, `close <id>`, `user-tabs`, `claim <id>`, `nav <id> <url>`, `snapshot <id>`, `screenshot <id>`, `info`, `ping`, `try <method> [json]`, `quit`

## How It Works

1. **Pipe Discovery**: Enumerates `\\.\pipe\codex-browser-use-*` named pipes via PowerShell
2. **Connection**: Dials the pipe using `go-winio` (Microsoft's Go Windows named pipe library)
3. **Protocol**: JSON-RPC 2.0 with 4-byte little-endian length-prefixed frames
4. **CDP**: After `attach`, sends Chrome DevTools Protocol commands through the extension host
5. **Session**: Each connection creates a fresh browser session; tabs are session-scoped

## Contributing

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
make test
make build
```

## License

[MIT](LICENSE) - Copyright (c) 2026 DeliciousBuding

---

> **Disclaimer**: This is an independent, third-party project. It is not affiliated with, endorsed by, or connected to OpenAI or the Codex Desktop team. Use at your own risk.
