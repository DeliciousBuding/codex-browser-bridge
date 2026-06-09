#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const exe = path.join(__dirname, "codex-browser-bridge.exe");

if (!fs.existsSync(exe)) {
  console.error("codex-browser-bridge.exe is missing. Reinstall the package with lifecycle scripts enabled.");
  process.exit(1);
}

const result = spawnSync(exe, process.argv.slice(2), { stdio: "inherit" });

if (result.error) {
  console.error(`failed to start codex-browser-bridge.exe: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status === null ? 1 : result.status);
