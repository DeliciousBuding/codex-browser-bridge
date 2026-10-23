# MCP Client Examples

The bridge is an MCP stdio server. The JSON shape is intentionally similar
across clients, but install location, config file path, and approval/restart
steps differ.

## Recommended: install the plugin

Install the plugin where available. It fetches the pinned npm package and
platform binary through `npx` on first use, and installs both the MCP server
entry and the progressive `codex-browser` skill:

```powershell
# Claude Code 2.1.292+
claude plugin install codex-browser --marketplace DeliciousBuding/codex-browser-bridge
```

The Claude plugin uses the `full` 61-tool source profile because Claude Code defers
tool schemas through tool search. The manual JSON examples below are the
fallback for MCP clients without plugin or tool-search support and use the
`full` profile so they expose the same source capability set. Set `basic` (42)
or `network` (60) only when deliberately reducing the surface. They require a
global `npm i -g` install or an
absolute command path.

Codex and ChatGPT desktop already control the browser extension natively. Do
not install this bridge as a Codex plugin; these examples are for non-Codex MCP
clients that need to reuse the same browser session.

## Supported clients

| Client | Config | Typical config location | After editing |
|--------|--------|-------------------------|---------------|
| Claude Code | plugin, or [claude-code.json](claude-code.json) fallback | Managed by `claude plugin` | Restart Claude Code and approve in `/mcp` |
| Cursor | [cursor.json](cursor.json) | Cursor MCP settings / project MCP config | Reload Cursor window |
| OpenClaw | [openclaw.json](openclaw.json) | OpenClaw MCP server config | Restart or reconnect MCP servers |
| Hermes Agent | [hermes-agent.json](hermes-agent.json) | Hermes Agent MCP server config | Restart or reconnect MCP servers |

Replace `C:\\Users\\YOUR_USER\\Downloads` with a real upload directory. This
directory is the only location `codex_file_input` can upload from.

## Platform notes

### Windows (native)
Use the config as-is after replacing `YOUR_USER`. For GUI-launched clients and
scheduled agents, prefer the absolute `command` path printed by npm
`postinstall`.

If you need to recover the installed command path later:

```powershell
$bridge = Join-Path (npm prefix -g) "codex-browser-bridge.cmd"
& $bridge --mode doctor
```

```json
"command": "C:\\Users\\YOUR_USER\\AppData\\Roaming\\npm\\codex-browser-bridge.cmd"
```

GUI-launched clients do not always inherit the same `PATH` as your terminal. If
the client reports spawn failures, paste the `command` from
`install.suggested_mcp_config` in the doctor output.

### Linux and WSL

v1.12.0 includes Linux GNU x64/arm64 packages and Unix sockets under
`/tmp/codex-browser-use/*.sock`; release binaries require glibc >= 2.39. The
Linux extension path has not been live-verified. WSL Linux sockets and Windows
named pipes are separate transports: WSL does not make them interchangeable.

When using a Windows install from WSL, point the client at the Windows executable:

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "/mnt/c/Users/YOUR_USER/AppData/Roaming/npm/codex-browser-bridge.cmd",
      "args": ["--mode", "mcp"],
      "transport": "stdio",
      "env": {
        "CODEX_BRIDGE_UPLOAD_BASE": "C:\\Users\\YOUR_USER\\Downloads"
      }
    }
  }
}
```

### Profiles

- `basic`: recommended starting profile; core tab/navigation/DOM/input tools plus `codex_doctor`
- `network`: basic plus cookies, storage, file upload, dialogs, CDP diagnostics, PDF, and network logs
- `full`: all tools, default

Use `CODEX_BRIDGE_PROFILE` or `--profile`.
