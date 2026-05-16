# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- Unit tests for `protocol`, `client`, `discovery`, and `mcp` packages
- `NewMCPServerWithIO` for testable I/O injection

### Fixed
- `discovery.extractUUID` no longer truncates UUIDs containing hyphens
- Better error messages on pipe-not-found and dial failures

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
