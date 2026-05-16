# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2026-05-16

### Added
- `codex_navigate_back` and `codex_navigate_forward` MCP tools (history navigation was already in the client; now exposed)
- `codex_wait_for_load` MCP tool — polls `document.readyState` until `complete` or timeout
- `codex_screenshot` now returns MCP `image` content so agents can view the screenshot directly (previously only base64 text)
- `MCPServer.SetVersion` so the build version flows into the MCP `initialize` handshake (`serverInfo.version`)
- Unit tests across `protocol`, `client`, `discovery`, and `mcp` packages
  - In-memory `net.Pipe` fake server for end-to-end RPC tests without a real Codex pipe
  - Concurrent `SendRequest` stress test under `-race`
  - Wire-format invariants for `executeCdp`, `claimUserTab`, history navigation, JS escaping, CUA event sequencing, DOM box-model math
  - MCP handler integration tests that exercise the full client → MCP path
- `NewMCPServerWithIO` constructor for testable I/O injection
- CI now runs `go test -race -cover`

### Fixed
- `discovery.extractUUID` no longer truncates UUIDs containing hyphens
- Clearer error messages on pipe-not-found and dial failures
- `Makefile install-local` now copies the `.exe` binary on Windows
- Duplicate option numbering in README install sections

### Internal
- `client.NewFromConn` for wrapping an existing `net.Conn` (used by tests)
- `cover.out` and `*.coverprofile` added to `.gitignore`

## [0.1.0] - 2026-05-16

### Added
- Named pipe discovery (`codex-browser-use-*` pipes)
- Pipe connection via go-winio
- Session management: `createTab`, `getTabs`, `getUserTabs`, `claimUserTab`, `closeTab`
- Navigation via CDP: `Page.navigate`, `Page.reload`, `Page.getNavigationHistory`
- Screenshot via CDP: `Page.captureScreenshot` (base64 PNG)
- DOM snapshot via CDP: `Accessibility.getFullAXTree`
- JavaScript evaluation via CDP: `Runtime.evaluate`
- Click/fill via CDP: `Runtime.evaluate` with `querySelector`
- CUA input via CDP: `Input.dispatchMouseEvent`, `Input.dispatchKeyEvent`
- MCP server (stdio JSON-RPC) with 20 tools
- CLI mode for interactive debugging
- Discover mode for listing active pipes

### Key findings
- Wire protocol uses camelCase method names (`getInfo`, not `get_info`)
- `executeCdp` requires `{target: {tabId}}` nested format
- Must call `attach` before any CDP command
- Each pipe connection creates a new browser session
