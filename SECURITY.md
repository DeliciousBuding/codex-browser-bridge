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
- Your environment (OS/architecture, browser version, ChatGPT desktop app/service version)

You can expect an initial response within a few days. Once the issue is confirmed, we will work on a fix and coordinate disclosure with you.

## Scope

This project bridges an existing Chromium browser-extension session to
MCP-compatible agents. Reports about the following are in scope:

- Issues that allow an MCP client to take actions outside the documented tool surface
- Issues in local named-pipe/Unix-socket transport or JSON-RPC framing that could be exploited by a hostile process on the same machine
- Issues with the npm install pipeline (checksum verification, post-install script)
- Issues that leak sensitive data (cookies, tokens, page contents) beyond the MCP transport

### Built-in protections

The bridge applies these safeguards at runtime:

- **URL scheme filtering**: `codex_navigate` blocks `file:`, `javascript:`, `data:`, `vbscript:`, `about:`, `chrome:`, and `edge:` URLs (see `src/browser.rs:validate_url`)
- **Browser identity checks**: only responses identifying themselves as an
  `extension` backend with a family and extension instance ID become browser
  choices. Instances are never selected or reconnected by pipe name alone.
- **Log sanitization**: error messages are stripped of newline characters before writing to the debug log, preventing log injection via crafted error responses
- **JSON safety**: the `jsonEscaped` helper always produces valid JSON strings for JavaScript embedding, with a safe `""` fallback on error instead of propagating corrupted values

Debug logs may still contain local pipe IDs, method names, extension responses, URLs, or page text. Redact logs before sharing them.

The bridge is designed for local use with trusted MCP or CLI clients on the same machine.
Current Linux source discovers `/tmp/codex-browser-use/*.sock`. It rejects
symlinked or unsafe directory components and requires a real socket owned by
the current effective UID with no group/other permissions (the official socket
mode is `0600`). The official shared directory's sticky `1777` mode is accepted.
Dialing checks the endpoint before and after connection and verifies the peer UID.
These checks do not protect against another hostile process running as the same
user. Socket access does not grant the bridge app-private approval services.

The bridge does not inherit all Codex Browser app-side controls. Codex site
approvals, Guardian review, enterprise policy, secure credential handoff, and
artifact-library export live above the extension pipe. Other MCP clients must
apply their own confirmation policy. Do not use the bridge to bypass a Codex
approval or policy decision.

Out of scope:

- MCP client access to your active browser session. This is the project's stated purpose. See the README's "Security" section for the threat model.
- Issues in the ChatGPT desktop app, ChatGPT browser extension, or the browser itself. Please report those upstream.
