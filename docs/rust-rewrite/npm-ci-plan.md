# npm and CI Plan

This branch keeps the npm package compatible with the current Go releases while the Rust rewrite reaches parity.

## Current npm Behavior

`npm/scripts/install.js` still downloads the release assets used by the published package:

- `codex-browser-bridge.exe` for Windows x64
- `codex-browser-bridge-arm64.exe` for Windows arm64
- `checksums.json` from the package, with `checksums.txt` from the GitHub Release as fallback

Do not change `npm/package.json` version during rewrite work. Do not publish from this branch until Rust release assets are built and verified.

## Local Rust Build

Use these commands from the repository root:

```bash
cargo check --locked
cargo test --locked
cargo build --locked --release
```

The Rust binary is generated at:

```text
target/release/codex-browser-bridge.exe
```

For local MCP testing, point the client at that absolute path and keep the same arguments:

```json
{
  "mcpServers": {
    "codex-browser": {
      "command": "D:/path/to/codex-browser-bridge/target/release/codex-browser-bridge.exe",
      "args": ["--mode", "mcp"],
      "transport": "stdio"
    }
  }
}
```

## CI Draft

The `ci.yml` workflow now has a `rust` job on `windows-latest`:

```bash
cargo check --locked
cargo test --locked
cargo build --locked --release
```

The existing Go and npm jobs remain in place. Go is still the release baseline until parity is complete.

## Release Switch Criteria

Switch npm release assets to Rust only after all items are true:

- Rust tests cover the wire-format, browser API, MCP, and pipe discovery cases listed in `design.md`.
- CI builds Windows x64 and arm64 Rust binaries.
- GitHub Release uploads Rust binaries with the same asset names npm already expects.
- `npm test` passes with embedded checksums for both asset names.
- A manual install from a draft release validates checksum download, binary placement, and `--version`.
