# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added — 11 new MCP tools (37 → 48)

Page info & export:
- **`codex_get_url`** / **`codex_get_title`**: read current URL / title without `codex_evaluate`
- **`codex_wait_for_element`**: poll a CSS selector until it matches. Essential for SPAs where `wait_for_load` returns immediately but content renders async
- **`codex_print_pdf`**: render page to PDF via `Page.printToPDF`
- **`codex_screenshot_element`**: capture a single element via clipped `captureScreenshot`

Interaction:
- **`codex_hover`**: mouseover + mousemove (dropdowns, tooltips, hover cards)
- **`codex_select_option`**: set `<select>` value + fire change/input
- **`codex_drag`**: CDP mouse drag (down → interpolated moves → up)

State & cookies:
- **`codex_storage`**: get/set `localStorage` (login state, tokens, SPA state)
- **`codex_delete_cookies`**: `Network.deleteCookies` (logout / account switch)

Viewport:
- **`codex_emulate_device`**: `Emulation.setDeviceMetricsOverride` (mobile testing), `reset=true` to clear
- **`codex_bring_to_front`**: activate a background tab via `Page.bringToFront` (fixes screenshot/CDP timeouts on throttled tabs)

### Event architecture (B-class) — 2 tools (48 → 50)

- **CDP event subscription** (`client.rs`): the read loop now routes frames that carry a `method` and no `id` (server-pushed CDP events) to subscribers, instead of dropping them. New `subscribe_events(method_prefix)` / `unsubscribe_events(id)` API. Never blocks the read loop on a slow consumer — events are dropped on buffer overflow (`try_send`).
- **`codex_network_monitor`**: capture `Network.*` events for a window (API/XHR/fetch debugging, endpoint reverse-engineering)
- **`codex_get_console_logs`**: capture `Runtime.consoleAPICalled` for a window (frontend error/log debugging)

### Changed

- **Sticky attach fast-path timeout** (`client.rs`): sticky CDP calls now use an independent 20s deadline instead of sharing the 60s budget. A background tab that goes silent fails in 20s instead of burning the full timeout, and the full re-attach path gets a fresh budget to retry.
- **`codex_screenshot` description**: documents that a timeout means the tab is likely background-throttled, with a pointer to `codex_bring_to_front`.
- **Profiles**: `basic` 26→32, `network` 33→46, `full` 37→48.

## [1.9.0] - 2026-06-20

### Added — 8 new MCP tools (28 → 36)

- **`codex_file_input`** (`[Input]`): Upload files to `<input type=file>` via `DOM.setFileInputFiles`. Security: path traversal defense via `canonicalize` + prefix check, max 10 MB, regular files only. Configurable via `CODEX_BRIDGE_UPLOAD_BASE` env.
- **`codex_dialog`** (`[Page]`): Handle JavaScript dialogs (alert/confirm/prompt) via `Page.handleJavaScriptDialog`. Accept/dismiss with optional prompt text.
- **`codex_find_element`** (`[DOM]`): Find elements by ARIA role and/or accessible name in the AX tree. Returns node IDs for use with `codex_click_element`.
- **`codex_click_element`** (`[Input]`): Click by accessibility node ID via `DOM.resolveNode` → `DOM.getBoxModel` → Input dispatch. No JS injection.
- **`codex_nav_and_wait`** (`[Navigation]`): Composite: navigate + wait_for_load in one MCP call.
- **`codex_click_and_wait`** (`[Input]`): Composite: click + wait_for_load in one MCP call.
- **`codex_form_fill`** (`[Input]`): Fill multiple form fields via `{selector: value}` map, optionally submit.
- **`codex_doctor`** (`[Session]`): Self-diagnostics — enumerate pipes, probe connectivity, report latencies and browser versions.

### Architecture

- **mcp.rs module split**: 815-line monolith → `src/mcp/{mod,types,schema,handlers}.rs`, each <400 lines.
- **Centralized security**: `src/security.rs` — `validate_url`, `validate_file_path` with path traversal prevention.
- **Tool profiles**: `basic` (25 tools) / `network` (32) / `full` (36) via `CODEX_BRIDGE_PROFILE` env or `--profile` CLI flag.

### Changed

- `validate_url` moved from `browser.rs` → `security.rs` (single canonical source).
- `Tool` visibility relaxed to `pub(crate)` for profile filtering.
- `Server` struct gains `new_with_profile()` constructor.

### Documentation

- Updated ROADMAP.md with v1.9.0 completion status and SUPER scores.

## [1.8.0] - 2026-06-20

### Added

- **CDP error normalization** (`P0-1`): CDP-level errors (Target closed, etc.) now correctly surface as `isError: true` in MCP responses. New `BridgeError::Cdp` variant with method name and code.
- **Sticky attach** (`P0-2`): Per-tab CDP session caching. Skips redundant detach+attach round-trips for repeated CDP calls on the same tab, reducing RTT by ~50% for multi-step agent operations.
- **`codex_network_cookies`**: Cookie values redacted by default (`redact_values: true`).
- **`codex_network_set_cookie`**: URL validation enforced.
- **`codex_execute_cdp`**: CDP method allowlist blocks Browser/Debugger/Target/Emulation/Security/Tracing domains.
- **`codex_page_assets`**: Exposes Codex extension `pageAssets` capability with frameId-aware resource fetching.

### Changed

- **Tool descriptions unified**: All 28 tools now have group tags (`[Tabs]`, `[Navigation]`, `[DOM]`, `[Page]`, `[Input]`, `[CDP]`, `[Network]`, `[Session]`), cross-references, and clearer LLM guidance.
- **`fullPage` → `full_page`**: Schema parameter renamed to snake_case (backward-compatible via fallback).
- **`timeout_ms`**: Schema type `number` → `integer`.
- **CLI extracted** (`S` principle): `browser.rs` 1100→857 lines, CLI REPL now in `src/cli.rs`.

### Removed

- **BridgeClient trait**: Removed as over-engineering (3/3 agent reviewers + ChatGPT agreed). No mock consumer existed. KISS principle restored — `browser.rs` uses `&Client` directly.
- **Go legacy**: `internal/`, `cmd/`, `go.mod`, `go.sum`, `.golangci.yml` removed (−5382 lines).
- **`is_debugger_error`**: Replaced by broader `is_session_invalid_error` with 8 patterns.

### Performance

- **`encode_frame`**: Length header + payload merged into single `write_all` (1 syscall vs 2).
- **MCP stdio**: `BufReader::read_until` with reusable `Vec` buffer replaces per-line `String` allocation.
- **Sticky attach**: Skips detach+attach when CDP session is cached (50-60% RTT reduction for repeated calls on same tab).

### Security

- **CDP error sanitization**: Error messages stripped of `\n` `\r` before surfacing (matching RPC error handling).
- **Session cache cleanup**: `attached_tabs` cleared on `finalize`, populated from `claim_user_tab`, self-healing on errors.
- **Cookie value redaction**: Default-on for `codex_network_cookies`.

### Governance

- **Repository SEO**: 15 GitHub topics, Discussions enabled, homepage set.
- **npm metadata**: 15 keywords, `homepage` + `bugs` fields added.
- **Client examples**: `examples/` (claude-code, openclaw, hermes-agent, cursor).
- **`cargo clippy`**: Added to CI + release workflow with `-D warnings`.
- **Go module caching**: `setup-go` cache enabled.
- **Codecov**: Rust coverage via `cargo-llvm-cov` (replaces old Go coverage).
- **`.gitignore`**: `docs/ask-gpt/` added (external review artifacts never committed).

### Documentation

- **README.zh-CN**: Synchronized with English README (Go references removed, npm recommended).
- **CONTRIBUTING.md**: Rewritten for Rust-only development workflow.
- **SECURITY.md**: Stale paths fixed.
- **ROADMAP.md**: Updated with v1.7.0, v1.8.0, v1.9.0, v2.0.0 plans and SUPER scoring.
- **ChatGPT architecture review**: External audit confirmed architecture direction, identified P0 priorities, validated BridgeClient removal.

### Reviews

- **SUPER multi-dimension review**: CDP error normalization + sticky attach scored PASS_WITH_FIXES (3/5), all 5 must-fix items resolved.
- **3-agent architecture review**: Consensus on BridgeClient removal, sticky attach priority, CDP error importance.
- **ChatGPT independent audit**: Validated architecture, recommended priorities (P0 CDP error → P0 sticky attach → P1 tool UX).

## [1.7.0] - 2026-06-20

### Added

- `codex_execute_cdp` — generic CDP executor. Pass any Chrome DevTools Protocol method name and params. Universal escape hatch for all CDP domains (Network, Performance, Storage, Emulation, etc.).
- `codex_page_assets` — exposes the Codex extension's `pageAssets` capability. Lists all page resources (images, fonts, CSS, JS) via `Page.getResourceTree`. Optional content fetch via `Page.getResourceContent` with base64 return.
- `codex_network_cookies` — get cookies via `Network.getCookies`. Supports URL filtering.
- `codex_network_set_cookie` — set a cookie via `Network.setCookie`. Supports all standard cookie attributes.

### Changed

- MCP tool count: 24 → 28
- Browser helpers now include: `execute_cdp_generic`, `get_resource_tree`, `get_resource_content`, `get_cookies`, `set_cookie`

### Tests

- New e2e integration test suite (`tests/cdp_tools_e2e.rs`) with mock pipe infrastructure
- 7 new e2e tests: CDP roundtrip, resource tree parsing, cookie parsing, error propagation
- 4 new schema validation tests for new tools
- Total test count: 85 → 92 (all passing)

### Governance

- AGENTS.md now documents MCP tool design patterns and security rules
- Public repo security audit: no secrets, tokens, or sensitive data found

## [1.6.0] - 2026-06-09

### Added

- Rebuilt the bridge binary in Rust with parity coverage for the extension wire protocol, pipe discovery, MCP tools, CLI commands, and browser helpers.
- CI now has a Windows Rust rewrite job for `cargo check`, `cargo test`, and x64/arm64 release-mode builds.
- Release validation now checks `Cargo.toml` against the release tag and extracts notes from the tagged changelog section.
- Rust rewrite docs now describe the npm compatibility plan and local build commands.

## [1.5.4] - 2026-06-09

### Fixed

- `codex_close_tab` now retires the tab CDP lock when Chrome reports that the target is already gone.

## [1.5.3] - 2026-06-09

### Fixed

- Release workflow now publishes the npm package after GitHub Release assets are created.
- Manual release runs now checkout the requested tag before test, lint, asset build, and npm publish jobs.
- npm publish now verifies that `npm/package.json` matches the release tag before publishing.
- npm installer now prefers package-embedded binary checksums, with the release checksum file kept as a fallback for older packages and development overrides.
- CUA click, type, and keypress actions now keep each high-level input sequence under one tab lock and retry debugger detach errors inside the sequence.
- `codex_wait_for_load` now bounds each CDP request by the caller's timeout.
- CDP tab locks are retired after successful tab close and session finalization.
- README, SECURITY, and issue templates now name the local browser data that must be redacted before sharing output.

## [1.5.2] - 2026-06-09

### Fixed

- Release tags now run test and lint jobs before GitHub Release assets are created.
- CI now runs Node tests for installer checksum parsing.
- Public issue and contributing docs now ask reporters to redact browser-local data before posting.
- Ignore rules now cover generated coverage files, npm pack output, and package archives.
- npm installer downloads from the project release for the package version by default. Development download overrides require `CODEX_BRIDGE_ALLOW_DEV_DOWNLOADS=1`.
- README wording now states project relationships and browser-session permissions directly.

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
