#!/usr/bin/env node

const { execSync } = require("child_process");

function requiredFilesForEnv(env = process.env) {
  const required = [
    "package.json",
    "scripts/install.js",
    "bin/codex-browser-bridge.js",
    "skills/codex-browser/SKILL.md",
  ];

  if (env.CODEX_BRIDGE_REQUIRE_CHECKSUMS === "1") {
    required.push("checksums.json");
  }

  return required;
}

function packedFiles() {
  const stdout = execSync("npm pack --dry-run --json", {
    cwd: __dirname + "/..",
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
  });
  const pack = JSON.parse(stdout)[0];
  return new Set(pack.files.map((file) => file.path));
}

function main() {
  const files = packedFiles();
  for (const required of requiredFilesForEnv()) {
    if (!files.has(required)) {
      throw new Error(`npm package is missing ${required}`);
    }
  }
  console.log(`npm package contents ok (${files.size} files)`);
}

if (require.main === module) {
  main();
}

module.exports = { packedFiles, requiredFilesForEnv };
