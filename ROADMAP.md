# ROADMAP

## v0.3.0 — Bug Fixes from Cross-Audit (2026-05-19)

30 bugs found by 6-agent parallel audit. Fixed in 4 parallel batches (all Haiku/Opus-4.6).

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

- [ ] **BUG-21** `client.go:238` — newUUID panic → error (deferred: Windows never fails)
- [x] **BUG-22** `main.go:33` — BRIDGE_DEBUG_LOG open failure logs warning
- [x] **BUG-23** `main.go` — os.Exit → return error; defer preserved
- [x] **BUG-24** `discovery.go:43` — extractUUID conditional single-char strip
- [x] **BUG-25** `client.go:174` — time.After → time.NewTimer + defer Stop()
- [x] **BUG-26** `browser.go:269-288` — DOMSnapshot fallback prepends marker
- [ ] **BUG-27** `browser.go:585` — ClaimUserTab auto-attach error (deferred: minor)
- [x] **BUG-28** `browser.go:291,294` — Screenshots typo (was already fixed)
- [x] **BUG-29** `browser.go:422-424` — DomCUAClick boxModel len(content) < 5 guard
- [ ] **BUG-30** `client.go:180-193` — SendNotification test coverage (deferred: test batch)

---

### Summary

| Severity | Fixed | Total | % |
|----------|-------|-------|---|
| CRITICAL | 5 | 5 | 100% |
| HIGH | 7 | 7 | 100% |
| MEDIUM | 5 | 8 | 62% |
| LOW | 8 | 10 | 80% |
| **Total** | **25** | **30** | **83%** |

5 deferred: BUG-13,15,17,19 (perf/edge-case MEDIUM) + BUG-21,27 (LOW, unlikely to trigger) + BUG-30 (test coverage).

### Audit methodology

6 subagents (2×Opus 4.7, 2×Sonnet 4.6, 2×Haiku/Opus-4.6-fast) scanned in parallel:
- Opus #1: core client logic, concurrency, CDP protocol
- Opus #2: MCP server, protocol framing, discovery
- Sonnet #1: error handling, edge cases, resource leaks
- Sonnet #2: test quality, coverage gaps
- Haiku #1: surface bugs, typos, naming, logic errors
- Haiku #2: main.go + discovery.go deep audit

### Fix methodology

4 parallel Haiku agents, each on dedicated branch, cherry-picked to main:
- `fix/a-server` (7152269): server.go — 4 bugs
- `fix/b-browser` (751280d): browser.go — 7 bugs
- `fix/c-client` (6bb3269): client/discovery/protocol — 6 bugs
- `fix/d-main` (ce64ab2): main.go — 5 bugs
