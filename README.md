<p align="center">
  <img src="assets/hero.png" alt="codex-browser-bridge" width="720">
</p>

<p align="center">
  <h1 align="center">codex-browser-bridge</h1>
  <p align="center">
    Let Claude Code and other MCP agents control your existing Chrome browser through Codex Desktop's browser bridge.
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
  <a href="https://goreportcard.com/report/github.com/DeliciousBuding/codex-browser-bridge">
    <img src="https://goreportcard.com/badge/github.com/DeliciousBuding/codex-browser-bridge?style=flat-square" alt="Go Report Card">
  </a>
  <a href="README.zh-CN.md">中文</a>
</p>

---

`codex-browser-bridge` is a small Go binary that exposes Codex Desktop's Chrome browser bridge as an MCP server.

It connects to the local Codex browser named pipe, speaks the same length-prefixed JSON-RPC protocol, and provides browser-control tools to Claude Code or any MCP-compatible agent.

## Why

Codex Desktop can talk to its Chrome extension through a privileged native pipe. Other agents, such as Claude Code, cannot directly access that internal bridge.

This project reuses the local browser bridge that already exists on your machine and wraps it as an MCP server.

That means an agent can:

- inspect your current browser tabs
- claim an existing tab
- open and close tabs
- navigate pages
- capture screenshots
- read DOM / accessibility snapshots
- click, type, scroll, and evaluate JavaScript

Useful when an agent needs to work with pages that require a real browser session, such as dashboards, logged-in web apps, local development servers, or documentation sites.

## Status

Version 1.5.1 is a local Windows tool for Codex Desktop and the Codex Chrome Extension. It supports both known Codex browser pipe name formats:

- `codex-browser-use-<uuid>`
- `codex-browser-use\<uuid>`

The bridge is still intended for local development and controlled automation, not remote or multi-user deployment.

## Features

- MCP server over stdio
- Single Go binary
- No browser profile copying
- Uses your existing Chrome session
- Auto-discovers `codex-browser-use-*` named pipes
- Talks to Codex Desktop's extension host through JSON-RPC
- Uses Chrome DevTools Protocol commands for page control
- Includes an interactive CLI mode for debugging

## Requirements

- Windows
- Chrome
- Codex Desktop running
- Codex Chrome Extension installed and enabled
- Go 1.23+ if building from source

> The bridge connects to local named pipes created by Codex Desktop. If no pipe is found, start Codex Desktop first and make sure the extension is active.

## Installation

### Option 1: Install with npm

```bash
npm i -g @delicious233/codex-browser-bridge
```

### Option 2: Install with Go

```bash
go install github.com/DeliciousBuding/codex-browser-bridge/cmd/bridge@latest
```

Make sure your Go binary path is available in `PATH`.

### Option 3: Download a release

Download the latest binary from:

```text
https://github.com/DeliciousBuding/codex-browser-bridge/releases
```

Then place `codex-browser-bridge.exe` somewhere in your `PATH`.

### Option 4: Build from source

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
make build
```

The binary will be generated at:

```text
bin/codex-browser-bridge.exe
```

## Quick Start with Claude Code

Add the MCP server to your Claude Code settings.

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

If you built from source, use the absolute path instead:

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "D:/path/to/codex-browser-bridge/bin/codex-browser-bridge.exe",
      "args": ["-mode", "mcp"],
      "transport": "stdio"
    }
  }
}
```

Restart Claude Code after editing the settings file.

Then ask the agent things like:

```text
List my open browser tabs.
```

```text
Open https://example.com in a new tab and take a screenshot.
```

```text
Claim my current documentation tab and summarize what is visible.
```

## CLI Usage

The binary has three modes.

### MCP mode

Default mode. Used by Claude Code or other MCP clients.

```bash
codex-browser-bridge -mode mcp
```

### Discover mode

Lists active Codex browser named pipes.

```bash
codex-browser-bridge -mode discover
```

Example output:

```json
[
  {
    "Name": "codex-browser-use-xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
    "UUID": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
  }
]
```

### Interactive CLI mode

Useful for debugging the bridge without an MCP client.

```bash
codex-browser-bridge -mode cli
```

Connect to a specific pipe:

```bash
codex-browser-bridge -mode cli -pipe "codex-browser-use-<uuid>"
```

Available CLI commands:

```text
tabs
create
close <tab_id>
user-tabs
claim <tab_id>
nav <tab_id> <url>
snapshot <tab_id>
screenshot <tab_id>
info
ping
try <method> [json_params]
quit
```

## MCP Tools

### Tab management

| Tool               | Description                                     |
| ------------------ | ----------------------------------------------- |
| `codex_list_tabs`  | List tabs managed by the current bridge session |
| `codex_create_tab` | Create a new browser tab                        |
| `codex_close_tab`  | Close a browser tab                             |
| `codex_user_tabs`  | List open tabs across browser windows           |
| `codex_claim_tab`  | Claim an existing user tab for automation       |

### Navigation

| Tool                       | Description                                         |
| -------------------------- | --------------------------------------------------- |
| `codex_navigate`           | Navigate a tab to a URL                             |
| `codex_navigate_back`      | Navigate a tab back one entry in its history        |
| `codex_navigate_forward`   | Navigate a tab forward one entry in its history     |
| `codex_reload`             | Reload a tab                                        |
| `codex_wait_for_load`      | Poll `document.readyState` until `complete`         |

### Page inspection

| Tool                    | Description                                  |
| ----------------------- | -------------------------------------------- |
| `codex_screenshot`      | Capture a screenshot (returns MCP image). `fullPage` is reserved for a future release. The current implementation captures the viewport. |
| `codex_dom_snapshot`    | Get an accessibility tree snapshot           |
| `codex_dom_get_visible` | Get a simplified visible DOM tree (human-readable; use codex_dom_snapshot for node IDs usable with codex_dom_click) |
| `codex_evaluate`        | Evaluate JavaScript in the page context      |
| `codex_get_info`        | Get backend information from the extension   |

### Interaction

| Tool                 | Description                      |
| -------------------- | -------------------------------- |
| `codex_click`        | Click an element by CSS selector |
| `codex_fill`         | Fill an input by CSS selector    |
| `codex_dom_click`    | Click a DOM node by accessibility node ID from codex_dom_snapshot |
| `codex_cua_click`    | Click by screen coordinates      |
| `codex_cua_type`     | Type text at the current focus   |
| `codex_cua_keypress` | Press keyboard keys              |
| `codex_cua_scroll`   | Scroll by coordinates            |

### Session

| Tool                 | Description                                         |
| -------------------- | --------------------------------------------------- |
| `codex_name_session` | Assign a human-readable name to the browser session |
| `codex_finalize`     | Finalize the session and clean up tabs              |

## Architecture

```text
MCP Client
  Claude Code / other agent
        │
        │ stdio JSON-RPC
        ▼
codex-browser-bridge
  Go binary
        │
        │ length-prefixed JSON-RPC frames
        ▼
Windows Named Pipe
  \\.\pipe\codex-browser-use-*
        │
        ▼
Codex Desktop extension host
        │
        ▼
Codex Chrome Extension
        │
        ▼
Chrome tabs
```

## How It Works

1. The bridge searches for local named pipes matching `codex-browser-use-*`.
2. It connects to the selected pipe through `go-winio`.
3. Every request is encoded as a 4-byte little-endian length prefix followed by a JSON-RPC payload.
4. Browser operations are sent to the Codex extension host.
5. Page-level operations use Chrome DevTools Protocol commands such as `Page.navigate`, `Page.captureScreenshot`, `Runtime.evaluate`, and `Input.dispatchMouseEvent`.
6. The MCP layer exposes these operations as `codex_*` tools.

## Security Notes

This tool gives an agent access to your active browser session.

Use it with the same caution you would apply to browser automation tools:

- do not expose the bridge to a network port
- do not run it for untrusted MCP clients
- review agent actions before allowing sensitive operations
- avoid using it on pages containing passwords, payment details, private tokens, or production admin consoles
- remember that claimed tabs may already be logged in

The project is intended for local development and controlled automation.

## Troubleshooting

### No pipe found

```text
No codex-browser-use pipes found. Is Codex Desktop running?
```

Check:

- Codex Desktop is running
- Chrome is running
- Codex Chrome Extension is installed and enabled
- the extension has been initialized by Codex Desktop

### Claude Code does not show the tools

Check:

- the binary is in `PATH`
- the MCP server config points to the correct executable
- Claude Code was restarted after editing settings
- `codex-browser-bridge -mode discover` works in a terminal

### CDP command fails

Some browser operations require the bridge to attach to the tab before sending CDP commands. If a tab was opened outside the bridge, list user tabs first, then claim the target tab.

## Development

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge

make test
make build
```

Common commands:

```bash
make build         # build binary
make test          # go vet ./... && go test ./...
make clean         # remove build output
```

## Roadmap

Planned or open work:

- clearer error messages for common pipe / extension failures
- non-Windows fallback or explicit platform guards
- screenshot output handling across MCP clients
- typed tool result schemas
- optional allowlist / confirmation layer for sensitive domains
- examples for Claude Code, Cursor, Codex CLI, and other MCP clients

## License

MIT License.

## Disclaimer

This is an independent third-party project. It is not affiliated with, endorsed by, or connected to OpenAI, Codex Desktop, Anthropic, Claude Code, Google, or Chrome.

## Acknowledgments

Thanks to [LINUX DO](https://linux.do/) for the community support and feedback.
