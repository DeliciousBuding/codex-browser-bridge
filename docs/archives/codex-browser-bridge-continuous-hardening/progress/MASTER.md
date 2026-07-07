# codex-browser-bridge Continuous Hardening — Progress Tracker

> **Task**: Stability, security, harness, CI/CD, release, npm, and multi-agent UX hardening.
> **Started**: 2026-07-06
> **Last Updated**: 2026-07-07
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

**Active Phase**: PR #15 finalization
**Active Task**: PR #15 final review follow-ups: bounded monitor event capture for network/console tools; next queued stability review is lazy/offline MCP startup and verified reconnect selection.
**Blockers**: Release requires npm Trusted Publisher configuration before the first OIDC publish. PR #15 remains draft until final review/undraft decision.

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
| 2026-07-07 | MCP response size caps | M | 5/5 | 1 | Added central `Content` text/image output bounds, covered `resources/read`, documented env knobs, and verified Rust/npm/actionlint/live E2E locally. |
| 2026-07-07 | Output cap multi-config | S | 5/5 | 0 | Added `max_text_bytes` and `max_image_bytes` to TOML config and CLI flags, preserving CLI > config > env > default precedence. |
| 2026-07-07 | Bridge runtime metadata | S | 5/5 | 1 | Extended `codex_get_info` with additive bridge runtime metadata while preserving top-level extension fields and avoiding raw upload path leakage. |
| 2026-07-07 | Trusted Publishing toolchain gate | S | 5/5 | 0 | Added an explicit npm >= 11.5.1 publish-job check and documented the npm CLI prerequisite for OIDC Trusted Publishing. |
| 2026-07-07 | Review follow-ups for CDP, assets, upload, prerelease publishing | M | 5/5 | 3 | Replaced broad raw CDP prefixes with an explicit method allowlist, required explicit upload base for file input, bounded page asset content fetches by known size and timeout, preserved extension-owned `bridge` metadata, and routed npm prereleases to `next`. |
| 2026-07-07 | Release Ubuntu harness gate | S | 5/5 | 0 | Added the non-Windows mock harness test and clippy job to the release workflow before asset creation. |
| 2026-07-07 | Multi-client config and resource cleanup follow-ups | M | 5/5 | 4 | Made explicit config paths authoritative, added CDP to the network profile, rejected relative upload paths, moved timeout cleanup into the Client path for page asset fetches, normalized asset content to base64, removed stateful/raw Runtime methods from generic CDP, and moved ship-bound changelog entries into 1.10.0. |
| 2026-07-07 | Release atomicity hardening | S | 5/5 | 1 | Stage GitHub Release assets as a draft, pass checksums to npm via workflow artifact, then publish the GitHub Release only after npm succeeds. |
| 2026-07-07 | CI concurrency hardening | S | 5/5 | 0 | Cancel stale PR CI/supply-chain runs and serialize same-tag release workflow runs without cancelling active releases. |
| 2026-07-07 | Client resource cleanup | S | 5/5 | 1 | Retire idle per-tab locks on tab close/finalize and treat invalid page asset sizes as unknown, with tests for lock retention and resource-size normalization. |
| 2026-07-07 | Live E2E timeout cleanup | S | 5/5 | 0 | Killed bridge processes now skip close/finalize cleanup and avoid blocking stderr reads on timeout. |
| 2026-07-07 | Live E2E timeout gate | S | 5/5 | 0 | Added a fake bridge timeout harness script and wired it into Windows CI/release test jobs. |
| 2026-07-07 | Release recoverability hardening | S | 5/5 | 2 | Made release jobs share an exact tag commit SHA, required non-empty dated changelog notes, made GitHub Release asset staging rerunnable, published assets before npm visibility, and pinned release actions to SHAs. |
| 2026-07-07 | Supply-chain advisory refresh | S | 5/5 | 1 | Updated `crossbeam-epoch` from 0.9.18 to 0.9.20 in `Cargo.lock` to clear `RUSTSEC-2026-0204` from the dev benchmark dependency chain. |
| 2026-07-07 | Workflow action pin enforcement | S | 5/5 | 0 | Pinned CI and supply-chain workflow actions to full commit SHAs and added a Node scanner enforced in CI. |
| 2026-07-07 | npm multi-client examples | S | 5/5 | 0 | Bundled `examples/` into the npm package and expanded postinstall hints for Claude Code, Cursor, OpenClaw, Hermes Agent, and skill-aware agents. |
| 2026-07-07 | Agent surface and URL/release follow-ups | M | 5/5 | 2 | Added CI/release agent-surface drift checks, normalized URL validation before CDP/cookie use, made Cursor/Hermes examples expose the full 52-tool profile, strengthened release changelog/action-pin checks, and added npm publish rerun idempotence. |
| 2026-07-07 | Resource-use follow-ups | M | 5/5 | 1 | Retired idle tab locks on attach failure and changed `codex_print_pdf` to bounded `ReturnAsStream`/`IO.read` processing with `IO.close`, plus mock tests for the stream sequence and PDF size budget. |
| 2026-07-07 | Live E2E stderr drain | S | 5/5 | 0 | Drained redirected bridge stderr asynchronously and strengthened the fake live-E2E timeout harness so stderr flood must not block initialize/create-tab before the expected tool-call timeout. |
| 2026-07-07 | npm tarball script hygiene | S | 5/5 | 0 | Made `prepack` stage a publish-only package manifest containing only `postinstall`, restored the dev manifest in `postpack`, and made package checks inspect the real tarball manifest. |
| 2026-07-07 | npm tarball manifest portability | S | 5/5 | 1 | Made packed manifest extraction use a tarball-relative path for Windows Git Bash portability and added fail-fast handling plus tests for stale publish manifest backups. |
| 2026-07-07 | Release contract and agent UX follow-ups | M | 5/5 | 4 | Added executable release contract checks for Trusted Publishing, main-branch manual dispatch, same-version checksum drift, npm 404 handling, and CI/release timeouts; clarified Windows skill install, GUI client PATH, upload-base, and zh-CN resource/prompt docs. |
| 2026-07-07 | Harness bounded-wait follow-ups | M | 5/5 | 2 | Bounded mock pipe reads and spawned task joins, removed global env mutation from runtime-info tests, bounded live E2E doctor/cleanup calls, and marked postfetch page-asset total-limit truncation as failed. |
| 2026-07-07 | Client/release final review follow-ups | M | 5/5 | 5 | Replaced remaining client reconnect test sleeps with bounded polling, added connection epochs so stale read loops cannot tear down fresh reconnects, pruned closed event subscribers, closed PDF streams on IO.read/parse failure, stopped sticky CDP from retrying non-session errors, moved release tag input validation before checkout/repo scripts, and enforced supply-chain job timeouts. |
| 2026-07-07 | Deadline and cookie validation follow-ups | S | 5/5 | 2 | Rejected expired CDP deadlines before writing side-effecting frames and validated cookie name/value/domain/path/sameSite before set/delete cookie tools touch the browser pipe. |
| 2026-07-07 | Monitor event capture byte budget | S | 5/5 | 2 | Added a shared 256 KiB drain budget for network/console monitor events, preserved raw observed counts after truncation, filtered network capture to pairable request/response events, and recorded follow-up findings for subscription-time byte bounds plus lazy MCP startup. |

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

1. Push the latest monitor event capture byte-budget follow-up.
2. Wait for PR #15 checks to return green again.
3. Consider the next minimal stability slice from subagent review: lazy/offline MCP startup, verified reconnect selection, and absolute MCP install hints for GUI/scheduled agents.
4. Decide whether to undraft and merge PR #15.
5. Configure npm Trusted Publisher before the first tokenless release publish.
6. After PR #15 lands, revisit Dependabot PR #14 against the updated MSRV/release baseline.

## Session Log

| Date | Session | Summary |
|:--|:--|:--|
| 2026-07-06 | current | Inspected repo, GitHub issues/PRs, Dependabot PR #14 failure, subagent audits, and implemented first hardening/release fixes. |
| 2026-07-06 | follow-up | Closed review findings in release automation, CDP safety, page asset resource bounds, MCP UTF-8 handling, and stricter MCP argument typing. |
| 2026-07-06 | phase-3 | Added mock and live E2E harnesses, npm installer injectable tests, and verified a live Codex Desktop + Chrome flow against example.com. |
| 2026-07-06 | final-local-audit | Re-ran full local verification, actionlint, npm package dry-runs, and live E2E; branch is locally PR-ready pending GitHub CI and npm Trusted Publisher setup. |
| 2026-07-06 | pr-ci-fix | Updated `anyhow` lockfile entry for RustSec advisory and verified the supply-chain gate locally before pushing. |
| 2026-07-06 | review-follow-ups | Addressed high/medium findings from security, CI/release, and stability review agents before final PR verification. |
| 2026-07-07 | response-caps | Closed remaining large MCP response risk by enforcing bounded text/image content at the shared response layer. |
| 2026-07-07 | output-cap-config | Made response cap settings available through config file and CLI, not only environment variables. |
| 2026-07-07 | runtime-info | Added agent-facing runtime diagnostics for profile, tool count, response caps, and upload-base configured status. |
| 2026-07-07 | release-toolchain | Added a publish-job npm CLI version gate for npm Trusted Publishing compatibility. |
| 2026-07-07 | review-hardening | Addressed subagent findings for raw CDP scope, page asset fetch budgets, file upload opt-in, metadata field compatibility, and npm prerelease dist-tags. |
| 2026-07-07 | release-harness-gate | Added Ubuntu mock harness checks to the release workflow dependency chain. |
| 2026-07-07 | config-resource-followups | Addressed subagent findings for CODEX_BRIDGE_CONFIG fallback, network profile CDP visibility, relative upload paths, Client pending cleanup on timeouts, base64 page asset content, raw CDP stateful methods, and 1.10.0 changelog hygiene. |
| 2026-07-07 | release-atomicity | Added draft GitHub Release staging and checksum artifact handoff so npm publish must succeed before release assets become public. |
| 2026-07-07 | workflow-concurrency | Added PR-run cancellation for CI/supply-chain and same-tag serialization for release workflow runs. |
| 2026-07-07 | client-resource-cleanup | Added close/finalize cleanup for stale per-tab locks and page asset size normalization to reduce long-lived agent session resource growth. |
| 2026-07-07 | live-e2e-timeout-cleanup | Hardened live E2E timeout cleanup so killed bridge processes do not cause secondary hangs. |
| 2026-07-07 | live-e2e-timeout-gate | Added a CI/release fake bridge timeout harness for live E2E cleanup regressions. |
| 2026-07-07 | release-recoverability | Hardened release reruns, exact tag-SHA checkout, changelog-note validation, npm installability ordering, and release-workflow action pinning. |
| 2026-07-07 | remote-ci-green | PR #15 was mergeable and all remote checks were green at `1d3192f`; remaining gates are draft status, merge decision, and npm Trusted Publisher setup. |
| 2026-07-07 | rustsec-2026-0204 | Cleared a fresh cargo-deny failure by updating `crossbeam-epoch` to 0.9.20 and verifying cargo-deny, Rust tests, clippy, npm tests, and whitespace locally. |
| 2026-07-07 | workflow-action-pins | Added full-SHA pin enforcement for all external GitHub Actions in CI, release, and supply-chain workflows. |
| 2026-07-07 | workflow-action-pins-green | PR #15 was mergeable and all remote checks were green at `d0269fb` after full workflow action pin enforcement. |
| 2026-07-07 | npm-multi-client-examples | Bundled examples into npm package contents and made postinstall hints point to local examples plus skill directories. |
| 2026-07-07 | agent-surface-url-release | Addressed follow-up review findings for malformed URL inputs, agent docs/profile drift, full-profile packaged examples, release self-checks, and npm publish rerun behavior. |
| 2026-07-07 | resource-use-followups | Addressed remaining resource-use findings for attach-failure tab lock cleanup and bounded streamed PDF generation. |
| 2026-07-07 | live-e2e-stderr-drain | Addressed the remaining live-E2E harness stderr backpressure risk with async stderr draining and a stronger fake-bridge gate. |
| 2026-07-07 | npm-tarball-script-hygiene | Addressed published npm package script hygiene by stripping dev-only scripts from the generated tarball manifest and checking the packed manifest contents. |
| 2026-07-07 | npm-tarball-manifest-portability | Addressed Windows Git Bash tar path handling in the npm package checker and hardened stale package-backup handling. |
| 2026-07-07 | release-contract-agent-ux | Addressed subagent findings for same-version release rerun checksum drift, workflow_dispatch ref safety, stale release-order docs, CI timeout bounds, Windows skill install commands, GUI-client PATH failures, and zh-CN resources/prompts docs. |
| 2026-07-07 | harness-bounded-waits | Addressed subagent findings for unbounded mock pipe reads/task joins, live E2E doctor and cleanup waits, process env mutation in runtime-info tests, and page-asset postfetch failure marking. |
| 2026-07-07 | client-release-final-review | Addressed subagent findings for stale read-loop reconnect teardown, remaining reconnect-test sleeps, event subscription cleanup, PDF IO stream cleanup on read failure, release tag validation before checkout, and supply-chain timeout enforcement. |
| 2026-07-07 | deadline-cookie-validation | Addressed subagent findings for late CDP writes after expired deadlines and malformed cookie fields reaching CDP. |
| 2026-07-07 | monitor-event-budget | Bounded network/console monitor event processing by bytes, kept truncation metadata agent-visible, prevented noisy unpairable Network events from hiding request/response pairs, and queued deeper MCP startup/reconnect work from the incident review. |
