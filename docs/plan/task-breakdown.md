# Task Breakdown: Codex Extension Full Capability Exposure

## Task Definition

Expose all Codex Chrome Extension v1.1.5 capabilities through the bridge's MCP layer:
- Generic CDP executor (universal escape hatch)
- Page assets (extension-declared `pageAssets` capability)
- Network cookie operations

**Scope**: `src/mcp.rs`, `src/browser.rs` (additive only)
**Target**: 4 new MCP tools, all using existing `executeCdp` port
**Hard constraint**: Zero breaking changes to existing tools

## Phases

### Phase 1: Core CDP & Page Assets (P0)

| ID | Task | Priority | Effort | Deps | Lane | S.U.P.E.R |
|----|------|:---:|:---:|------|:---:|-----------|
| T1 | Add `codex_execute_cdp` tool — generic CDP executor | P0 | S | None | A | P🅿️ R🆁 |
| T2 | Add `codex_page_assets` tool — list + bundle page resources | P0 | M | T1 | B | P🅿️ S🆂 |
| T3 | Add browser helpers: `execute_cdp_generic()`, `get_resource_tree()`, `get_resource_content()` | P0 | S | None | A | S🆂 P🅿️ |
| T4 | Unit tests for T1, T2, T3 | P0 | M | T1,T2,T3 | — | R🆁 |

### Phase 2: Network Domain (P1)

| ID | Task | Priority | Effort | Deps | Lane | S.U.P.E.R |
|----|------|:---:|:---:|------|:---:|-----------|
| T5 | Add `codex_network_cookies` tool — get cookies for URL(s) | P1 | S | None | A | P🅿️ |
| T6 | Add `codex_network_set_cookie` tool — set a cookie | P1 | S | None | B | P🅿️ |
| T7 | Add browser helpers: `get_cookies()`, `set_cookie()` | P1 | S | None | A | S🆂 |
| T8 | Unit tests for T5, T6, T7 | P1 | M | T5,T6,T7 | — | R🆁 |

### Phase 3: Integration & Review

| ID | Task | Priority | Effort | Deps | Lane | S.U.P.E.R |
|----|------|:---:|:---:|------|:---:|-----------|
| T9 | Build release binary + integration smoke test | P0 | M | T4,T8 | — | R🆁 |
| T10 | Multi-agent Code Review (correctness + security + simplify) | P0 | M | T9 | — | R🆁 |
| T11 | Update AGENTS.md governance surface | P2 | S | T9 | — | — |

### Effort Legend: S=Small(<30min) M=Medium(30-90min) L=Large(90min+)

## Task Details

### T1: `codex_execute_cdp` tool

**Description**: Add a generic MCP tool that accepts a CDP method name and params object, executes it, and returns the raw result.
**Acceptance Criteria**:
- Tool registered as `codex_execute_cdp` with schema: `{tab_id: string, method: string, params: object}`
- Any valid CDP method works (Page.*, Runtime.*, Network.*, DOM.*, etc.)
- Invalid CDP method returns a clear error from the extension
- Tab attachment is automatic (reuses existing attach/detach lifecycle)
**Test Expectation**: Unit test for handler dispatch + schema validation + error path
**S.U.P.E.R Design Drivers**: P (Ports over Implementation) — this IS the universal port

### T2: `codex_page_assets` tool

**Description**: Expose the extension's `pageAssets` capability. Uses `Page.getResourceTree` CDP to list all page resources (images, fonts, CSS, JS, etc.) and optionally fetches content via `Page.getResourceContent`.
**Acceptance Criteria**:
- Tool registered as `codex_page_assets` with schema: `{tab_id: string, include_content: boolean, types: string[]}`
- Returns structured list: `[{url, type, mimeType, size, content?(base64)}]`
- `types` filter works (e.g. `["Image", "Stylesheet"]`)
- `include_content: false` returns metadata only
**Test Expectation**: Unit test for resource tree parsing + content base64 encoding

### T3: Browser helpers

**Description**: Add `execute_cdp_generic()`, `get_resource_tree()`, `get_resource_content()` to `src/browser.rs`.
**Acceptance Criteria**:
- `execute_cdp_generic(client, tab_id, method, params)` — generic CDP wrapper
- `get_resource_tree(client, tab_id)` — calls Page.getResourceTree, returns parsed tree
- `get_resource_content(client, tab_id, frame_id, url)` — calls Page.getResourceContent, returns base64
**Test Expectation**: Unit test for resource tree JSON parsing

### T4: Phase 1 Tests

**Description**: Unit tests covering T1-T3.
**Acceptance Criteria**:
- Test: `codex_execute_cdp` schema validation (missing tab_id, missing method, invalid params)
- Test: `codex_page_assets` resource tree parsing from mock CDP response
- Test: `get_resource_content` result parsing
- Test: `execute_cdp_generic` error propagation
- `cargo test --locked` passes
**Test Expectation**: All new tests pass

### T5: `codex_network_cookies` tool

**Description**: Get cookies via CDP `Network.getCookies`. Supports filtering by URL(s).
**Acceptance Criteria**:
- Tool registered as `codex_network_cookies` with schema: `{tab_id: string, urls: string[]}`
- Without urls: gets cookies for current page URL (from tab info)
- With urls: gets cookies matching those URLs
- Returns structured list: `[{name, value, domain, path, expires, httpOnly, secure, sameSite}]`
**Test Expectation**: Unit test for cookie list parsing

### T6: `codex_network_set_cookie` tool

**Description**: Set a cookie via CDP `Network.setCookie`.
**Acceptance Criteria**:
- Tool registered as `codex_network_set_cookie` with schema: `{tab_id: string, name: string, value: string, url?: string, domain?: string, path?: string, httpOnly?: boolean, secure?: boolean}`
- Sets cookie on the current page
**Test Expectation**: Unit test for parameter validation

### T7: Browser helpers for network

**Description**: Add `get_cookies()`, `set_cookie()` to `src/browser.rs`.
**Acceptance Criteria**:
- `get_cookies(client, tab_id, urls)` — calls Network.getCookies
- `set_cookie(client, tab_id, cookie_params)` — calls Network.setCookie
**Test Expectation**: Unit test for cookie response parsing

### T8: Phase 2 Tests

**Description**: Unit tests for network tools.
**Acceptance Criteria**:
- Test: cookie parsing from Network.getCookies CDP response
- Test: `set_cookie` parameter validation (missing name, missing value)
- `cargo test --locked` passes
**Test Expectation**: All new tests pass

### T9: Build & Integration Smoke

**Description**: Build release binary and run CLI smoke test against real Codex extension.
**Acceptance Criteria**:
- `cargo build --locked --release` succeeds
- CLI mode: `create` → `nav` → `execute_cdp` with `Runtime.evaluate` works
- CLI mode: `execute_cdp` with `Page.getResourceTree` works
- CLI mode: `execute_cdp` with `Network.getCookies` works
**Test Expectation**: Manual CLI smoke test passes

### T10: Multi-agent Code Review

**Description**: Launch parallel code-review agents for correctness, security, and simplification.
**Acceptance Criteria**:
- Correctness review: all CDP methods use proper params format
- Security review: no injection vectors in new tools
- Simplify review: no dead code, consistent patterns
**Test Expectation**: Review findings addressed

### T11: Governance Update

**Description**: Update AGENTS.md with new tool design rules.
**Acceptance Criteria**: AGENTS.md documents the MCP tool design pattern
