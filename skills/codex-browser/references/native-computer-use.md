# Native Computer Use

Use an explicitly configured `codex-computer` server running
`codex-browser-bridge --surface computer --mode mcp`. It exposes the installed
native `cua_repl` tools, not the browser `codex_*` tool catalog.

Start with the native `js` tool instructions and exactly one documented entry
point. Read the returned API documentation and state before proceeding.
Windows/Linux app bindings use the exact `windowId` returned by inventory;
macOS uses the native app name/path/bundle ID. Keep native ID types.

Application access uses the native MCP elicitation. Preserve the user's
accept/decline/cancel choice and native persistence metadata. A rejected
application, locked desktop, stale target or unsupported native method is an
error; do not replace the executor or bypass policy.

Observe after input. Use screenshot IDs from the latest corresponding native
observation for coordinate actions. Treat a timeout as an unknown outcome and
do not replay input. Native input can activate windows.

Keep a multi-step CLI task in one `batch`; `call` starts a fresh worker.
`--image-dir` writes visible image artifacts while retaining native image blocks.
Noninteractive CLI cannot approve applications.

`js_reset`, completion summary, turn end and worker exit have distinct meanings.
Use the native completion summary when the task outcome is observed. Ending the
worker must not close user apps. Platform and native runtime restrictions are
listed in `docs/native-computer-use.md` in the repository.
