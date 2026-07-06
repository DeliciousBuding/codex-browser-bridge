# codex-browser-bridge Continuous Hardening — Progress Tracker

> **Task**: Stability, security, harness, CI/CD, release, npm, and multi-agent UX hardening.
> **Started**: 2026-07-06
> **Last Updated**: 2026-07-06
> **Mode**: LOCAL_ONLY
> **Repo**: DeliciousBuding/codex-browser-bridge

## References

- [Project Overview](../analysis/project-overview.md)
- [Module Inventory](../analysis/module-inventory.md)
- [Risk Assessment](../analysis/risk-assessment.md)
- [Task Breakdown](../plan/task-breakdown.md)
- [Dependency Graph](../plan/dependency-graph.md)
- [Milestones](../plan/milestones.md)

## Phase Summary

| Phase | Name | Tasks | Done | Progress |
|:--|:--|--:|--:|:--|
| 1 | Safety and Runtime Guardrails | 4 | 4 | Complete |
| 2 | Release, npm, and CI/CD Hygiene | 4 | 4 | Complete |
| 3 | Test Harness and E2E | 3 | 3 | Complete |
| 4 | Agent UX and Multi-Client Install | 3 | 3 | Complete |

## Phase Checklist

- [x] Phase 1: Safety and Runtime Guardrails (4/4 tasks) - [details](phase-1-safety-runtime-guardrails.md)
- [x] Phase 2: Release, npm, and CI/CD Hygiene (4/4 tasks) - [details](phase-2-release-npm-cicd.md)
- [x] Phase 3: Test Harness and E2E (3/3 tasks) - [details](phase-3-test-harness-e2e.md)
- [x] Phase 4: Agent UX and Multi-Client Install (3/3 tasks) - [details](phase-4-agent-ux-install.md)

## Current Status

**Active Phase**: PR CI hardening
**Active Task**: Review follow-ups in progress: raw CDP tightening, npm package staging, release version bump, and harness timeout/download bounds.
**Blockers**: Release requires npm Trusted Publisher configuration before the first OIDC publish. PR #15 CI must complete green before undrafting/merge.

## Governance Status

**Shared instruction surface**: `AGENTS.md` exists and was refreshed for this hardening pass.
**Claude Code instruction surface**: none.
**Other platform rule surfaces**: no dedicated Cursor/Windsurf/Cline/Codex rules found.
**Memory surface**: no repo-local fallback selected; do not create one without explicit user selection.
**Memory fallback path**: none.

## Adaptive Control State

```yaml
adaptive:
  strategy: "small hardening phases before larger harness and docs work"
  drift_score: 2
  thresholds:
    annotate: 3
    replan: 5
    rescope: 8
  total_tasks: 12
  completed_tasks: 12
  last_updated: "2026-07-06"
```

## Task Telemetry Log

| Date | Task | Actual Effort | S.U.P.E.R Score | Unplanned Dependencies | Notes |
|:--|:--|:--|:--|--:|:--|
| 2026-07-06 | T1.1-T1.4 | M | 4/5 | 0 | Guardrails implemented with focused helpers and tests. |
| 2026-07-06 | T2.1-T2.2 | S | 4/5 | 0 | npm skill package check fixed; Dependabot MSRV policy added. |
| 2026-07-06 | T1 review follow-ups | S | 4/5 | 2 | Blocked Page.navigateToHistoryEntry, bounded page asset content responses, rejected invalid UTF-8 and fractional integers. |
| 2026-07-06 | T2.3-T2.4 | M | 4/5 | 2 | Added release process docs, Unreleased changelog, least-privilege permissions, annotated tag gate, artifact attestations, and OIDC npm publishing. |
| 2026-07-06 | T4.1-T4.3 | M | 4/5 | 0 | README, zh-CN README, examples, skill, and AGENTS updated for config, profiles, upload base, and client installs. |
| 2026-07-06 | T3.1 partial | S | 4/5 | 1 | Added Linux-only MCP tools/call envelope tests; Windows tests pass, Linux target typechecked to linker, WSL run blocked by crates.io/cache timeout. |
| 2026-07-06 | T3.1-T3.3 completion | M | 4/5 | 1 | Added non-Windows mock pipe E2E, expanded Ubuntu CI harness, refactored npm installer for injectable tests, added live E2E script, and ran live E2E successfully against Codex Desktop + Chrome. |
| 2026-07-06 | Final local audit | S | 4/5 | 1 | `actionlint` via `go run`, Rust, npm, package checks, example JSON, PowerShell syntax, and live E2E all passed locally; GitHub CI remains external. |
| 2026-07-06 | PR #15 supply-chain fix | S | 5/5 | 1 | Updated `anyhow` from 1.0.102 to 1.0.103 to clear RustSec advisory; local `cargo test`, clippy, Linux test target check, npm tests, and `cargo deny check` passed. |
| 2026-07-06 | Subagent review follow-ups | M | 5/5 | 3 | Tightened raw CDP method blocks, validated cookie URLs, pre-budgeted page asset fetches, staged npm README/LICENSE/skills via prepack, bumped release target to 1.10.0, bounded npm downloads, and added MCP response timeout to live E2E. |

## Quick Status Commands

```bash
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cd npm && npm test
git status --short --branch
gh pr list -R DeliciousBuding/codex-browser-bridge --state open
gh issue list -R DeliciousBuding/codex-browser-bridge --state open
```

## Next Steps

1. Commit the hardening branch and open a PR when ready.
2. Watch GitHub CI, especially Ubuntu mock E2E and release workflow syntax.
3. Configure npm Trusted Publisher before the first tokenless release publish.

## Session Log

| Date | Session | Summary |
|:--|:--|:--|
| 2026-07-06 | current | Inspected repo, GitHub issues/PRs, Dependabot PR #14 failure, subagent audits, and implemented first hardening/release fixes. |
| 2026-07-06 | follow-up | Closed review findings in release automation, CDP safety, page asset resource bounds, MCP UTF-8 handling, and stricter MCP argument typing. |
| 2026-07-06 | phase-3 | Added mock and live E2E harnesses, npm installer injectable tests, and verified a live Codex Desktop + Chrome flow against example.com. |
| 2026-07-06 | final-local-audit | Re-ran full local verification, actionlint, npm package dry-runs, and live E2E; branch is locally PR-ready pending GitHub CI and npm Trusted Publisher setup. |
| 2026-07-06 | pr-ci-fix | Updated `anyhow` lockfile entry for RustSec advisory and verified the supply-chain gate locally before pushing. |
| 2026-07-06 | review-follow-ups | Addressed high/medium findings from security, CI/release, and stability review agents before final PR verification. |
