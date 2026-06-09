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
- Your environment (Windows version, Chrome version, Codex Desktop version)

You can expect an initial response within a few days. Once the issue is confirmed, we will work on a fix and coordinate disclosure with you.

## Scope

This project bridges your existing Chrome browser session to MCP-compatible agents. Reports about the following are in scope:

- Issues that allow an MCP client to take actions outside the documented tool surface
- Issues in the named-pipe / JSON-RPC framing that could be exploited by a hostile process on the same machine
- Issues with the npm install pipeline (checksum verification, post-install script)
- Issues that leak sensitive data (cookies, tokens, page contents) beyond the MCP transport

### Built-in protections

The bridge applies these safeguards at runtime:

- **URL scheme filtering**: `codex_navigate` blocks `file:`, `javascript:`, `data:`, `vbscript:`, `about:`, `chrome:`, and `edge:` URLs (see `internal/client/browser.go:validateURL`)
- **Pipe spoofing detection**: warns to stderr when multiple `codex-browser-use-*` pipes are found, as this could indicate a hostile process impersonating Codex Desktop
- **Log sanitization**: error messages are stripped of newline characters before writing to the debug log, preventing log injection via crafted error responses
- **JSON safety**: the `jsonEscaped` helper always produces valid JSON strings for JavaScript embedding, with a safe `""` fallback on error instead of propagating corrupted values

Debug logs may still contain local pipe IDs, method names, extension responses, URLs, or page text. Redact logs before sharing them.

The bridge is designed for local use with trusted MCP clients on the same machine.

Out of scope:

- MCP client access to your active browser session. This is the project's stated purpose. See the README's "Security Notes" section for the threat model.
- Issues in Codex Desktop, the Codex Chrome Extension, or Chrome itself. Please report those upstream.
