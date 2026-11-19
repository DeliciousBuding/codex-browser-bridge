# Security Policy

## Reporting a vulnerability

If you find a security issue, please **do not** open a public GitHub issue.

Instead, use one of these private channels:

- GitHub's [private vulnerability reporting](https://github.com/DeliciousBuding/codex-browser-bridge/security/advisories/new)
- Email the maintainer via the address listed on the [GitHub profile](https://github.com/DeliciousBuding)

Please include:

- A description of the issue and its impact
- Steps to reproduce, or a proof-of-concept
- The affected version (output of `codex-browser-bridge --version`)
- Your environment (OS/architecture, Codex app/CLI version, MCP host)

You can expect an initial response within a few days. Once the issue is confirmed, we will work on a fix and coordinate disclosure with you.

## Scope

`codex-browser-bridge` is a local stdin/stdout MCP and CLI relay to the
installed Codex native runtimes (`node_repl` and `cua_repl`). It opens no
network listening sockets and implements no custom browser CDP/DOM adapter.

Reports about the following are in scope:

- Issues that allow an MCP client to bypass configured `enabled_tools` / `disabled_tools` policies or execute commands outside the discovered launch row
- Issues in stdio JSON-RPC framing, backpressure bounds, or process-tree cleanup that cause resource exhaustion or orphaned privileged child processes
- Issues in `~/.codex/config.toml` parsing or error diagnostics that leak host secrets, tokens, or raw config lines
- Issues with the npm install pipeline (SHA-256 checksum verification, post-install script, release provenance)

### Built-in protections

- **Launch-row environment policy**: only variables declared in the discovered Codex launch row and OS core allowlist are forwarded; `NON_INHERITABLE_ENV_VARS` are stripped, and malformed `~/.codex/config.toml` parse errors are redacted to line numbers only.
- **Process lifecycle isolation**: child runtimes are wrapped in a Windows Job Object / Unix process group with `KillOnDrop` and bounded `turn_ended` shutdown cleanup.
- **Bounded framing and backpressure**: native frames are capped at 64 MiB, CLI input lines at 1 MiB, and outbound relay queues at 256 frames / 8 MiB.
- **Interactive approval preservation**: native `elicitation/create` requests are forwarded verbatim to interactive MCP hosts; non-interactive CLI workers respond with `{"action": "cancel"}` and never auto-approve.

Debug logs (`RUST_LOG=debug`) may contain tool names, paths, or error messages. Redact logs before sharing them.

### Out of scope

- Authorized MCP client access to your local Codex Browser Use or Computer Use runtime (that is the project's stated purpose).
- Issues in the Codex desktop app, Codex CLI, `node_repl`, `cua_repl`, or the Chromium browser itself. Please report those upstream.
