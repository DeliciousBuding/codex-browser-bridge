# Risk Assessment

## S.U.P.E.R Architecture Health Summary

| Principle | Status | Key Findings | Priority |
|:--|:--|:--|:--|
| S Single Purpose | Yellow | Core split is good, but `browser.rs`, `client.rs`, and handler dispatch are high-churn files. | Medium |
| U Unidirectional Flow | Green | Flow is MCP -> browser -> client -> protocol/pipe; no circular module dependency found. | Low |
| P Ports over Implementation | Green | MCP schemas, handler validation, npm package checks, and release tags are now covered by explicit contracts/tests. | Medium |
| E Environment-Agnostic | Yellow | Binary remains Windows-only by design; docs now state npm/WSL/config boundaries and non-Windows CI runs mock harnesses. | Medium |
| R Replaceable Parts | Green | Protocol/config surfaces and mock pipe harnesses now verify replaceable transport/client behavior without Codex Desktop. | Medium |

**Overall Health**: 3/5 fully green, 2/5 partial. Remaining work is targeted polish and CI/runtime verification, not a rewrite.

## Risk Matrix

| Risk | Impact | Likelihood | Severity | Mitigation |
|:--|:--|:--|:--|:--|
| Raw CDP bypasses safer wrappers | Agent can navigate unsafe schemes or read sensitive cookies | Low | Medium | Positive allowlist, method-specific blocks, and regression tests |
| MCP stdio unbounded line input | Local MCP client can grow memory unbounded | Low | Medium | Max line length and invalid UTF-8 parse-error handling in stdio loop |
| User duration parameters unbounded | Long sleeps, overflow, stuck event subscriptions | Low | Medium | Handler and browser-layer duration caps plus strict integer validation |
| Client read-loop and event subscriptions retain resources | Long-running process leaks handles/tasks | Medium | High | Close path, prune closed subscriptions, cancellation guard |
| Release package checks miss bundled skill | npm publish can fail or publish incomplete package | Low | Medium | Bundle before pack, assert `skills/codex-browser/SKILL.md`, and require checksums after release assets exist |
| Dependabot bumps dev deps beyond MSRV | CI red PRs and noisy dependency queue | Medium | Medium | Dependabot ignores `criterion` semver-minor updates while MSRV is Rust 1.85 |
| Docs/examples overpromise client support | Claude/OpenClaw/Hermes/Cursor setup friction | Low | Medium | Client-specific examples and config docs are updated and JSON-validated |
| True E2E missing | Regressions in pipe/CDP behavior can escape | Low | Medium | Non-Windows mock E2E, handler envelope tests, npm installer harness, and optional live E2E script |

## High-Severity Risks

- **Security boundary clarity**: `codex_execute_cdp` now uses an allowlist plus method-specific blocks; future CDP expansion should keep dedicated safe wrappers preferred.
- **Runtime resource management**: Event subscriptions and `page_assets include_content` now have bounded behavior; future long-running monitors should preserve cancellation/size caps.
- **Release correctness**: npm package content, checksum embedding, tag/version/changelog validation, least-privilege permissions, and artifact attestations are enforced before publishing.

## Testing Risks

- Non-Windows mock E2E and MCP envelope tests run in Ubuntu CI; local Windows runs typecheck them via `cargo check --tests --target x86_64-unknown-linux-gnu`.
- Optional live E2E was verified against a real Codex Desktop + Chrome session with `scripts/live-e2e.ps1`.
- npm installer tests now cover injected install flows, checksum mismatch handling, dev download routing, and wrapper missing-binary behavior.
- Remaining CI-only risk: GitHub-hosted Ubuntu must execute the non-Windows mock E2E after this branch is pushed.

## Project Governance Risks

- `AGENTS.md` was refreshed with current tool counts, release rules, and verification commands.
- No resolved durable memory surface exists in the repo.
- Project skill is a key part of agent UX, but CI previously did not assert it is included in npm artifacts.

## Compatibility Concerns

- Tightening URL validation to `http`/`https` may reject unusual schemes previously accepted by accident.
- Tightening raw CDP may require advanced users to use purpose-built safe tools or request allowlist expansion.
- Raising MSRV for criterion would affect CI and users; current plan preserves Rust 1.85.
