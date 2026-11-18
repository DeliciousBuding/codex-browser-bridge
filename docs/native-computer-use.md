# Native Computer Use

Computer Use exposes the installed Codex `cua_repl` MCP service to other agents.
The bridge supplies runtime discovery, MCP transport, CLI presentation and
owned worker cleanup. Native JavaScript objects, platform dispatch, permissions,
input, screenshots, documentation and errors stay with that service.

The browser surface remains the default. Select `--surface computer` explicitly.
Codex itself continues using its native integration. Do not install a second
bridge executor for the same Codex session.

## Runtime and setup

Install and start the native Codex desktop runtime on the machine doing the
work. The bridge reads the user-level `[mcp_servers.cua_repl]` table when present,
otherwise the current native plugin's generated `.mcp.json`. Neither source is
modified. `--native-config <file>` selects an explicit MCP JSON containing
`mcpServers.cua_repl`.

The native command, arguments, Node/NodeREPL paths, sandbox and native host
environment are preserved. The bridge selects `computer` and removes the
`browser` trusted-service entry, retaining the installed Sky mapping and other
host services. A missing Sky mapping uses `@oai/sky/service`. A stale executable,
missing host pipe, unsupported platform, locked desktop or native policy failure
returns an error. The adapter does not disable the sandbox, elevate privileges,
create a replacement helper, or download proprietary runtime packages.

```bash
codex-browser-bridge --surface computer --mode discover
codex-browser-bridge --surface computer --mode doctor
```

Discovery checks launch files. Doctor performs MCP initialization, reads the
native catalog and ends its owned worker without GUI calls. Neither proves
application input or platform support.

For another MCP client:

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

The default Claude plugin continues to expose the browser surface. Opt into
Computer Use with this separate registration when needed; do not configure the
same native worker through both a direct registration and the adapter.

## Native methods and authorization

The tested native catalog contains `js`, `js_reset`,
`js_add_node_module_dir` and `turn_ended`. Tools and schemas are read from the
runtime; their number can change independently of the bridge's browser tools.
Read the native `js` instructions and emitted documentation before acting.

Application approval is a bidirectional MCP form elicitation. The adapter keeps
accept, decline, cancel, persistence metadata, images and notifications.
MCP frames retain fields added by the native runtime, including extensions of
known requests, capabilities and tool definitions.
The receiving MCP host must support and present native elicitation; granting
access to the bridge process is not an application approval.

Per-call native session and turn metadata is preserved. Adapter-generated IDs
are correlation only. No approved-app or entitlement metadata is fabricated.
Native parameters and unsupported-method errors are retained, including target
ID types, screenshot freshness, platform scrolling units and held-input limits.
The native `js` annotation is not evidence that arbitrary UI actions are read only.

## CLI

CLI commands use the same native launcher and catalog:

```bash
codex-browser-bridge --surface computer tools
codex-browser-bridge --surface computer tools js
codex-browser-bridge --surface computer call js '{"code":"await cua.getState();"}'
codex-browser-bridge --surface computer --image-dir ./captures batch
```

`batch` reads one `{"name":"js","arguments":{...}}` JSON object per line and
retains the same worker and JS bindings. A one-shot `call` starts a fresh worker.
`--session-file` and `--keep-turn` do not restore desktop objects and are rejected.
Batch input is bounded to 1 MiB per line.

Native JSON/JSONL goes to stdout and diagnostics to stderr. Image blocks retain
their base64 and metadata. `--image-dir` additionally writes image files and
adds `bridge_artifacts` with the matching content indexes, MIME types and paths.
Tool errors exit nonzero and input is never replayed.
Browser selection, profiles, browser output limits, network policies and
`--finalize` are rejected on this surface. Native image data is preserved in
stdout even when `--image-dir` is used.

The CLI is noninteractive: an application that requires approval returns the
native unmet-approval error. It cannot auto-accept an elicitation. Use a capable
MCP host for application approval.

## Lifecycle

One MCP connection owns one native worker. Native objects survive calls;
`js_reset` discards bindings but does not close user apps. A completion summary
is a native task result, separate from reset and turn end.

EOF attempts `turn_ended` for the executing context and closes the owned worker.
Interrupts forward cancellation and attempt `Interrupt` cleanup. Cleanup is
bounded and unconfirmed cleanup is reported. A native hook's acknowledgment does
not by itself prove physical held-input release. Separate workers do not imply
separate physical keyboard, mouse or clipboard.

Cleanup retains the executing session and turn, uses a fresh call ID and does
not reuse the original request's progress token. An interrupted MCP session
exits nonzero. Each relay write has a ten-second deadline; backpressure can
delay the opposite direction by that interval. A failed or interrupted native
write ends the worker because its completion is unknown.

## Verification status

These are candidate-source results, not an installed release claim.

| Platform/runtime | Protocol and observation | Input | Boundary |
| --- | --- | --- | --- |
| Windows x64, `cua-repl` 0.1.5 / Sky 0.7.21 | Native host path: initialize, application decline/accept, synthetic AX state and screenshot passed | Not accepted: native input failed while the test desktop was locked | Requires the running native host and an unlocked interactive desktop; standalone helper returned `spawn EPERM` |
| Linux ARM64 X11, installed native runtime | Direct native service and an earlier adapter: synthetic AX state and screenshot passed; current relay acceptance pending | Earlier adapter: synthetic button count changed from 0 to 1 on both paths | Broader clipboard, dialog, DPI and concurrent-client behavior remains unverified |
| Windows ARM64 / Linux x64 | Binary platform exists; desktop workflow unverified | Unverified | Do not infer desktop parity from the build |
| macOS | No accepted native workflow or released macOS binary | Unverified | Native runtime, GUI/TCC and first-run denial need a Mac |
| Linux Wayland | Unverified | Unverified | X11 evidence does not establish portal/Wayland support |

Source reference: public Codex host integration at
`30c75292aa4c8ab614c99f8d7a38e0d60b9347da`; native service execution was inspected
separately. Public CLI configuration types are not a desktop executor.
Tracking and remaining acceptance work are in issue #57.
