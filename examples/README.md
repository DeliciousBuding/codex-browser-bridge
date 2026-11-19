# MCP client examples

Install and configure the official runtimes on the machine running the bridge.
The examples register `codex-browser` (`node_repl`) and `codex-computer`
(`cua_repl`) independently. Use [native-browser.json](native-browser.json) or
[native-computer-use.json](native-computer-use.json) if you only need one.
See [native setup and validation](../docs/native-computer-use.md) for launch
discovery, the Browser bootstrap and approval requirements.

| Client | Registration |
| --- | --- |
| Claude Code | Install the plugin, or use [claude-code.json](claude-code.json) without the plugin. |
| Antigravity | Merge [antigravity.json](antigravity.json) into the client's MCP configuration. |
| Cursor | Merge [cursor.json](cursor.json) into Cursor's MCP configuration. |
| OpenClaw | Merge [openclaw.json](openclaw.json) into the client's stdio MCP settings. |
| Hermes Agent | Merge [hermes-agent.json](hermes-agent.json) into the client's stdio MCP settings. |

```bash
claude plugin install codex-browser --marketplace DeliciousBuding/codex-browser-bridge
```

The plugin uses a pinned npm package through `npx`. Manual configurations
require the bridge command on the client's `PATH`, or an absolute installed
executable path. Avoid registering the same server manually and via the plugin
in one client. Use the runtime's own instructions and verify connectivity
before acting; a client that cannot present MCP elicitation cannot grant
native approvals.

`native-browser` is the default surface, so `--surface` may be omitted; the
examples pass it explicitly for clarity. The exposed tool list is the
installed runtime's own catalog.
