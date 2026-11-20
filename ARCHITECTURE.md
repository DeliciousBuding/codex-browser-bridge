# Architecture

A contributor's guide to how `codex-browser-bridge` fits together and why it
is shaped the way it is. For *what* each piece does, read the source; this
file records the *decisions*.

## Overview

A single-process Rust MCP/CLI relay over stdio. It has one transport path and
two targets:

- `--surface native-browser` (default) relays the installed Codex `node_repl`
  Browser Use runtime.
- `--surface computer` relays the installed Codex `cua_repl` Computer Use
  runtime.

For either target the bridge discovers the launch row from
`~/.codex/config.toml` (or `$CODEX_HOME/config.toml`, or an explicit
`--native-config` JSON), spawns one owned worker process tree (Windows Job
Object / Unix process group), and relays full-duplex JSON-RPC frames. Tool
schemas, pagination cursors, images, errors, `elicitation/create` approvals,
and the runtime's own MCP extensions pass through unmodified.

```text
Claude plugin marketplace ──installs──▶ routing skill + dual native mcp.json

MCP client ──stdio JSON-RPC──▶ bridge ──stdio──▶ installed Codex node_repl / cua_repl
```

There is no custom CDP client, DOM or observation engine, locator engine,
tool profile system, or browser session layer. Those responsibilities belong
to the installed runtime, and the bridge does not duplicate or shadow them.

Current releases publish Windows x64/arm64, Linux GNU x64/arm64 (glibc >= 2.39
for the Linux release builds), and macOS arm64/x64 binaries. Release builds and
CI do not exercise macOS TCC/Accessibility prompts, so a published macOS binary
does not by itself establish desktop-control permission on a given machine.

The repository is both the Claude plugin source and the generic MCP server
source. The Claude plugin registers `codex-browser` (`--surface native-browser`)
and `codex-computer` (`--surface computer`) alongside a minimal routing skill.
Codex and ChatGPT desktop already have native browser access and are not plugin
targets for this bridge.

## Module map (`src/`)

| Module | Responsibility |
|---|---|
| `main.rs` | CLI entry. `--surface native-browser\|computer`, `--mode mcp\|discover\|doctor`, `--native-config`, `--image-dir`, `--omit-inline-images`, and the `tools`/`call`/`batch` subcommands |
| `native/mod.rs` | Launch discovery, host environment policy, owned worker spawn, full-duplex stdio MCP relay, tool policy, CLI/doctor workers, and `turn_ended` cleanup |
| `native/tests.rs` | Relay, launch-policy, cleanup, and CLI result unit tests for the native module |
| `cli.rs` | Shared CLI subcommand definitions (`tools`, `call`, `batch`) forwarded to the native runtime |
| `logging.rs` | Log initialization |
| `lib.rs` | Module declarations |

## Key design decisions

### A relay, not an adapter

The bridge speaks MCP on both sides. It does not translate between a bridge
tool vocabulary and the runtime's own API. Whatever the installed service
advertises—tool names, schemas, resource extension fields, `_meta`
conventions—reaches the client as-is. This keeps the surface honest: the
client sees the runtime, not a re-encoding of it.

The boundary is deliberate. Anything that would require the bridge to
understand CDP, layout, accessibility, or browser state would be a second
implementation of the runtime, and would drift.

### Host-owned handshake

`run_mcp` forwards the client's `initialize` and subsequent frames to the
worker. The external MCP host negotiates the protocol version, drives
pagination, handles interactive elicitation, and enforces its own request
deadlines. The bridge owns none of those policies.

The CLI/doctor paths are the exception: they need a catalog without a client,
so they request legacy `2025-06-18`, accept the initialize-based versions known
to the native SDK, and traverse paginated `tools/list` responses within bounded
page, item, and cursor limits. Unsupported or missing protocol versions fail
before `notifications/initialized`.

### Tool policy from the launch row

`enabled_tools` is applied before `disabled_tools` across advertised catalogs,
direct `tools/call` requests, and the end-of-session `turn_ended` hook. A
disabled tool request fails locally with `-32602` instead of reaching the
worker. If `turn_ended` is disabled, cleanup skips the hook and terminates
the owned worker tree.

An explicit `turn_ended` is tracked by request ID. The matching execution
context clears and the generated turn rotates only after a successful result.
JSON-RPC errors and `isError: true` retain the context for exit cleanup; an
unanswered hook is cancelled and reported as an unknown outcome without replay.

### Call correlation metadata

When a `tools/call` request omits Codex correlation metadata, the relay fills
`_meta.callId`, `_meta["x-codex-turn-metadata"]` (`session_id` and `turn_id`),
`_meta.sessionId`, and `_meta.threadId` (preferring `thread_id` when
supplied). The relay also tracks the active execution context so the cleanup
hook can be attributed to the right session and turn. Malformed metadata is
rejected with `-32602` without closing the session.

### Launch environment policy

The child is launched with `env_clear()` plus, in order: the platform core
environment variables, literal `env` overrides, explicitly listed `env_vars`
(`local` or default), and custom CA certificate variables resolved to absolute
paths. Everything else is stripped, including Codex's non-inheritable internal
auth tokens (`NODE_REPL_AUTH_TOKEN`, `CODEX_GUARDIAN_DECISIONS_API_KEY`, and
related variables).

On Windows, bare commands such as `python` or `npx` are resolved against the
constructed child `PATH` using the process's `PATHEXT`, matching Codex CLI's
program resolver, and `PATHEXT` is forwarded. On Unix, the launch row must
carry desktop session variables (`DISPLAY`, `XAUTHORITY`, Wayland session
variables) in `env` or `env_vars`; the bridge does not add them.

Discovery never modifies user configuration or plugin caches, and
`discover`/`doctor` report boolean `browser_access_env` presence flags for the
Browser entry without printing values.

### Bounded relay

Frame and buffer limits are fixed rather than configurable:

- 64 MiB per native JSON-RPC frame
- 8 MiB and 256 frames of outbound relay backlog per side
- 1 MiB per `batch` input line
- 100 catalog pages, 2,048 catalog items, and 64 KiB cursors in CLI/doctor
  traversal, with at most 1,024 tracked catalog requests

Relay writes are wrapped in a 10-second timeout. Startup and tool timeouts
default to 30 and 300 seconds and follow `startup_timeout_sec`/
`startup_timeout_ms` and `tool_timeout_sec` from the launch row.

### Deterministic worker ownership

Each MCP connection or CLI invocation owns one worker process tree. On EOF or
interrupt, the bridge cancels in-flight work, invokes `turn_ended` (`Stop` or
`Interrupt`) for the active execution context with a fresh `callId` and no
`progressToken`, closes child stdin, and terminates the tree. The cleanup
budget is 8 seconds for `turn_ended` and transport close, followed by up to 8
seconds for process-tree termination. Timed-out or interrupted calls exit
nonzero and are never replayed.

`js_reset` clears JavaScript bindings in the worker without closing user
applications.

### Claude plugin distribution

The Claude plugin surface is intentionally small:

- `mcp.json` launches the npm package through a version-pinned `npx` entry.
  The first start downloads the package and platform binary; a global npm
  install is optional and keeps later starts on the existing command.
- `.claude-plugin/plugin.json` and `.claude-plugin/marketplace.json` make the
  repository a Claude Code plugin and marketplace. The manifest reuses root
  `mcp.json`; a second `.mcp.json` would make Claude auto-load the same server
  as project MCP and create a duplicate process.

The plugin does not bundle platform executables. `check-plugin-surface.js`
keeps the `npx` package pin equal to the plugin and npm versions.

There is deliberately no `plugin.json`, `.codex-plugin/`, or Codex marketplace.
Codex / ChatGPT desktop already controls its browser extension natively; the
bridge exists for Claude and other non-Codex MCP clients that need to reuse the
same logged-in browser session.

`codex-browser` and `codex-computer` share the relay implementation but own
separate workers; handles cannot move between them.

## Data flow — a tool call

1. MCP client sends `tools/call` (for example `js`) over stdio.
2. `native/mod.rs::relay` checks the tool policy and fills missing correlation
   metadata.
3. The frame is written to the owned worker's stdin.
4. The installed runtime executes it and returns a result or an
   `elicitation/create` request.
5. The relay forwards the frame back to the client, or routes the elicitation
   response to the worker.
6. On exit, `turn_ended` runs and the process tree is closed.

## Testing

- **Windows** (CI `rust` job): unit tests, integration tests, clippy
  `--all-targets`, coverage, x64 + arm64 release build.
- **Linux GNU x64 / arm64** (source CI): native full tests, clippy, and
  release builds. Linux transports are not implied by a Windows pass.
- Source and integration tests use fixture launch rows and mock workers; no
  live desktop app is required.
- `tests/native_process.rs` covers owned-process teardown behavior.

Pure helpers are unit-tested directly — never via `#[path]` source-tree
embedding (that anti-pattern broke the v1.8/v1.9 releases).

## Supply chain

`cargo-deny` (`deny.toml`): advisories + licenses (tight allow list — a new
dependency under an unlisted license fails the check and forces review) + bans
+ sources. Runs on every push/PR plus a weekly schedule. `dependabot` covers
`cargo` (grouped, weekly) and `github-actions`.

## Release

Tagging `v*` triggers `release.yml`: test → build all six assets → checksums
and attestations → stage and publish the GitHub Release → publish npm with
provenance. The npm package embeds release checksums and downloads the matching
binary on install. CI and release also validate that all plugin manifests, MCP
files, marketplace entries, example pins, skill references, and npm versions
remain one coherent distribution surface.
