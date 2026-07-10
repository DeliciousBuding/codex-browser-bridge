# Milestones

| # | Milestone | Target Criteria | Status |
|:--|:--|:--|:--|
| 1 | Safety guardrails merged | URL, MCP line, duration, and raw CDP tests pass; clippy clean | Complete |
| 2 | Release automation clean | npm skill packaged before dry-run/publish; Dependabot respects MSRV; release policy documented | Complete |
| 3 | Harness coverage improved | Handler tests and mock E2E cover key tool paths; optional live E2E documented | Complete |
| 4 | Agent install experience clear | README, examples, skill, AGENTS align for Claude/OpenClaw/Hermes/Cursor | Complete |

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
