# Task Breakdown

## Overview

- **Total Phases**: 4
- **Total Tasks**: 12
- **Estimated Total Effort**: L
- **Tracking Mode**: GITHUB_STANDARD-capable, local docs used for this run to avoid creating issue noise before plan review.

## S.U.P.E.R Design Constraints

- **S**: Keep changes in the owning layer: MCP input validation in `src/mcp`, browser safety in `src/browser.rs`/`src/security.rs`, release automation in `.github`.
- **U**: Preserve MCP -> browser -> client -> protocol flow.
- **P**: Treat MCP schemas, npm package files, release tags, and changelog sections as contracts.
- **E**: Keep Windows-only runtime explicit; do not imply Linux npm install works.
- **R**: Prefer small helpers and tests over broad abstractions.

## Testing and Governance Constraints

- Behavior and contract changes require automated tests.
- CI/release changes require dry-run validation where possible.
- Docs-only work must name a validation command or manual review target.
- Durable future-agent rules go to `AGENTS.md` or project skill, not a new memory file unless explicitly selected.

## Phase 1: Safety and Runtime Guardrails

**Goal**: Close clear security/resource gaps without reshaping the architecture.

| ID | Task | Priority | Effort | Depends On | Lane | S.U.P.E.R | Test Expectation | Acceptance Criteria |
|:--|:--|:--|:--|:--|:--|:--|:--|:--|
| T1.1 | URL allowlist for navigation-like tools | P0 | S | None | A | P,E | Add URL regression tests | Only `http://` and `https://` pass `validate_url`; tests pass |
| T1.2 | Bound MCP stdio line size | P0 | S | None | A | P,E | Add stdio limit unit test | Oversized line is rejected before JSON parse; no unbounded buffer growth |
| T1.3 | Bound user duration/delay params | P0 | S | None | A | P,R | Add arg extractor and browser guard tests | `timeout_ms`, `duration_ms`, `delay_ms` have documented caps |
| T1.4 | Restrict raw CDP escape hatch | P0 | S | None | A | P,R | Add method validation tests | Dedicated sensitive methods are blocked; low-level inspect/input methods remain allowed |

## Phase 2: Release, npm, and CI/CD Hygiene

**Goal**: Make public release automation deterministic and aligned with npm/GitHub open-source practice.

| ID | Task | Priority | Effort | Depends On | Lane | S.U.P.E.R | Test Expectation | Acceptance Criteria |
|:--|:--|:--|:--|:--|:--|:--|:--|:--|
| T2.1 | Fix npm skill packaging order | P0 | S | None | B | P,E | `npm pack --dry-run` content check | CI and release assert `skills/codex-browser/SKILL.md` before publish |
| T2.2 | Encode MSRV policy for Dependabot | P1 | S | None | B | E,R | Config validation by review | Dependabot no longer proposes criterion 0.8 while MSRV is 1.85 |
| T2.3 | Add release/tag/changelog policy doc | P1 | M | T2.1 | B | P,E | Docs-only; validate with release workflow checks | Tag format, SemVer, Keep a Changelog, and npm provenance policy are documented |
| T2.4 | Harden release permissions and publish path | P1 | M | T2.1 | B | E,R | Workflow syntax and dry-run checks | Jobs use least required permissions; npm trusted publishing path is documented or implemented |

## Phase 3: Test Harness and E2E

**Goal**: Add practical regression coverage for the runtime paths most likely to break.

| ID | Task | Priority | Effort | Depends On | Lane | S.U.P.E.R | Test Expectation | Acceptance Criteria |
|:--|:--|:--|:--|:--|:--|:--|:--|:--|
| T3.1 | Add handler-level MCP tests | P1 | M | T1 | C | P,R | Automated tests | Representative tools validate args and error envelopes without real Chrome |
| T3.2 | Add mock pipe E2E harness | P1 | L | T1 | C | P,R | Automated integration tests | `getInfo`, navigate/evaluate/screenshot/finalize can run against scripted mock server |
| T3.3 | Add optional live E2E script | P2 | M | T3.2 | C | E,R | Opt-in script; no default CI dependency on Codex Desktop | Local command verifies real Codex Desktop + Chrome path when available |

## Phase 4: Agent UX and Multi-Client Install

**Goal**: Make Claude Code, Cursor, OpenClaw, Hermes Agent, npm, WSL, profiles, and upload config unambiguous.

| ID | Task | Priority | Effort | Depends On | Lane | S.U.P.E.R | Test Expectation | Acceptance Criteria |
|:--|:--|:--|:--|:--|:--|:--|:--|:--|
| T4.1 | Refresh README/README.zh-CN config docs | P1 | M | T2 | D | P,E | Docs-only; link/command review | Config precedence, `upload_base`, WSL limits, profiles, resources/prompts are documented |
| T4.2 | Client-specific examples | P1 | M | T4.1 | D | E,R | JSON syntax validation | Claude Code, Cursor, OpenClaw, Hermes examples include paths, env, approval/restart notes |
| T4.3 | Update agent skill and AGENTS.md | P1 | S | T4.1 | D | P,E | Docs-only; consistency grep | Tool count, `codex_doctor` profile behavior, upload base, and numeric tab ids are consistent |

## Parallel Lanes

| Lane | Tasks | Merge Risk | Key Files |
|:--|:--|:--|:--|
| A | T1.1-T1.4 | Medium | `src/security.rs`, `src/browser.rs`, `src/mcp/*`, tests |
| B | T2.1-T2.4 | Low | `.github/*`, `.github/dependabot.yml`, release docs |
| C | T3.1-T3.3 | Medium | `tests/*`, `src/client.rs`, `src/mcp/*` |
| D | T4.1-T4.3 | Medium | `README*`, `examples/*`, `skills/*`, `AGENTS.md` |
