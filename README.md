# codex-browser-bridge

MCP server and CLI for reusing the installed Codex **Browser Use** (`node_repl`) and **Computer Use** (`cua_repl`) runtimes from Claude Code, Antigravity, Cursor, OpenClaw, Hermes Agent, and shell scripts.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/LICENSE)
[![Latest Release](https://img.shields.io/github/v/release/DeliciousBuding/codex-browser-bridge?style=flat-square)](https://github.com/DeliciousBuding/codex-browser-bridge/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/DeliciousBuding/codex-browser-bridge/ci.yml?style=flat-square)](https://github.com/DeliciousBuding/codex-browser-bridge/actions)
[![Coverage](https://img.shields.io/codecov/c/github/DeliciousBuding/codex-browser-bridge?style=flat-square)](https://codecov.io/gh/DeliciousBuding/codex-browser-bridge)

[Website](https://deliciousbuding.github.io/codex-browser-bridge/) · [中文文档](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/README.zh-CN.md) · [Native Setup](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/native-computer-use.md) · [Upgrading to 3.0](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/upgrade-v3.md)

## Choose an entry

| Entry | Runtime | Use it for |
|---|---|---|
| `--surface native-browser` (default) | Codex `node_repl` | The installed Codex Browser/Chrome plugin JavaScript API (`agent.browsers`, `tab.playwright`, `tab.dom_cua`). |
| `--surface computer` | Codex `cua_repl` | The installed Computer Use `cua` API for desktop windows and unified browser targets. |

Bare invocations without `--surface` use `native-browser`. See [Native setup](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/native-computer-use.md) for launch discovery, host environment policy, and elicitation approvals. Browser Use and Computer Use have separate MCP registrations and worker state.

## Quick Start

### Plugins & Skills

```bash
# Claude Code plugin
claude plugin install codex-browser --marketplace DeliciousBuding/codex-browser-bridge

# Google Antigravity (agy CLI — imports .claude-plugin/plugin.json directly)
agy plugin install https://github.com/DeliciousBuding/codex-browser-bridge

# Agent Skills (skills.sh)
npx skills add DeliciousBuding/codex-browser-bridge
```

Cursor native plugin metadata is provided in [.cursor-plugin/plugin.json](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/.cursor-plugin/plugin.json) and Official MCP Registry metadata in [server.json](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/server.json). The plugin registers both native MCP services (`codex-browser` and `codex-computer`). Install and configure the native Codex runtimes first; the plugin does not bundle them. If only one runtime is installed on your machine, the other entry failing to start is expected—use a single-entry manual configuration if you only want one.

### Manual MCP configuration

```bash
npm i -g @delicious233/codex-browser-bridge   # or download a release binary
```

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

Use only the entry you need if a client needs one service or if only one runtime is installed. Client templates are in [examples/](https://github.com/DeliciousBuding/codex-browser-bridge/tree/main/examples).

### CLI

```bash
codex-browser-bridge --surface native-browser --mode doctor
codex-browser-bridge --surface computer --mode doctor
codex-browser-bridge --surface computer call js '{"code":"await cua.listWindows();"}'
```

Use `batch` for several native JS calls that share one worker. `tools`, `call`, and `batch` do not combine with `--mode`; `--image-dir` applies to `call` and `batch` only. Add `--omit-inline-images` with `--image-dir` to keep the saved files and `bridge_artifacts` while dropping the base64 `data` from stdout.

**Requires:** the configured native runtime for each entry · Node.js 18+ only for the npm/npx path.

## Scope

The binary is a thin, stateless Rust stdio relay of roughly 1,700 lines. It discovers the selected launch row, spawns one owned worker, forwards MCP frames in both directions, applies the launch row's tool filters and environment policy, and tears the worker tree down on exit.

It does not implement CDP, DOM snapshots, locators, observation cursors, tool profiles, or browser sessions. Those belong to the installed runtime, and their behavior, permissions, and catalog are what clients see. The only transport path is stdio to that runtime.

## Upgrading

- **From 2.x to 3.0:** read [docs/upgrade-v3.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/upgrade-v3.md) (the compatibility browser adapter and its structured tools were removed).
- **From 1.x to 2.x:** read [docs/upgrade-v2.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/upgrade-v2.md).

## Platform support

| Platform | Status |
|---------|--------|
| Windows x64 / arm64 | Published binaries |
| Linux GNU x64 / arm64 | Published binaries; glibc >= 2.39 |
| macOS arm64 / x64 | Published binaries; grant the installed runtime Accessibility and Screen Recording (TCC) permissions when macOS prompts |

Native relay behavior follows the installed runtime and its platform support. A Linux/WSL process needs a runtime installed in that environment; the bridge does not bridge a Windows worker across the OS boundary.

## MCP behavior

- The relay forwards the client's handshake, catalog, pagination cursors, unknown tool fields, images, and errors without normalizing schemas or native extensions. The external host owns version negotiation and client request deadlines.
- When a `tools/call` request omits Codex correlation metadata, the relay fills `_meta.callId`, `_meta["x-codex-turn-metadata"]` (`session_id` and `turn_id`), `_meta.sessionId`, and `_meta.threadId`.
- `enabled_tools` is applied before `disabled_tools` across advertised catalogs, direct `tools/call` requests, and the end-of-session `turn_ended` hook.
- Application approval uses bidirectional MCP form elicitation (`elicitation/create`); `accept`, `decline`, and `cancel` responses pass through unchanged. The CLI is non-interactive and responds with `{"action": "cancel"}`.
- On EOF, `SIGINT`, or `SIGTERM`, the bridge cancels in-flight work, sends `turn_ended`, closes the worker's stdin, and terminates the owned process tree (Windows Job Object, Unix process group). Timed-out or interrupted calls are never replayed.

## Maintenance

Upstream Codex desktop app updates do not require a bridge release by themselves. After an upgrade, verify the relay first:

1. `codex-browser-bridge --surface native-browser --mode doctor`
2. `codex-browser-bridge --surface computer --mode doctor`

Ship a patch only for a real launcher/transport contract change or a fixed regression in this repository.

## Security

This tool acts inside your live browser and desktop session.

- Never expose it on a network port; run it only for trusted MCP clients.
- The installed runtime's permissions and approval prompts are authoritative. The bridge adds no second permission layer and does not re-implement CDP access control.
- Review agent actions before allowing sensitive operations; avoid password, payment, and admin pages.
- Redact tab titles, URLs, DOM text, and screenshots before sharing output.
- No telemetry and no hidden fallback runtime.

## Development

```bash
git clone https://github.com/DeliciousBuding/codex-browser-bridge.git
cd codex-browser-bridge
cargo check --locked
cargo test --locked
cargo clippy --locked -- -D warnings
cargo build --locked --release
```

Read [ARCHITECTURE.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/ARCHITECTURE.md) for the module map and design boundaries, and [CONTRIBUTING.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/CONTRIBUTING.md) for conventions. Release steps are in [docs/release-process.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/release-process.md).

## Related

- [examples/](https://github.com/DeliciousBuding/codex-browser-bridge/tree/main/examples) — MCP configs for Claude Code, Antigravity, Cursor, OpenClaw, Hermes Agent, and native Computer Use
- [skills/codex-browser/](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/skills/codex-browser/SKILL.md) — progressive agent skill
- [ARCHITECTURE.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/ARCHITECTURE.md) — module map and design decisions
- [docs/native-computer-use.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/docs/native-computer-use.md) — discovery, environment policy, approvals, and verification status
- [ROADMAP.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/ROADMAP.md) · [CHANGELOG.md](https://github.com/DeliciousBuding/codex-browser-bridge/blob/main/CHANGELOG.md)

## License

MIT. Maintained independently from OpenAI, Anthropic, and Google.

Thanks to [LINUX DO](https://linux.do/) for community support and feedback.
