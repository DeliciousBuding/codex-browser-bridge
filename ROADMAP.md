# ROADMAP

## Status: v1.9.1 shipped (2026-06-21)

52 MCP tools, CDP event architecture, structured network monitoring, `--mode doctor` CLI, JPEG/WebP screenshots. See [CHANGELOG.md](CHANGELOG.md) for the full v1.7.0 → v1.9.1 arc.

**Architecture health (SUPER):** S 5, U 5, P 5, E 3 (Windows-only), R 4 = **22/25**. The remaining gaps are operational maturity, not architecture — that is what v1.10.0 targets.

### Completed releases

- **v1.9.1** (2026-06-21): 16 new tools → 52 total. CDP event subscription (`network_monitor`, `console_logs`). Background-tab fix (`bring_to_front` + sticky 20s timeout). JPEG/WebP screenshots, sessionStorage, `--mode doctor`, `performance_metrics`.
- **v1.9.0** (2026-06-20): `src/mcp/` module split, 8 tools (file upload, dialog, locator layer, composite tools, profiles, doctor).
- **v1.8.0** (2026-06-20): CDP error normalization, sticky attach, Go legacy purged, protocol/MCP stdio optimization.
- **v1.7.x** (2026-06-20): CDP MCP tools, security hardening, Codecov, repo SEO.

---

## v1.10.0 — Engineering hardening (current)

The tool layer is saturated. The honest gaps are runtime robustness, supply chain, performance baselines, and test coverage — not more tools.

### P0 — Runtime robustness & supply chain

- [ ] **Pipe auto-reconnect.** Today the read loop dying (Codex restart, extension refresh, pipe hiccup) bricks the client and forces an MCP server restart. Add supervised reconnect: detect read-loop termination, re-discover + re-dial the pipe, re-establish session, drain pending requests with a clear error. Long-running agents hit this regularly.
  - Effort: M · files: `src/client.rs`
- [ ] **Supply-chain CI** (`cargo-deny` + `cargo-audit`). Standard for public Rust projects; currently absent. `deny.toml` for license/advisory/ban checks, a weekly audit job, and dependabot for `Cargo.toml` (today only `github-actions` is covered).
  - Effort: S · files: `.github/workflows/`, `deny.toml`

### P1 — Performance & test baselines

- [ ] **criterion benchmarks.** The `encode_frame` single-syscall and sticky-attach optimizations have no baseline. Add `benches/protocol.rs` (frame encode/decode throughput) + `benches/attach.rs` (sticky vs full-attach latency). Proves the wins and guards against regression.
  - Effort: S · files: `benches/`, `Cargo.toml` dev-dep
- [ ] **Restore e2e coverage the right way.** The deleted `#[path]`-include tests were an anti-pattern (they embed the source tree and break under modularization — that is what killed the v1.8/v1.9 releases). Rewrite key flows (network_monitor event pairing, CDP error propagation, storage round-trip) as integration tests using the **public crate API** + tokio mock, not source-tree embedding.
  - Effort: M · files: `tests/`

### P2 — Distribution & protocol depth

- [ ] **winget + scoop manifests.** `winget install codex-browser-bridge` is more native than npm for Windows users. Discovery lift, no code.
  - Effort: S
- [ ] **MCP resources/prompts.** Expose cookies/localStorage as subscribable resources (agents read state without a tool call per access); ship prompt templates for login/table-extraction flows.
  - Effort: M
- [ ] **Config file** (`.codex-browser-bridge.toml`) for profile + upload_base, replacing env-only.
  - Effort: S
- [ ] **ARCHITECTURE.md** — design-decision record for contributors.
  - Effort: S

---

## v2.0.0 — Cross-platform

- [ ] Unix domain socket transport (macOS/Linux). Codex Desktop on Darwin uses a different IPC mechanism than Windows named pipes — needs research on the actual socket naming.
- [ ] Non-Windows CI matrix (ubuntu-latest, macos-latest).
- [ ] npm `os` field expanded to `darwin`, `linux`.
- [ ] WSL: auto-select the Windows named pipe from a WSL guest.
- Effort: L

---

## Backlog

Feature-sized items not yet scoped into a release:

- [ ] `codex_network_monitor` URL filter (reduce noise; the paired list is usable today)
- [ ] `codex_performance_trace` — `Performance.enable` + trace export (the metrics tool already shipped)
- [ ] `codex_execute_cdp` user-customizable allowlist (config/env)
- [ ] Typed tool result schemas (structured agent consumption)
- [ ] Screenshot clip/scale params (format + quality already shipped)
- [ ] OpenGraph social share image
- [ ] macOS socket-naming research + real Darwin test
