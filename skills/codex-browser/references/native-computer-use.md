# Native runtime setup

Use `--surface native-browser --mode mcp` for Browser Use (`node_repl`), and
`--surface computer --mode mcp` for Computer Use (`cua_repl`). The plugin
registers both. The actual catalog comes from each installed runtime.

For Browser Use, initialize the installed Browser/Chrome client following its
instructions. Use `agent.browsers.list()` to verify the backend when setting up
the worker. Select the exact backend, read `browser.documentation()` once,
then reuse that browser and its tabs.

For desktop input, start with `await cua.getState()`. Read the returned native
instructions, use IDs from the current inventory, and observe the target again
after input. Use fresh screenshot IDs when the native method requires them.

Native approval requests need an MCP client that can present elicitation.
For multi-step CLI work, use `batch` to keep one worker. The CLI cannot approve
an elicitation. Use `--image-dir` only if you need image files as well as the
native response.

See [native setup and verification](../../../docs/native-computer-use.md) for
configuration, diagnostics, and platform verification.
