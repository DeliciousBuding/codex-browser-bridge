# ROADMAP

## v1.5.0: Codex 26.602+ pipe discovery fix (2026-06-05)

Codex Desktop 26.602+ changed pipe naming from `codex-browser-use-<uuid>` to `codex-browser-use\<uuid>`. The old PowerShell `Get-ChildItem` discovery treated `\` as a directory separator and missed new-format pipes entirely.

### Fixed
- [x] **DISC-01** `discovery.go`: replaced `Get-ChildItem` with `[System.IO.Directory]::GetFileSystemEntries` plus substring extraction
- [x] **DISC-02** `discovery.go`: `extractUUID` handles both `-` and `\` separators
- [x] **DISC-03** `client.go`: warning threshold changed from `> 1` to `> 2` because old and new pipes can coexist

### Verified
- Smoke test: `get_info`, `create_tab`, `navigate`, `screenshot`, `close_tab`
- All existing unit tests pass (`go test ./...`)
- go vet clean

---

## v0.3.0: bug fixes from cross-audit (2026-05-19)

The cross-audit tracked 30 bugs. Fixes landed in 4 batches.

---

### Critical: 5/5 fixed

- [x] **BUG-01** `server.go:161`: 4KB buffer changed to 10MB `bufio.NewReaderSize`
- [x] **BUG-02** `server.go:200-221`: MCP `notifications/initialized` no-op and nil-ID notification detection
- [x] **BUG-03** `main.go:109`: CLI whitespace panic fixed with `len(args) == 0` guard
- [x] **BUG-04** `main.go:104,243`: CLI EOF spin fixed by making `nextLine()` return `(string, bool)`
- [x] **BUG-05** `browser.go:496`: Fill element-not-found now returns an error through the JS JSON response

### High: 7/7 fixed

- [x] **BUG-06** `server.go:292-547`: 19 handlers now check `json.Unmarshal` errors
- [x] **BUG-07** `browser.go:483,496`: JS injection path now uses `json.Marshal` through `jsonEscaped` instead of Go `%q`
- [x] **BUG-08** `protocol.go:22` + `client.go:211`: `Response.ID` changed from `int` to `*int` with nil checks
- [x] **BUG-09** `server.go:258,268,279,307,320`: `json.Marshal` errors are logged or returned
- [x] **BUG-10** `browser.go:112,142`: NavigateBack/Forward now use dual bounds checks
- [x] **BUG-11** `discovery.go:19-26`: PowerShell subprocess now has a 15s timeout via `context.WithTimeout`
- [x] **BUG-12** `client.go:216`: `readLoop` send is non-blocking with `select { case ch <- resp: default: }`

### Medium: 4/8 fixed

- [ ] **BUG-13** `browser.go:236`: global CDP detach+attach optimization (deferred: performance, not crash)
- [x] **BUG-14** `browser.go:341-348`: CUAType attaches once and calls executeCdp directly, with no detach between chars
- [ ] **BUG-15** `browser.go:358-372`: CUAKeypress still has per-key attach cycles (deferred: performance)
- [x] **BUG-16** `browser.go:342-343`: CUAType sends keyDown, char, and keyUp per character
- [ ] **BUG-17** `browser.go:179-181`: WaitForLoad transient error retry (deferred: edge case)
- [x] **BUG-18** `client.go:67`: health check uses a 5s goroutine timeout wrapper
- [ ] **BUG-19** `browser.go:255-257`: isDebuggerError match expansion (deferred: future extension)
- [x] **BUG-20** `main.go:212-213`: CLI try command uses `args[2:]` instead of byte offset

### Low: 10/10 fixed

- [x] **BUG-21** `client.go:238`: newUUID returns error, with fallbackUUID via math/rand
- [x] **BUG-22** `main.go:33`: BRIDGE_DEBUG_LOG open failure logs a warning
- [x] **BUG-23** `main.go`: os.Exit replaced by returned errors so defers are preserved
- [x] **BUG-24** `discovery.go:43`: extractUUID uses conditional single-char strip
- [x] **BUG-25** `client.go:174`: time.After replaced with time.NewTimer and defer Stop()
- [x] **BUG-26** `browser.go:269-288`: DOMSnapshot fallback prepends marker
- [x] **BUG-27** `browser.go:639`: ClaimUserTab auto-attach error is logged to logger
- [x] **BUG-28** `browser.go:291,294`: screenshots typo was already fixed
- [x] **BUG-29** `browser.go:422-424`: DomCUAClick checks boxModel len(content) < 5
- [x] **BUG-30** `client.go:180-193`: TestSendNotificationFrame validates wire format

---

### Summary

| Severity | Fixed | Total | % |
|----------|-------|-------|---|
| CRITICAL | 5 | 5 | 100% |
| HIGH | 7 | 7 | 100% |
| MEDIUM | 4 | 8 | 50% |
| LOW | 10 | 10 | 100% |
| **Total** | **26** | **30** | **87%** |

Deferred: BUG-13 and BUG-15 for CDP attach performance, BUG-17 for WaitForLoad transient retry, and BUG-19 for debugger error matching.

### Audit

Cross-audit areas: core logic and concurrency, MCP protocol and framing, error handling and edge cases, test coverage, surface-level bugs, and main/discovery behavior.

### Fix

4 parallel branches, cherry-picked to main:
- `fix/a-server` (7152269): server.go, 4 bugs
- `fix/b-browser` (751280d): browser.go, 7 bugs
- `fix/c-client` (6bb3269): client/discovery/protocol, 6 bugs
- `fix/d-main` (ce64ab2): main.go, 5 bugs
