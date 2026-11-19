# Native Codex runtimes

Use `--surface computer --mode mcp` for the installed `cua_repl` and its `cua`
API. With a unified Browser and Computer Use configuration, this one entry
handles both surfaces. Add `--surface native-browser --mode mcp` only for
workflows that need the installed Browser/Chrome plugin's `node_repl` JavaScript
API. Follow the installed Browser plugin's bootstrap instructions in that
worker. Tool names and schemas come from each service's `tools/list`; enabled
tools can differ. Objects stay in their owning worker.

1. Start with the native `js` tool instructions. Inspect the returned API
   documentation and AX/screenshot state before sending UI input. Under
   `unified-computer-use` (`CUA_REPL_ENABLED_SURFACES="browser,computer"`),
   `cua` inside `js` exposes both desktop apps (`listApps`, `listWindows`,
   `getApp`) and browser tabs (`listBrowsers`, `getBrowser`, `createBrowserTab`,
   `getTab`, `listTabs`) via the shared `Target` interface (`getAXState`,
   `getScreenshot`, `getAXStateAndScreenshot`, `click`, `drag`, `scroll`,
   `selectText`, `setValue`, `performSecondaryAction`, `paste`, `pressKey`,
   `typeText`).
2. Keep native target identifier types: Windows and Linux `getApp({windowId})`
   use the exact `windowId` from `listWindows()`; macOS uses the native app
   name, path, or bundle ID.
3. The installed Codex launch row (`~/.codex/config.toml` `[mcp_servers.cua_repl]`
   or the bundled plugin `.mcp.json`) is authoritative for enabled surfaces,
   trusted services, environment variables, tool allow/deny filters, and
   timeouts.
4. Application approval uses MCP form elicitation (`elicitation/create`),
   preserving `accept`, `decline`, and `cancel` decisions and persistence
   metadata. The non-interactive CLI rejects approval prompts.
5. Re-observe after input and use screenshot IDs from the latest observation for
   coordinate actions. Treat timeouts or interrupts as unknown outcomes and do
   not replay input automatically.
6. For multi-step CLI workflows, use `batch` so JavaScript objects persist in one
   worker (`call` starts a fresh worker each time). Add `--image-dir <dir>` on
   `call` or `batch` to save returned image blocks to disk alongside the JSON
   response.

See `docs/native-computer-use.md` for runtime discovery rules, environment policy,
and platform status.
