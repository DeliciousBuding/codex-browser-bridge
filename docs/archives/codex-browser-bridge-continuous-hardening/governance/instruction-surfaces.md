# Instruction Surfaces

## Shared

- `AGENTS.md` is the canonical shared instruction surface for Codex, Cursor-style Markdown-aware agents, and future local contributors.
- Snapshot: `governance/AGENTS.md`.

## Platform-Specific

- No `CLAUDE.md`, `.cursor/rules/`, `.windsurf/`, `.clinerules*`, or `.codex/` rule surface existed during this workflow.
- Claude/OpenClaw/Hermes/Cursor setup guidance lives in `README.md`, `README.zh-CN.md`, `examples/`, and `skills/codex-browser/SKILL.md`.

## Release Governance

- Release/tag/changelog/npm publishing rules live in `docs/release-process.md` and remain active outside this archive.
- npm publishing uses Trusted Publishing/OIDC. Maintainers must configure the npm trusted publisher before the first tokenless release.
