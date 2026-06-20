# MCP Client Examples

The `codex-browser-bridge` MCP server uses stdio transport — the standard MCP wire protocol.
All MCP clients configured correctly for stdio transport will work identically.

## Supported clients

| Client | Config |
|--------|--------|
| Claude Code | [claude-code.json](claude-code.json) |
| OpenClaw | [openclaw.json](openclaw.json) |
| Hermes Agent | [hermes-agent.json](hermes-agent.json) |
| Cursor | [cursor.json](cursor.json) |

## Platform notes

### Windows (native)
Use the config as-is. The `codex-browser-bridge` binary must be in `PATH` or use an absolute path:
```json
"command": "D:/path/to/codex-browser-bridge.exe"
```

### WSL
From WSL, the bridge accesses Windows named pipes. Use the Windows binary path via `/mnt/c/...`.
