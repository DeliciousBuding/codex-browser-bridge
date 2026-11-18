# Native Computer Use

`--surface computer` exposes the installed Codex `cua_repl` MCP service to external
agents. The bridge handles runtime discovery, stdio MCP relay, CLI commands, and
owned worker cleanup; JavaScript execution, desktop objects, permissions, input,
screenshots, and errors come directly from `cua_repl`.

The browser surface remains the default. Pass `--surface computer` to use the
native desktop surface. Codex itself already connects to `cua_repl` natively and
should not be configured to call this bridge.

## Runtime and setup

Start the native Codex desktop runtime on the target machine. Discovery reads
`[mcp_servers.cua_repl]` from `~/.codex/config.toml` (or `$CODEX_HOME/config.toml`)
when present, and otherwise falls back to the latest generated `.mcp.json` under
`plugins/cache/openai-bundled/unified-computer-use`. Pass `--native-config <file>`
to use an explicit MCP JSON containing `mcpServers.cua_repl`. Discovery never
modifies user config or plugin caches.

The bridge preserves the launch row's command, arguments, working directory,
literal `env`, `env_vars`, `enabled` flag, timeout settings, and
`enabled_tools`/`disabled_tools` lists, including `CUA_REPL_ENABLED_SURFACES` and
`NODE_REPL_TRUSTED_SERVICES`. Native `js` can therefore access every surface
enabled by the host configuration, without the bridge's browser-specific tab,
network, or output-size wrappers.

Local stdio launch follows Codex CLI's environment policy:

- Inherits the platform core environment variables, explicitly listed `env_vars`
  (`local` or default), literal `env` overrides, and custom CA certificate variables
  (resolved to absolute paths).
- Strips ambient variables outside that set as well as Codex's non-inheritable
  internal auth tokens.
- On Windows, resolves bare commands (such as `python` or `npx`) against the
  constructed child `PATH` and `PATHEXT`.
- Desktop session variables (such as `DISPLAY` and `XAUTHORITY` on X11, or
  Wayland session variables) must be present in the launch row's `env` or
  `env_vars`.

Startup and tool timeouts default to 30 seconds and 300 seconds unless overridden
by `startup_timeout_sec`/`startup_timeout_ms` or `tool_timeout_sec` in the launch
row. CLI and doctor apply these budgets directly (`arguments.timeout_ms` controls
the inner JavaScript evaluation timeout). In `--mode mcp`, the external MCP host
manages handshake negotiation, catalog pagination, interactive elicitation waits,
and client request deadlines.

```bash
codex-browser-bridge --surface computer --mode discover
codex-browser-bridge --surface computer --mode doctor
```

`discover` validates the launch configuration and paths. `doctor` performs the
MCP handshake, lists the filtered tool catalog, and shuts down the worker cleanly.

For an MCP client:

```json
{
  "mcpServers": {
    "codex-computer": {
      "command": "codex-browser-bridge",
      "args": ["--surface", "computer", "--mode", "mcp"]
    }
  }
}
```

The default Claude plugin registers the browser surface only; add this entry
separately when you want Computer Use.

## Native methods and authorization

The standard `cua_repl` catalog exposes `js`, `js_reset`,
`js_add_node_module_dir`, and `turn_ended`. Call `js` and read its returned
documentation and state before issuing UI actions.

`enabled_tools` is applied before `disabled_tools` across advertised catalogs,
direct `tools/call` requests, and the end-of-session `turn_ended` cleanup hook.
If `turn_ended` is disabled, cleanup skips the hook and terminates the owned
worker tree. CLI and doctor traverse paginated `tools/list` responses within
bounded page, item, and cursor limits under the startup timeout; the MCP relay
passes pagination cursors and unknown tool fields through untouched.

Application approval uses bidirectional MCP form elicitation
(`elicitation/create`). The relay preserves `accept`, `decline`, and `cancel`
responses, persistence metadata, progress notifications, and image blocks. The
connected MCP host must support MCP elicitation to approve new applications.

When a `tools/call` request omits Codex correlation metadata, the bridge fills in
`_meta.callId`, `_meta["x-codex-turn-metadata"]` (`session_id` and `turn_id`),
`_meta.sessionId`, and `_meta.threadId` (preferring `thread_id` when supplied).

## CLI

```bash
codex-browser-bridge --surface computer tools
codex-browser-bridge --surface computer tools js
codex-browser-bridge --surface computer call js '{"code":"await cua.getState();"}'
codex-browser-bridge --surface computer --image-dir ./captures batch
```

- `call` runs one tool invocation in a fresh worker.
- `batch` reads newline-delimited `{"name":"js","arguments":{...}}` JSON objects
  (up to 1 MiB per line) on a single worker so JavaScript bindings persist across
  steps.
- `--image-dir` (valid with `call` or `batch`) writes returned image blocks to
  files and attaches `bridge_artifacts` metadata while keeping the original
  base64 image blocks in stdout JSON.
- Browser-only flags (`--pipe`, `--browser-family`, `--browser-instance`,
  `--profile`, `--upload-base`, `--max-text-bytes`, `--max-image-bytes`,
  `--max-output-bytes`, `--network-policy`, `--finalize`, `--session-file`,
  `--keep-turn`) are rejected before startup.
- The CLI is non-interactive and declines `elicitation/create` requests; approve
  applications through an interactive MCP host.

## Lifecycle

Each MCP session or CLI invocation owns one native worker process tree
(Windows Job Object with `CREATE_NO_WINDOW`, or Unix process group). `js_reset`
clears JavaScript bindings in the worker without closing user applications.

On EOF or interrupt (`SIGINT`/`SIGTERM`), the bridge cancels any in-flight
request, invokes `turn_ended` (`Stop` or `Interrupt`) for the active execution
context with a fresh `callId` and no `progressToken`, closes child `stdin`, and
terminates the owned process tree within an 8-second cleanup budget. Timed-out or
interrupted calls exit nonzero and are never replayed.

## Verification status

Verified on 2026-11-18 against `cua-repl` 0.1.5 / Sky 0.7.21 (Codex host
integration reference `30c75292aa4c8ab614c99f8d7a38e0d60b9347da`). Tracking and
remaining platform checks live in
[issue #57](https://github.com/DeliciousBuding/codex-browser-bridge/issues/57).

| Platform / runtime | Protocol & observation | Input & lifecycle | Notes |
| --- | --- | --- | --- |
| Windows x64 (`cua-repl` 0.1.5 / Sky 0.7.21) | `initialize`, catalog filtering, fresh `accept`/`decline`/`cancel` elicitation, `js_reset`, synthetic AX state, screenshot, and CLI non-interactive rejection passed | Unlocked desktop synthetic Win32 click (`0 -> 1`), `typeText`, active-call cancel without replay, 17-worker process-tree cleanup, and fresh-worker retry passed | Verified against a running Codex desktop host pipe; standalone helper spawn without the host returned `spawn EPERM` |
| Linux ARM64 X11 (`cua-repl` 0.1.5 / Sky 0.7.21) | Direct native service and bridge relay both passed `initialize`, catalog, synthetic GTK AX state, screenshot, CLI batch persistence/`js_reset`, `--image-dir`, and `SIGTERM` cleanup | Synthetic GTK button click (`0 -> 1`) and process-group cleanup passed | Requires X11 session variables (`DISPLAY`, `XAUTHORITY`) in the launch row |
| Windows ARM64 / Linux x64 | Release binaries built and CI tested | Not live-tested on desktop GUI | — |
| macOS | CI source build, unit/process tests, clippy, and CLI smoke passed on `macos-15` | Not live-tested with macOS TCC/Accessibility | No prebuilt macOS release binary |
| Linux Wayland | Untested | Untested | — |
