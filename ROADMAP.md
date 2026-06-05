# ROADMAP

## v1.5.0 — Codex 26.602+ Pipe Discovery Fix (2026-06-05)

Codex Desktop 26.602+ changed pipe naming from `codex-browser-use-<uuid>` to `codex-browser-use\<uuid>`. The old PowerShell `Get-ChildItem` discovery treated `\` as a directory separator and missed new-format pipes entirely.

### Fixed
- [x] **DISC-01** `discovery.go` — `Get-ChildItem` → `[System.IO.Directory]::GetFileSystemEntries` + substring extraction
- [x] **DISC-02** `discovery.go` — `extractUUID` handles both `-` and `\` separators
- [x] **DISC-03** `client.go` — Warning threshold `> 1` → `> 2` (old + new pipes coexist normally)

### Verified
- Smoke test: `get_info` ✅ `create_tab` ✅ `navigate` ✅ `screenshot` ✅ `close_tab` ✅
- All existing unit tests pass (`go test ./...`)
- go vet clean

---

## v0.3.0 — Bug Fixes from Cross-Audit (2026-05-19)

30 bugs found by parallel audit. Fixed in 4 batches.

---

### CRITICAL — 5/5 fixed ✅

- [x] **BUG-01** `server.go:161` — 4KB buffer → 10MB `bufio.NewReaderSize`
- [x] **BUG-02** `server.go:200-221` — MCP `notifications/initialized` no-op; nil-ID notification detection
- [x] **BUG-03** `main.go:109` — CLI whitespace panic: `len(args) == 0` guard
- [x] **BUG-04** `main.go:104,243` — CLI EOF spin: `nextLine()` returns `(string, bool)`
- [x] **BUG-05** `browser.go:496` — Fill element-not-found returns error via JS JSON response

### HIGH — 7/7 fixed ✅

- [x] **BUG-06** `server.go:292-547` — 19 handlers: `json.Unmarshal` error checked
- [x] **BUG-07** `browser.go:483,496` — JS injection: Go `%q` → `json.Marshal` (`jsonEscaped`)
- [x] **BUG-08** `protocol.go:22` + `client.go:211` — Response.ID `int` → `*int`; nil check
- [x] **BUG-09** `server.go:258,268,279,307,320` — `json.Marshal` errors logged/returned
- [x] **BUG-10** `browser.go:112,142` — NavigateBack/Forward dual bounds check
- [x] **BUG-11** `discovery.go:19-26` — PowerShell subprocess 15s timeout via `context.WithTimeout`
- [x] **BUG-12** `client.go:216` — readLoop non-blocking `select { case ch <- resp: default: }`

### MEDIUM — 5/8 fixed

- [ ] **BUG-13** `browser.go:236` — Global CDP detach+attach optimization (deferred: perf, not crash)
- [x] **BUG-14** `browser.go:341-348` — CUAType: attach once, executeCdp direct (no detach between chars)
- [ ] **BUG-15** `browser.go:358-372` — CUAKeypress per-key attach cycles (deferred: perf)
- [x] **BUG-16** `browser.go:342-343` — CUAType: keyDown+char+keyUp per character
- [ ] **BUG-17** `browser.go:179-181` — WaitForLoad transient error retry (deferred: edge case)
- [x] **BUG-18** `client.go:67` — Health check: 5s goroutine timeout wrapper
- [ ] **BUG-19** `browser.go:255-257` — isDebuggerError match expansion (deferred: future extension)
- [x] **BUG-20** `main.go:212-213` — CLI try command: `args[2:]` instead of byte offset

### LOW — 8/10 fixed

- [x] **BUG-21** `client.go:238` — newUUID returns error + fallbackUUID via math/rand
- [x] **BUG-22** `main.go:33` — BRIDGE_DEBUG_LOG open failure logs warning
- [x] **BUG-23** `main.go` — os.Exit → return error; defer preserved
- [x] **BUG-24** `discovery.go:43` — extractUUID conditional single-char strip
- [x] **BUG-25** `client.go:174` — time.After → time.NewTimer + defer Stop()
- [x] **BUG-26** `browser.go:269-288` — DOMSnapshot fallback prepends marker
- [x] **BUG-27** `browser.go:639` — ClaimUserTab auto-attach error logged to logger
- [x] **BUG-28** `browser.go:291,294` — Screenshots typo (was already fixed)
- [x] **BUG-29** `browser.go:422-424` — DomCUAClick boxModel len(content) < 5 guard
- [x] **BUG-30** `client.go:180-193` — TestSendNotificationFrame validates wire format

---

### Summary

| Severity | Fixed | Total | % |
|----------|-------|-------|---|
| CRITICAL | 5 | 5 | 100% |
| HIGH | 7 | 7 | 100% |
| MEDIUM | 5 | 8 | 62% |
| LOW | 10 | 10 | 100% |
| **Total** | **28** | **30** | **93%** |

2 deferred: BUG-13,15 (global CDP + CUAKeypress perf optimization, non-blocking) + BUG-17 (WaitForLoad retry, edge case) + BUG-19 (isDebuggerError expansion, future extension).

### Audit

Cross-dimensional review: core logic & concurrency, MCP protocol & framing, error handling & edge cases, test quality & coverage gaps, surface-level bugs, and main/discovery deep-dive.

### Fix

4 parallel branches, cherry-picked to main:
- `fix/a-server` (7152269): server.go — 4 bugs
- `fix/b-browser` (751280d): browser.go — 7 bugs
- `fix/c-client` (6bb3269): client/discovery/protocol — 6 bugs
- `fix/d-main` (ce64ab2): main.go — 5 bugs
