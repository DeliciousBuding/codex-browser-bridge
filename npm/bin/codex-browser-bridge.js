#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");
const { resolveTarget, runtimeGlibc } = require("../scripts/install");

let executable;
try {
  ({ executable } = resolveTarget(process.platform, process.arch, process.platform === "linux" ? runtimeGlibc() : undefined));
} catch (err) {
  console.error(err.message);
  process.exit(1);
}
const exe = path.join(__dirname, executable);

if (!fs.existsSync(exe)) {
  console.error(`${executable} is missing. Reinstall the package with lifecycle scripts enabled.`);
  process.exit(1);
}

const result = spawnSync(exe, process.argv.slice(2), { stdio: "inherit" });

if (result.error) {
  console.error(`failed to start ${executable}: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status === null ? 1 : result.status);
