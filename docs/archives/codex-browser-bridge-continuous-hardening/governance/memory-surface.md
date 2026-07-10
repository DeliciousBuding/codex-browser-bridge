# Memory Surface

No repo-local fallback memory file was selected for this workflow.

Durable project rules were written to `AGENTS.md`, `docs/release-process.md`, README files, examples, and the project skill instead of creating a competing memory source.

Key durable facts:

- The bridge remains Windows-runtime-only; non-Windows CI uses mock pipe harnesses.
- Rust MSRV is 1.85 until intentionally raised.
- npm release publishing expects Trusted Publishing/OIDC rather than a long-lived `NPM_TOKEN`.
- `scripts/live-e2e.ps1` is the opt-in real Codex Desktop + Chrome smoke test.
