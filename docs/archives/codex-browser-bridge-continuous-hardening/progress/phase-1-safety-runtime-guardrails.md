# Phase 1: Safety and Runtime Guardrails

- [x] T1.1 URL allowlist for navigation-like tools
  - Acceptance: only `http://` and `https://` pass `validate_url`; regression tests pass.
- [x] T1.2 Bound MCP stdio line size
  - Acceptance: oversized lines are rejected before JSON parsing; invalid UTF-8 returns a JSON-RPC parse error.
- [x] T1.3 Bound user duration/delay params
  - Acceptance: waits, captures, form delays, and integer-like MCP args reject unbounded or fractional values.
- [x] T1.4 Restrict raw CDP escape hatch
  - Acceptance: sensitive methods including `Page.navigate` and `Page.navigateToHistoryEntry` are blocked; read/input CDP methods remain allowed.

## Notes

Verified with `cargo fmt --check`, `cargo test --locked`, and `cargo clippy --locked --all-targets -- -D warnings` on 2026-07-06.
