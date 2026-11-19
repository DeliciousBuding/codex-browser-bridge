# Native runtime setup

`codex-browser-bridge` relays MCP and CLI calls directly to the installed Codex
native runtimes:

- `--surface native-browser --mode mcp` (default surface): Codex `node_repl`
  (`codex-browser` in the plugin; Claude Code tool `mcp__codex-browser__js`).
- `--surface computer --mode mcp`: Codex `cua_repl` (`codex-computer` in the
  plugin; Claude Code tool `mcp__codex-computer__js`).

## Browser Use (`node_repl`)

1. Discover `browser_client_url` (`file:///...`) with
   `codex-browser-bridge --mode discover` (also printed by `--mode doctor`).
   If `codex-browser-bridge` is not on `PATH`, look under
   `$CODEX_HOME/plugins/cache/openai-bundled/browser/<version>/scripts/browser-client.mjs`
   and convert to a `file:///` URL.
2. Bootstrap once per worker:
   ```js
   const { setupBrowserRuntime } = await import("<browser_client_url>");
   const agent = await setupBrowserRuntime();
   ```
   Do not use bare `import "browser-client"`, and do not paste raw Windows
   backslashes into JS string literals (e.g. `\26` triggers a strict-mode octal
   syntax error).
3. Select the browser in order:
   - Explicit user choice: `agent.browsers.get("iab" | "chrome" | "edge" | "dot" | "extension")` (do not fall back if unavailable).
   - Known target URL: `agent.browsers.getForUrl(url)`.
   - Otherwise: `agent.browsers.getDefault()`.
   - Troubleshooting only: `agent.browsers.list()` and `agent.documentation.get("bootstrap-troubleshooting")` or `agent.documentation.get("chrome-troubleshooting")`.
4. Read `browser.documentation()` once without truncation (chunk only if the
   host truncates output), keep handles in `const`/`let` (never `globalThis`),
   batch multi-step work or `Promise.all(...)` inside one `js` cell, and reuse
   that browser and its tabs.

## Computer Use (`cua_repl`)

1. On the first `js` call—and immediately after `js_reset`—execute **only**
   `await cua.getState()` (optionally assigning the result).
2. Read the returned native instructions, use IDs from the current inventory,
   store cross-cell handles on `globalThis`, and re-observe after input.

## Approvals, lifecycle, and CLI

- Native approval requests (`elicitation/create`) require an interactive MCP
  host; non-interactive CLI calls cancel elicitations (`{"action": "cancel"}`).
- Only mark deliverable/handoff tabs (`markDeliverable` / `markHandoff`), never
  research or intermediate tabs. Marks are turn-scoped and expire on
  `turn_ended`.
- Long-lived MCP hosts can call `turn_ended` (missing `hook_event_name` /
  `session_id` / `turn_id` are filled from the active turn context, and the
  bridge rotates `turn_id` for the next task).
- For multi-step CLI work, use `batch` to keep one worker. Pass `--image-dir`
  only when you need saved image files alongside the JSON response.

See [native setup and verification](../../../docs/native-computer-use.md) for
configuration, diagnostics, and platform verification.
