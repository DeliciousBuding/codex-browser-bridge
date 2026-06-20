# Risk Assessment: codex-browser-bridge MCP Extension

## Task Scope Risk

| Risk | Severity | Likelihood | Impact | Mitigation |
|------|----------|------------|--------|------------|
| Extension protocol changes | Low | Low | New tools stop working | All new tools use `executeCdp` (generic, stable) |
| CDP method not supported by extension | Medium | Low | Specific CDP call fails | Probe CDP methods before committing to tools |
| Breaking existing tools | Low | Very Low | Regression | All additions are additive; existing code untouched |
| `pageAssets` capability format unknown | Medium | Medium | Can't design tool schema | Probe via CDP first; fallback to generic `execute_cdp_raw` |
| Git conflicts (Go ↔ Rust) | Low | Low | Merge issues | Only modify `src/` (Rust); Go code is legacy |

## Project Health

| Dimension | Rating | Notes |
|-----------|--------|-------|
| Test Coverage | Medium | Unit tests for protocol, parsing, URL validation. No integration tests for CDP. |
| Error Handling | Good | Structured `BridgeError` with user/protocol/rpc variants. Tab-gone detection. |
| Code Quality | Good | Clean Rust idiomatic code, well-documented functions. |
| Documentation | Good | README, CHANGELOG, ROADMAP, docs/rust-rewrite/. |
| CI/CD | Good | GitHub Actions: test, lint, release with x64/arm64. |

## Testing Gaps

1. **No MCP integration tests**: Individual tools tested indirectly through handler tests, not full stdio lifecycle
2. **No CDP integration tests**: All CDP calls go through real Codex extension — no mock for integration testing
3. **New tool testing strategy**: Add unit tests for handler dispatch + schema validation; manual smoke test for real CDP calls

## Governance

| Surface | Path | Status |
|---------|------|--------|
| Shared rules | `AGENTS.md` | Exists (minimal: security + .env rules) |
| Claude Code rules | None | Not yet created |
| Memory | None | Not yet configured |

## S.U.P.E.R Architecture Health Summary

| Principle | Project Score | Transformation Impact |
|-----------|:---:|------|
| **S**ingle Purpose | ✅ 5/5 | Well-modularized; adding tools follows existing pattern |
| **U**nidirectional Flow | ✅ 5/5 | MCP → browser → client → pipe (clean layers) |
| **P**orts over Implementation | ✅ 5/5 | `executeCdp` is the universal CDP port — all new tools use it |
| **E**nvironment-Agnostic | ⚠️ 3/5 | Windows-only; no change planned (user on Windows) |
| **R**eplaceable Parts | ✅ 5/5 | Modules independently testable |

### Violation Hotspots (Priority for This Transformation)

1. ⚠️ **`pageAssets` gap (P)**: Extension's declared capability lacks a bridge port. Add `codex_page_assets` tool using CDP `Page.getResourceTree` + `Page.getResourceContent`.
2. ⚠️ **No generic CDP tool (P)**: 24 specialized tools but no universal CDP escape hatch. Add `codex_execute_cdp` for arbitrary CDP commands.
3. ⚠️ **No Network domain exposure (P)**: Cookies, request monitoring unavailable. Add `codex_network_*` tools if CDP probe confirms support.
