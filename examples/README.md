# MCP Client Examples

The bridge is an MCP stdio server. The JSON shape is intentionally similar
across clients, but install location, config file path, and approval/restart
steps differ.

## Supported clients

| Client | Config | Typical config location | After editing |
|--------|--------|-------------------------|---------------|
| Claude Code | [claude-code.json](claude-code.json) | `%USERPROFILE%\.claude\.mcp.json` | Restart Claude Code and approve in `/mcp` |
| Cursor | [cursor.json](cursor.json) | Cursor MCP settings / project MCP config | Reload Cursor window |
| OpenClaw | [openclaw.json](openclaw.json) | OpenClaw MCP server config | Restart or reconnect MCP servers |
| Hermes Agent | [hermes-agent.json](hermes-agent.json) | Hermes Agent MCP server config | Restart or reconnect MCP servers |

Replace `C:\\Users\\YOUR_USER\\Downloads` with a real upload directory. This
directory is the only location `codex_file_input` can upload from.

## Platform notes

### Windows (native)
Use the config as-is after replacing `YOUR_USER`. The binary must be in `PATH`
or use an absolute path:

```powershell
where.exe codex-browser-bridge
```

```json
"command": "C:\\Users\\YOUR_USER\\AppData\\Roaming\\npm\\codex-browser-bridge.cmd"
```

GUI-launched clients do not always inherit the same `PATH` as your terminal. If
the client reports spawn failures, paste the full `.cmd` path returned by
`where.exe`.

### WSL
The npm package is Windows-only (`os: win32`), so `npm i -g` from a Linux WSL
environment will be rejected by npm. Install on Windows, then point the WSL MCP
client at the Windows executable:

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

- `basic`: core tab/navigation/DOM/input tools plus `codex_doctor`
- `network`: basic plus cookies, storage, file upload, dialogs, CDP diagnostics, PDF, and network logs
- `full`: all tools, default

Use `CODEX_BRIDGE_PROFILE` or `--profile`.
