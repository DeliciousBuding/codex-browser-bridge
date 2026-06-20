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

## v1.8.0: Performance + Architecture hardening

### SUPER Score Baseline

| Principle | Current | Target | Gap |
|-----------|:------:|:------:|:---:|
| **S**ingle Purpose | 4/5 | 5/5 | Extract CLI from browser.rs |
| **U**nidirectional Flow | 5/5 | 5/5 | — |
| **P**orts over Implementation | 4/5 | 5/5 | `ClientTrait` for mock testing |
| **E**nvironment-Agnostic | 3/5 | 4/5 | Unix socket fallback scaffold |
| **R**eplaceable Parts | 4/5 | 5/5 | `ClientTrait` enables faster unit tests |
| **Total** | **20/25** | **24/25** | |

### S: Extract CLI module

- [ ] Move `run_cli()` and `split_args()` from `src/browser.rs` → `src/cli.rs`
- [ ] `browser.rs` drops from ~1000 → ~850 lines (pure CDP + parsing)
- [ ] No behavior change, mechanical refactor
- **Effort**: S

### R+P: ClientTrait for mockability

- [ ] Define `ClientTrait` with `send_request()` and `execute_cdp()`
- [ ] Implement for real `Client` (zero-cost abstraction via static dispatch)
- [ ] Implement `MockClient` for unit tests
- [ ] Convert browser.rs functions to generic over `C: ClientTrait`
- [ ] Unit tests become ~10x faster (no real pipe, no tokio spawn)
- **Effort**: M

### Performance: Protocol layer

- [ ] `encode_frame`: use `write_vectored` → 1 syscall instead of 2
- [ ] `decode_frame`: avoid zero-init with `unsafe { set_len }` → saves memset on MB frames
- [ ] MCP stdio: replace `BufReader::lines()` with raw buffer → avoids per-line String alloc
- [ ] Benchmark before/after with criterion
- **Effort**: M

### E: Unix socket scaffold

- [ ] `#[cfg(not(windows))]` add `tokio::net::UnixStream` transport
- [ ] Refactor `pipe.rs` → `transport.rs` with `windows` and `unix` modules
- [ ] WSL path detection: auto-select Windows named pipe from WSL guest
- [ ] Non-Windows `dial_named_pipe` → clearer error suggesting WSL
- **Effort**: M

### MCP client examples

- [ ] `examples/claude-code.json` — Claude Code MCP config
- [ ] `examples/openclaw.json` — OpenClaw MCP config
- [ ] `examples/hermes-agent.json` — Hermes Agent MCP config
- [ ] `examples/cursor.json` — Cursor MCP config
- [ ] All examples identical (stdio MCP is universal) with platform-specific path notes
- **Effort**: S

---

## v1.9.0: New features

- [ ] `codex_fullpage_screenshot` — true full-page capture via CDP clip + scroll compose + stitch
- [ ] `codex_network_monitor` — `Network.enable` + buffered event capture for request/response inspection
- [ ] `codex_emulate_device` — `Emulation.setDeviceMetricsOverride` for mobile viewport testing
- [ ] `codex_storage` — localStorage/sessionStorage read/write via `Runtime.evaluate`

---

## v2.0.0: Cross-platform release

- [ ] Full Unix socket transport (macOS, Linux)
- [ ] Non-Windows CI matrix (ubuntu-latest, macos-latest)
- [ ] npm `os` field expanded to include `darwin`, `linux`
- [ ] Cross-platform E2E test suite

---

## Backlog

- [ ] `codex_execute_cdp` user-customizable allowlist (config file or env var)
- [ ] Screenshot format options (JPEG, WebP quality param)
- [ ] Typed tool result schemas for structured agent consumption
- [ ] `codex_performance_trace` — `Performance.enable` + trace export
- [ ] OpenGraph social share image
- [ ] `cargo bench` criterion benchmarks for protocol layer
- [ ] `codex_console` — `Runtime.evaluate` with `console.log` capture via `Runtime.consoleAPICalled`
