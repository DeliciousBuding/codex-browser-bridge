# Changelog

All notable changes to this project will be documented in this file.

## [1.5.1] - 2026-06-09

### Fixed

- Serialized per-tab CDP detach, attach, and execute sequences to prevent concurrent actions on the same tab from detaching each other.
- `codex_click` now returns an error when the selector is missing instead of reporting success after a JavaScript exception.
- `codex_dom_click` now rejects short `DOM.getBoxModel` content arrays instead of panicking.
- `codex_wait_for_load` now retries transient navigation-time CDP errors until the timeout.
- Pipe discovery now ignores the bare `codex-browser-use` namespace entry and only returns concrete pipe names.
- MCP JSON-RPC handling now ignores notifications, rejects malformed request envelopes, rejects zero-length frames, and validates required tool arguments before opening pipe calls.
- The npm package now uses a committed JavaScript command wrapper, downloads release assets for the package version instead of `latest`, rejects unsupported CPU architectures, and checks package contents in CI.
- Manual release workflow runs now require a valid `v*` release tag.

## [1.5.0] - 2026-06-05

### Fixed

- **Pipe discovery broken after Codex Desktop 26.602+**: Newer Codex versions create pipe names with backslash separators (`codex-browser-use\<uuid>`) which PowerShell's `Get-ChildItem` treats as directories and skips. Switched to `[System.IO.Directory]::GetFileSystemEntries` and manual prefix stripping to discover both formats.
- **`extractUUID` updated** to strip both `-` and `\` separators after the `codex-browser-use` prefix.
- **Pipe warning threshold** raised from >1 to >2. Old-format and new-format pipes can coexist during Codex upgrades.

## [0.3.0] - 2026-05-19

### Fixed

**Critical (5)**
- MCP buffer limit: `bufio.Reader` 4KB default → 10MB to prevent `ErrBufferFull` crash on real messages
- MCP protocol: `notifications/initialized` no longer produces error response (JSON-RPC 2.0 §4.1)
- CLI whitespace-only input no longer panics (`args[0]` index out of range)
- CLI EOF no longer spins at 100% CPU
- Fill element-not-found now returns an explicit error instead of silently succeeding

**High (7)**
- All 19 MCP tool handlers now check `json.Unmarshal` errors (previously silently zeroed on type mismatch)
- JS injection vector fixed: Go `%q` replaced with `json.Marshal` for JavaScript string literals in Click/Fill
- `Response.ID` changed from `int` to `*int` so `id:0` is not misclassified as notification
- `json.Marshal` errors in `writeResult`/`writeError` and handler `MarshalIndent` calls now checked
- `NavigateBack`/`NavigateForward` now validate both array bounds instead of one
- PowerShell pipe discovery subprocess now has 15s timeout via `context.WithTimeout`
- `readLoop` was blocking send on duplicate responses; now uses non-blocking select

**Medium (5)**
- `CUAType` now dispatches keyDown+char+keyUp sequence per CDP spec; attaches debugger once
- Health check during pipe auto-discovery uses 5s timeout (was 60s)
- CLI `try` command JSON extraction uses `args[2:]` instead of fragile byte offset
- `DOMSnapshot` fallback prepends marker to distinguish plain-text from AX tree
- `DomCUAClick` checks `len(content) >= 5` before box model coordinate access

**Low (10)**
- `newUUID` returns error + `fallbackUUID` via `math/rand` instead of `panic`
- `BRIDGE_DEBUG_LOG` open failure now logs warning to stderr
- `os.Exit` moved out of `runMCP`/`runCLI` into `main()` so deferred cleanup runs
- `extractUUID` uses conditional single-char strip instead of greedy `TrimLeft`
- `time.After` replaced with `time.NewTimer` + deferred `Stop()` to prevent leaks
- `ClaimUserTab` auto-attach error now logged
- Screenshots typo fixed (was already resolved)
- `SendNotification` test coverage added (`TestSendNotificationFrame`)
- `WaitForLoadTimeout` test: `strings.HasPrefix` replaces fragile `[:7]` slice
- E2E Screenshot test now validates non-empty base64 return value

### Security

- URL validation blocks dangerous schemes (`file:`, `javascript:`, `data:`, `vbscript:`) in `codex_navigate`
- Named pipe spoofing warning when multiple `codex-browser-use-*` pipes exist (unexpected state could indicate a hostile process)
- `ErrorObject` log injection sanitization: newline characters stripped from error messages before logging
- `jsonEscaped` error fallback: returns `""` instead of propagating nil/invalid JSON into JavaScript evaluation strings
- Missing MCP `ping` handler added (returns empty result per JSON-RPC 2.0)
- Test mock fidelity improvements: recording server enforces exact method match, duplicate response detection

### Changed

- `codex_dom_get_visible` description clarified: returns human-readable DOM tree (not node IDs); use `codex_dom_snapshot` for accessibility node IDs usable with `codex_dom_click`
- `codex_screenshot` `fullPage` parameter documented as reserved (not yet implemented, always captures viewport)

## [0.2.0] - 2026-05-16

### Added
- `codex_navigate_back` and `codex_navigate_forward` MCP tools (history navigation was already in the client; now exposed)
- `codex_wait_for_load` MCP tool: polls `document.readyState` until `complete` or timeout
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
