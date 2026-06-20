# ROADMAP

## v1.7.0: CDP MCP tools + security hardening ✅ (2026-06-20)

Expose all Codex Chrome Extension v1.1.5 capabilities as MCP tools, with comprehensive security review and testing.

- [x] `codex_execute_cdp`, `codex_page_assets`, `codex_network_cookies`, `codex_network_set_cookie`
- [x] CDP allowlist, cookie redaction, URL validation, log sanitization
- [x] 110+ tests, 9 e2e tests, SUPER multi-dimension review (all must-fix resolved)
- [x] Go legacy removed, CI pure Rust, clippy zero-warning
- [x] GitHub Release v1.7.0 + npm `@delicious233/codex-browser-bridge@1.7.0`
- [x] SEO: 15 repo topics, Discussions enabled, npm keywords + homepage + bugs

---

## v1.7.1: Cleanup & polish ✅ (2026-06-20)

- [x] Remove stale Go badges/docs/instructions from README and zh-CN README
- [x] Rewrite CONTRIBUTING.md (Rust-only)
- [x] Fix SECURITY.md stale paths
- [x] Re-enable Codecov badge (now Rust `cargo-llvm-cov`)
- [x] Add `codecov.yml` config

---

## v1.8.0: CDP error normalization + Sticky attach ✅ (2026-06-20)

### SUPER Score (post v1.8.0)

| Principle | Score | Evidence |
|-----------|:-----:|----------|
| **S**ingle Purpose | 5/5 | CLI extracted to `cli.rs`, browser.rs 857 lines pure CDP |
| **U**nidirectional Flow | 5/5 | `Client → Browser → MCP` one-way, no circular deps |
| **P**orts over Implementation | 5/5 | Protocol frames are the contract; `BridgeError` variants are typed interfaces |
| **E**nvironment-Agnostic | 3/5 | Windows-only; Unix socket scaffold pending v2.0.0 |
| **R**eplaceable Parts | 4/5 | CDP allowlist, configurable timeouts; no mock Client (trait removed — over-engineering) |
| **Total** | **22/25** | |

### Completed

- [x] **CDP error normalization** (P0-1): CDP-level errors surface as `isError: true` via `BridgeError::Cdp`
- [x] **Sticky attach** (P0-2): Per-tab CDP session cache, ~50% RTT reduction
- [x] **CLI extracted** (S): `browser.rs` 1100→857 lines, `src/cli.rs`
- [x] **BridgeClient trait removed**: 3/3 reviewers + ChatGPT agreed over-engineering; `browser.rs` uses `&Client` directly
- [x] **Go legacy removed**: −5382 lines, CI pure Rust
- [x] **Protocol optimization**: `encode_frame` single `write_all`, MCP stdio `BufReader` with reusable buffer
- [x] **Tool descriptions unified**: 28 tools with `[Group]` tags, snake_case params, `timeout_ms` integer type
- [x] **MCP client examples**: `examples/` (claude-code, openclaw, hermes-agent, cursor)
- [x] **CI/CD hardened**: clippy `-D warnings`, Codecov via `cargo-llvm-cov`, Go module caching
- [x] **Repository SEO**: 15 topics, Discussions, npm keywords+homepage+bugs
- [x] **ChatGPT architecture review**: External audit validated direction

### Design decision: No Client trait

After implementing and then removing `BridgeClient`, the conclusion is firm: a mock client trait is over-engineering for this codebase. The Client wraps a Windows named pipe — mocking that requires either a real pipe (already fast) or an async channel (adds complexity without benefit). Current lib tests (17) run in <50ms. The E2E tests use real named pipes with a mock CDP server. KISS principle prevails.

---

## v1.9.0: Agent UX + Browser primitives ✅ (2026-06-20)

Focus: make agents more effective at browser automation. Prioritize tools that reduce round-trips and improve reliability.

### P0: mcp.rs 模块拆分

- [ ] Split `src/mcp.rs` (~750 lines) → `src/mcp/` directory
- [ ] `src/mcp/mod.rs` — re-exports, `register_tools()`, handler dispatch
- [ ] `src/mcp/handlers.rs` — all 28 `handle_*` functions
- [ ] `src/mcp/schema.rs` — tool schema definitions + `registered_tools()`
- [ ] `src/mcp/types.rs` — shared helper types (`ToolHandler`, arg extractors)
- [ ] No behavior change, pure module refactor
- **Effort**: M

### P1: file upload support

- [ ] `codex_file_input` — `DOM.setFileInputFiles` for `<input type="file">`
- [ ] Accept local file path, validate existence + readability
- [ ] Security: path traversal prevention, only regular files
- **Effort**: S

### P1: alert/dialog handling

- [ ] `codex_dialog` — `Page.handleJavaScriptDialog` (accept/dismiss)
- [ ] `codex_wait_for_dialog` — poll `Page.javascriptDialogOpening` event
- [ ] Accept/dismiss with optional prompt text
- **Effort**: S

### P2: locator layer

- [ ] `codex_find_element` — locate by role+name (ARIA), not CSS selector
- [ ] `codex_click_element` — click via accessibility node ID (no JS injection)
- [ ] `codex_type_element` — type into focused/identified element
- [ ] Reduces agent's dependency on CSS selector guesswork
- **Effort**: M

### P2: composite tools

- [ ] `codex_nav_open_and_wait` — navigate + wait_for_load in one call
- [ ] `codex_click_and_wait` — click + wait_for_load/navigation
- [ ] `codex_form_fill` — accept `{selector: value}` map, dispatch all inputs
- [ ] Reduces MCP round-trips for common multi-step patterns
- **Effort**: M

### P2: tool profiles

- [ ] `basic` profile — core 12 tools (tabs + nav + dom + screenshot)
- [ ] `network` profile — basic + cookies + CDP network
- [ ] `full` profile — all 28 tools
- [ ] Configurable via env `CODEX_BRIDGE_PROFILE` or CLI `--profile`
- [ ] Reduces LLM tool choice fatigue
- **Effort**: S

### P2: codex_doctor diagnostic

- [ ] `codex_doctor` tool — self-diagnostic check
- [ ] Verify Codex Desktop + Chrome + Extension presence
- [ ] Report pipe count, version, health check latency
- [ ] Useful for agent self-debugging before starting browser operations
- **Effort**: S

---

## v2.0.0: Cross-platform release

- [ ] Full Unix socket transport (macOS, Linux)
- [ ] macOS: Codex Desktop on Darwin uses Unix domain sockets (different from Windows named pipes)
- [ ] Non-Windows CI matrix (ubuntu-latest, macos-latest)
- [ ] npm `os` field expanded to include `darwin`, `linux`
- [ ] Cross-platform E2E test suite
- [ ] WSL path detection: auto-select Windows named pipe from WSL guest
- **Effort**: L

---

## Backlog

- [ ] Screenshot format options (JPEG, WebP quality param)
- [ ] `codex_emulate_device` — `Emulation.setDeviceMetricsOverride` for mobile viewport testing
- [ ] `codex_storage` — localStorage/sessionStorage read/write via `Runtime.evaluate`
- [ ] `codex_console` — `Runtime.evaluate` with `console.log` capture via `Runtime.consoleAPICalled`
- [ ] `codex_network_monitor` — `Network.enable` + buffered event capture for request/response inspection
- [ ] `codex_execute_cdp` user-customizable allowlist (config file or env var)
- [ ] Typed tool result schemas for structured agent consumption
- [ ] `codex_performance_trace` — `Performance.enable` + trace export
- [ ] OpenGraph social share image
- [ ] `cargo bench` criterion benchmarks for protocol layer
- [ ] macOS: Codex Desktop browser bridge uses Unix domain sockets — needs research on actual socket naming convention
- [ ] macOS: Test with real Codex Desktop + Chrome + Extension on Darwin
