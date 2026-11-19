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

On the first `js` call—and immediately after `js_reset`—execute exactly one
entry call, optionally assigning its result. Do not add waits, snapshots, or
other API calls to that invocation. Read its returned documentation and initial
state before continuing. Follow the installed tool's first matching entry:

| Target | Entry |
| --- | --- |
| Inventory needed | `await cua.getState()` |
| Referenced tab | `let tab = await cua.getTab({ mention: tabMentionUrl })` using the complete supplied reference |
| Existing tab URL and browser | `let tab = await cua.getTab({ url }, { browser: browserId })` |
| Known tab ID and browser | `let tab = await cua.getTab(tabId, { browser: browserId })` |
| New tab in a named browser | `let tab = await cua.createBrowserTab(browserName, url, browserOptions)`; pass the name directly |
| URL without a named browser | `let browser = await cua.getBrowser({ url })` |
| Known Windows window | `let app = await cua.getApp({ windowId: selected.id })` using the observed window ID |

Use the runtime's documented browser options, including `visible` for IAB/Dot
and a short emoji-prefixed `sessionName` for Chrome/Edge. Use Dot only when
available. Keep handles in the persistent REPL, use observed IDs, and
re-observe after input. After context recovery, call
`await cua.rewriteDocumentation()` before continuing CUA work.

When `completion_summary` is exposed, send a `js` call containing only that
field once the CUA outcome is observed. It records the outcome; it does not
reset the REPL or end the host turn.

## Approvals, lifecycle, and CLI

- Native approval requests (`elicitation/create`) require an interactive MCP
  host; non-interactive CLI calls cancel elicitations (`{"action": "cancel"}`).
- Only mark deliverable/handoff tabs (`markDeliverable` / `markHandoff`), never
  research or intermediate tabs. Marks are turn-scoped and expire on
  `turn_ended`. Follow backend cleanup documentation: unmarked agent-created
  tabs close, claimed user tabs are released and remain open, and existing
  persistent IAB/Dot tabs remain open unless explicitly closed.
- Long-lived MCP hosts can call `turn_ended` (missing `hook_event_name` /
  `session_id` / `turn_id` are filled from the active turn context, and the
  bridge rotates `turn_id` after a successful matching acknowledgement).
- A rejected hook retains the active context for exit cleanup. An unanswered
  explicit hook is reported as unconfirmed and is not replayed.
- For multi-step CLI work, use `batch` to keep one worker. Pass `--image-dir`
  only when you need saved image files alongside the JSON response; add
  `--omit-inline-images` to drop the base64 payload from stdout and read the
  saved files instead.

See [native setup and verification](../../../docs/native-computer-use.md) for
configuration, diagnostics, and platform verification.
