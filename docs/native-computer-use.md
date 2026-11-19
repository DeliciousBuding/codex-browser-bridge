# Native Codex runtimes

External agents can run the installed Codex runtimes through the same stdio
relay. The selected runtime supplies its own tools, schemas, JavaScript
objects, permissions, images, and errors.

| Entry | Runtime | When to use it |
| --- | --- | --- |
| `--surface native-browser` (default) | Codex `node_repl` | Use the installed Codex Browser/Chrome plugin's JavaScript API and instructions, including its browser backend selection and permissions. |
| `--surface computer` | Codex `cua_repl` | Use the installed Computer Use `cua` API and its configured surfaces. |

Codex itself uses the native services directly. The bridge's plugin and
client templates register both services. If only one is installed or needed,
register just that one. Each registration owns a worker; handles cannot move
between them.
Separate workers on one desktop still share physical input and focus.

## Runtime and setup

Start the native Codex runtime and any required browser extension on the
machine that owns the browser or desktop session. Discovery reads the selected
`[mcp_servers.node_repl]` or `[mcp_servers.cua_repl]` row from
`~/.codex/config.toml` (or `$CODEX_HOME/config.toml`). Computer Use can also
discover the generated `.mcp.json` under
`plugins/cache/openai-bundled/unified-computer-use`. The Browser entry requires
an actual `node_repl` row and does not infer a launch command from the plugin
bundle. `--native-config <file>` selects an explicit JSON configuration with
the corresponding `mcpServers.node_repl` or `mcpServers.cua_repl` entry.
Discovery never modifies user config or plugin caches.

The bridge preserves the launch row's command, arguments, working directory,
literal `env`, `env_vars`, `enabled` flag, timeout settings, and
`enabled_tools`/`disabled_tools` lists, including `CUA_REPL_ENABLED_SURFACES` and
`NODE_REPL_TRUSTED_SERVICES`. Native `js` can therefore access every surface
enabled by the host configuration, exactly as it does inside Codex; the bridge
adds no tab, network, or output-size wrapper of its own.

Local stdio launch follows Codex CLI's environment policy:

- Inherits the platform core environment variables, explicitly listed `env_vars`
  (`local` or default), literal `env` overrides, and custom CA certificate variables
  (resolved to absolute paths).
- Strips ambient variables outside that set as well as Codex's non-inheritable
  internal auth tokens.
- On Windows, resolves bare commands (such as `python` or `npx`) against the
  constructed child `PATH` (using the process's `PATHEXT` for extension
  matching, matching Codex CLI's `program_resolver`) while forwarding `PATHEXT`
  in the child environment.
- Desktop session variables (such as `DISPLAY` and `XAUTHORITY` on X11, or
  Wayland session variables) must be present in the launch row's `env` or
  `env_vars`.
- For `--surface native-browser` (`node_repl`), when `[mcp_servers.node_repl]`
  declares `BROWSER_USE_ACCESS_*` in `env_vars`, those host-provided variables
  must be present in the launching process environment for the browser backend
  to authorize access (`js` can still execute without them, while
  `agent.browsers.list()` reports the browser access host unavailable).
  `discover` and `doctor` report boolean presence flags under
  `browser_access_env` without printing values.

Startup and tool timeouts default to 30 seconds and 300 seconds unless overridden
by `startup_timeout_sec`/`startup_timeout_ms` or `tool_timeout_sec` in the launch
row. CLI and doctor apply these budgets directly (`arguments.timeout_ms` controls
the inner JavaScript evaluation timeout). In `--mode mcp`, the external MCP host
manages handshake negotiation, catalog pagination, interactive elicitation waits,
and client request deadlines.

```bash
codex-browser-bridge --surface native-browser --mode doctor
codex-browser-bridge --surface computer --mode discover
codex-browser-bridge --surface computer --mode doctor
```

`discover` checks the launch row and explicit executable paths (and reports
`browser_access_env` presence booleans for `native-browser`). `doctor`
performs the MCP handshake, lists the filtered tool catalog, and shuts down the
worker. Browser backend access is checked separately through the Browser JS API.
Note that the Claude plugin registers both `codex-browser` and `codex-computer`;
if only one runtime is installed on the host, the uninstalled server failing to
start is expected—use a single-entry manual config if you only want one.

For an MCP client, merge either or both entries into its local stdio configuration:

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "codex-browser-bridge",
      "args": ["--surface", "native-browser", "--mode", "mcp"]
    },
    "codex-computer": {
      "command": "codex-browser-bridge",
      "args": ["--surface", "computer", "--mode", "mcp"]
    }
  }
}
```

The Claude plugin registers both native services under separate names.
For a manual setup, use the entries above. Use the installed plugin's instructions to
initialize Browser JS in the `node_repl` worker; forwarding its MCP catalog
alone does not install those instructions in the receiving agent. Host-specific
backends and approval prompts still depend on the installed runtime and
receiving MCP client.

For example, run `codex-browser-bridge --mode discover` to read
`browser_client_url` (`file:///...`), then call the native `js` tool with that
URL (never paste raw Windows backslashes into a JS string literal):

```js
const { setupBrowserRuntime } = await import("<browser_client_url>");
const agent = await setupBrowserRuntime();
```

Select the browser in official Codex order:

- Explicit browser requested by user: `await agent.browsers.get("iab" | "chrome" | "edge" | "dot" | "extension")` (do not fall back if unavailable).
- Known target URL: `await agent.browsers.getForUrl(url)`.
- Otherwise: `await agent.browsers.getDefault()`.
- Troubleshooting only: `await agent.browsers.list()` and `await agent.documentation.get("bootstrap-troubleshooting")` or `get("chrome-troubleshooting")`.

Read `browser.documentation()` once without truncation:

```js
const browser = await agent.browsers.getDefault();
nodeRepl.write(await browser.documentation());
```

Reuse `agent`, `browser`, and tab handles in `const`/`let` bindings (never
`globalThis`) for later calls, and batch multi-step work or `Promise.all(...)`
inside a single `js` cell to minimize round-trips. If the native documentation
refers to a specific capability guide, fetch that guide when needed rather than
requesting the full documentation again.

## Tool discovery and instructions

Each service exposes its native `js` under the registration's own namespace:
`codex-browser` uses `node_repl`, and `codex-computer` uses `cua_repl`. The
client determines the displayed tool identifier; Codex's `mcp__…__js` name
is a Codex convention. The bridge forwards initialization instructions,
tool descriptions and tool results. The short skill chooses the service;
it does not copy either runtime's full API or confirmation policy.

- For desktop work, call `js` with `await cua.getState()` and read the
  instructions and state returned by the installed runtime.
- For webpage work with the Browser API, bootstrap the installed Browser
  client as above and read its own instructions. A successful plain-JS
  call only proves that the REPL runs; `agent.browsers.list()` checks the
  browser backend. A generic Playwright import is not this bootstrap.
- Keep the same MCP connection across steps. Use `js_reset` when exposed
  and start again from the runtime's instructions after reset. If a host
  provides tool search, it can defer schemas; the bridge does not control
  when that host puts instructions into the model's context.
- Use `doctor` for setup or troubleshooting. During normal work, bootstrap
  once per worker, ask for the state needed for the next decision, and request
  an image when the task needs it. The bridge does not truncate native
  instructions or suppress results to save context.

The bridge does not import Codex's system prompt, app-private context,
secrets, or approval decisions into another MCP host. Backend availability
and policy depend on that installation; a successful handshake is not proof
that a particular backend can execute an action.

## Native methods and authorization

Native catalogs can contain `js`, `js_reset`, `js_add_node_module_dir`, and
`turn_ended`. The installed service and tool filters determine availability.
With the unified configuration, `cua_repl` can also operate browser tabs
through its `cua` API. That API remains separate from the Browser plugin's API.

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
- `--omit-inline-images` (valid only with `--image-dir`) keeps the saved files
  and `bridge_artifacts` metadata but removes the base64 `data` field from each
  image block, so CLI agents can read screenshots from disk without the stdout
  token cost.
- The only top-level options are `--mode`, `--surface`, `--native-config`,
  `--image-dir`, and `--omit-inline-images`; the removed 2.x browser flags are
  rejected by the argument parser before startup.
- The CLI is non-interactive and cancels `elicitation/create` requests with MCP
  result `{"action": "cancel"}`; approve applications through an interactive MCP
  host.

## Lifecycle

Each MCP connection or CLI invocation owns one native worker process tree
(Windows Job Object with `CREATE_NO_WINDOW`, or Unix process group). `js_reset`
clears JavaScript bindings in the worker without closing user applications.

On EOF or interrupt (`SIGINT`/`SIGTERM`), the bridge cancels any in-flight
request, invokes `turn_ended` (`Stop` or `Interrupt`) for the active execution
context with a fresh `callId` and no `progressToken`, closes child `stdin`, and
terminates the owned process tree (8-second `turn_ended` and transport-close
budgets, followed by a 3-second graceful exit window and up to 8 seconds for
process-tree termination). Timed-out or interrupted calls exit nonzero and are
never replayed. Closing the connection with an unanswered tool call is an
interrupt; closing after the calls have settled is a normal stop. Hosts that
keep a connection across turns can send the native lifecycle hook with their
turn metadata. The bridge cannot infer host turn boundaries from MCP alone.
Native JSON-RPC frames are limited to 64 MiB.

## Verification status

Verified on 2026-11-18 against `cua-repl` 0.1.5 / Sky 0.7.21 (Codex host
integration reference `30c75292aa4c8ab614c99f8d7a38e0d60b9347da`). Tracking and
remaining platform checks live in
[issue #57](https://github.com/DeliciousBuding/codex-browser-bridge/issues/57).

| Platform / runtime | Protocol & observation | Input & lifecycle | Notes |
| --- | --- | --- | --- |
| Windows x64 (`node_repl`) | Native handshake and catalog, official Browser client bootstrap and extension discovery passed | Localhost fixture click (`0 -> 1`), tab close, cross-call bindings, `js_reset`, and clean MCP exit passed | Claude Code verified both native connections; its model-driven operation and prompt UI were not part of the test |
| Linux ARM64 X11 (`node_repl`) | Native handshake and catalog, Browser client bootstrap and extension discovery passed | Browser navigation acceptance stopped at the installed runtime's permission check | Claude Code verified both native connections |
| Windows x64 (`cua-repl` 0.1.5 / Sky 0.7.21) | `initialize`, catalog filtering, fresh `accept`/`decline`/`cancel` elicitation, `js_reset`, synthetic AX state, screenshot, and CLI non-interactive rejection passed | Unlocked desktop synthetic Win32 click (`0 -> 1`), `typeText`, active-call cancel without replay, owned worker process-tree cleanup, and fresh-worker retry passed | Verified against a running Codex desktop host pipe; standalone helper spawn without the host returned `spawn EPERM` |
| Linux ARM64 X11 (`cua-repl` 0.1.5 / Sky 0.7.21) | Direct native service and bridge relay both passed `initialize`, catalog, synthetic GTK AX state, screenshot, CLI batch persistence/`js_reset`, `--image-dir`, and `SIGTERM` cleanup | Synthetic GTK button click (`0 -> 1`) and process-group cleanup passed | Requires X11 session variables (`DISPLAY`, `XAUTHORITY`) in the launch row |
| Windows ARM64 / Linux x64 | Release binaries built and CI tested | Not live-tested on desktop GUI | — |
| macOS | CI source build, unit/process tests, clippy, CLI smoke, and both darwin builds on `macos-15`; prebuilt arm64/x64 release binaries are published | Not live-tested with macOS TCC/Accessibility | Grant the installed runtime Accessibility and Screen Recording permissions before desktop control or screenshots |
| Linux Wayland | Untested | Untested | — |
