# Network, CDP, And Session

Use this reference for cookies, storage, traffic capture, raw diagnostics, and
cleanup.

## Network And State

| Tool | Purpose |
|------|---------|
| `codex_network_cookies` | Read cookies, with values redacted by default |
| `codex_network_set_cookie` | Set a cookie after URL validation |
| `codex_delete_cookies` | Delete cookies by name |
| `codex_storage` | Read or write local/session storage |
| `codex_network_monitor` | Capture paired requests and responses |

Do not reveal cookie or storage values. Ask for explicit confirmation before
passing `redact_values: false`. Prefer the dedicated cookie and storage tools
over raw CDP.

## CDP Escape Hatch

`codex_execute_cdp` runs only explicitly allowlisted diagnostic methods. Use it
when no typed tool covers the operation. Navigation, cookie mutation,
screenshots, PDF, file upload, page-resource content, event-producing enable
calls, arbitrary Runtime JavaScript, and destructive methods remain blocked.

## Session Control

| Tool | Purpose |
|------|---------|
| `codex_name_session` | Give the current browser session a diagnostic name |
| `codex_finalize` | Send official `turnEnded`; preserve marked handoff/deliverable tabs |
| `codex_get_info` | Read bridge and extension metadata |
| `codex_doctor` | Check pipe connectivity, installation, and browser metadata |

`codex_doctor` is the only schema marked `anthropic/alwaysLoad`; use it when
connectivity is unknown. Call `codex_finalize` when done with bridge-owned tabs.
The extension closes unmarked agent-created tabs and releases unmarked claimed
user tabs; a disconnected transport does not prove cleanup succeeded. After a
successful finalization, the same MCP process can start a fresh turn.
