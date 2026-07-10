# Phase 3: Test Harness and E2E

- [x] T3.1 Add handler-level MCP tests
  - Acceptance: JSON-RPC `tools/call` envelope tests cover invalid params, malformed arguments, unknown tools, profile-filtered tools, and handler validation errors.
  - Status: implemented in `src/mcp/mod.rs`; non-Windows target typechecks locally and will execute in Ubuntu CI.
- [x] T3.2 Add mock pipe E2E harness
  - Acceptance: fake Codex pipe server covers `getInfo`, title/screenshot, CDP allow/deny behavior, URL blocking without pipe traffic, and sticky attach sequencing.
  - Status: implemented in `tests/cdp_tools_e2e.rs`; local Windows run skips by cfg, non-Windows target typechecks locally and Ubuntu CI now runs full `cargo test --locked`.
- [x] T3.3 Add optional live E2E script
  - Acceptance: opt-in command verifies a real Codex Desktop + Chrome session without becoming a default CI dependency.
  - Status: implemented in `scripts/live-e2e.ps1` and run successfully on 2026-07-06 against `https://example.com`.

## Notes

Phase 3 verification evidence:

- `cargo test --locked` passes on Windows; non-Windows-only E2E tests are skipped locally by cfg.
- `cargo check --locked --tests --target x86_64-unknown-linux-gnu` typechecks the non-Windows mock E2E and MCP envelope tests locally.
- `npm test` covers installer checksum parsing, injected install flows, checksum mismatch handling, and wrapper missing-binary behavior.
- `scripts/live-e2e.ps1 -BridgePath .\target\debug\codex-browser-bridge.exe -Url https://example.com -TimeoutMs 15000` passed with title `Example Domain` and screenshot content.
