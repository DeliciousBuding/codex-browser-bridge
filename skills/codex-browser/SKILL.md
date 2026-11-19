---
name: codex-browser
description: Use the installed Codex Browser Use (`node_repl`) and Computer Use (`cua_repl`) native runtimes for browser tabs and desktop applications.
---

# Codex Browser and Computer Use

- **Browser Use (`node_repl`)**: Use the `codex-browser` MCP service (in Claude Code: `mcp__codex-browser__js`, `js_reset`, `turn_ended`). Run `codex-browser-bridge --mode discover` once if needed to read the installed `browser_client` path, then bootstrap in `js` via `const { setupBrowserRuntime } = await import("<browser_client>"); const agent = await setupBrowserRuntime();` (never `import "browser-client"`). Keep REPL handles in `const`/`let` bindings, never `globalThis`.
- **Browser selection order**: Follow the installed Codex runtime convention—when the user names a browser, call `agent.browsers.get("iab" | "chrome" | "edge" | "extension")` and do not fall back if unavailable; when a URL is known, call `agent.browsers.getForUrl(url)`; otherwise call `agent.browsers.getDefault()`. Reserve `agent.browsers.list()` for troubleshooting (`agent.documentation.get("bootstrap-troubleshooting")` / `get("chrome-troubleshooting")`).
- **Computer Use (`cua_repl`)**: Use the `codex-computer` MCP service (in Claude Code: `mcp__codex-computer__js`, `js_reset`, `turn_ended`). On the first `js` call (and after `js_reset`), send **only** `await cua.getState()`, read the returned native instructions and inventory, and store cross-cell handles on `globalThis`.
- **Lifecycle & approvals**: Keep objects in the worker that created them. Present native elicitation/approval requests to the user and respect their decision. Observe after input and never replay an action with an unknown outcome. Mark tabs (`markDeliverable` / `markHandoff`) before ending a task, and call `turn_ended` (or close the MCP connection) when a task finishes.

See [native runtime setup](references/native-computer-use.md) for bootstrap, troubleshooting, and CLI batch usage.
