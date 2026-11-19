---
name: codex-browser
description: Use the installed Codex Browser Use and Computer Use runtimes for browser tabs and desktop applications.
---

# Codex Browser and Computer Use

- For Browser Use, select the registered Browser MCP service (`node_repl`, named
  `codex-browser` in the default plugin). Follow the installed Browser/Chrome
  plugin instructions to initialize its Browser JavaScript API.
- For Computer Use, select the registered Computer MCP service (`cua_repl`,
  named `codex-computer` in the default plugin) and read the native `js`
  instructions from `await cua.getState()` before sending desktop input.
- Keep objects in the service that created them. Present native approval
  requests to the user and respect the decision. Observe after input and
  never replay an action with an unknown outcome.
- Bootstrap once per worker. Read the native instructions when returned;
  reuse the current target rather than repeating inventory or setup.

See [native runtime setup](references/native-computer-use.md) for setup, discovery, and lifecycle details.
