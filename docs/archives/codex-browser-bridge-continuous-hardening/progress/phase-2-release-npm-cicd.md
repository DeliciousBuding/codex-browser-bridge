# Phase 2: Release, npm, and CI/CD Hygiene

- [x] T2.1 Fix npm skill packaging order
  - Acceptance: CI/release package checks copy `skills/` before `npm pack --dry-run`; normal and checksum-required package checks pass.
- [x] T2.2 Encode MSRV policy for Dependabot
  - Acceptance: `criterion` semver-minor updates are ignored while Rust MSRV remains 1.85.
- [x] T2.3 Add release/tag/changelog policy doc
  - Acceptance: SemVer tag, annotated tag, changelog, release PR, and local preflight rules are documented.
- [x] T2.4 Harden release permissions and publish path
  - Acceptance: workflows use least needed permissions; release tags require `vX.Y.Z`; release assets get attestations; npm publish uses Trusted Publishing/OIDC.

## Notes

Local verification covered npm tests, package dry-runs, and workflow review. `actionlint` is not installed locally. npm Trusted Publisher configuration is an external maintainer prerequisite before the first OIDC publish.
