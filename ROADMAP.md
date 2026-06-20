# ROADMAP

## v1.7.0: CDP MCP tools + security hardening (2026-06-20)

Expose all Codex Chrome Extension v1.1.5 capabilities as MCP tools, with comprehensive security review and testing.

### Added
- [x] `codex_execute_cdp` — generic CDP executor with security allowlist (blocks Browser, Debugger, Target, etc.)
- [x] `codex_page_assets` — expose extension `pageAssets` capability via `Page.getResourceTree` + `Page.getResourceContent`
- [x] `codex_network_cookies` — get cookies via `Network.getCookies` (values redacted by default)
- [x] `codex_network_set_cookie` — set cookies via `Network.setCookie` (URL validation enforced)
- [x] CDP security allowlist (BLOCKED_CDP_DOMAINS)
- [x] Cookie value redaction (default: `redact_values=true`)
- [x] FrameId-aware resource content fetching
- [x] Log sanitization (`sanitize_for_log`)

### Tests
- [x] E2E test suite: `tests/cdp_tools_e2e.rs` (9 tests with mock pipe infrastructure)
- [x] Schema validation tests against actual `registered_tools()` (not hardcoded copies)
- [x] 110+ tests passing (up from 85)

### Governance
- [x] AGENTS.md: MCP tool design patterns
- [x] SUPER multi-dimension review: all 9 must-fix items resolved
- [x] Public repo security audit: clean
- [x] `cargo clippy` in CI (zero warnings)
- [x] CI simplified: Go legacy removed, pure Rust

### Release
- [x] GitHub Release v1.7.0
- [x] npm: `@delicious233/codex-browser-bridge@1.7.0`

---

## v1.6.0: Rust rewrite (2026-06-09)

- [x] Rebuilt bridge binary in Rust with parity for all Go features
- [x] CI: Rust check, test, build x64/arm64
- [x] Release validation: Cargo.toml + npm version vs tag

---

## v1.5.0: Codex 26.602+ pipe discovery fix (2026-06-05)

- [x] Backslash pipe name format support
- [x] `extractUUID` handles both `-` and `\` separators

---

## Future

### v1.8.0 (planned)

- [ ] `codex_fullpage_screenshot` — implement `fullPage: true` via CDP clip + scroll compose
- [ ] `codex_network_monitor` — `Network.enable` + event capture for request/response inspection
- [ ] `codex_emulate_device` — `Emulation.setDeviceMetricsOverride` for mobile testing
- [ ] `codex_storage` — localStorage/sessionStorage read/write via `Runtime.evaluate`
- [ ] Non-Windows transport abstraction (Unix sockets / TCP fallback for WSL)

### Backlog

- [ ] Allowlist/custom-rules for sensitive domains (user-configurable)
- [ ] Screenshot output format options (JPEG, WebP)
- [ ] Tab screenshot capture with configurable viewport dimensions
- [ ] Typed tool result schemas for structured agent consumption
- [ ] CLI `execute-cdp` subcommand for quick CDP debugging (without MCP)
