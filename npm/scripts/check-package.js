#!/usr/bin/env node

const { execFileSync } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");

function requiredFilesForEnv(env = process.env) {
  const required = [
    "package.json",
    "README.md",
    "LICENSE",
    "scripts/install.js",
    "bin/codex-browser-bridge.js",
    "examples/README.md",
    "examples/claude-code.json",
    "examples/cursor.json",
    "examples/hermes-agent.json",
    "examples/openclaw.json",
    "skills/codex-browser/SKILL.md",
  ];

  if (env.CODEX_BRIDGE_REQUIRE_CHECKSUMS === "1") {
    required.push("checksums.json");
  }

  return required;
}

function npmPack(packDestination) {
  const args = ["pack", "--json", "--pack-destination", packDestination];
  const command = process.env.npm_execpath ? process.execPath : process.platform === "win32" ? "npm.cmd" : "npm";
  const commandArgs = process.env.npm_execpath ? [process.env.npm_execpath, ...args] : args;
  const stdout = execFileSync(command, commandArgs, {
    cwd: path.resolve(__dirname, ".."),
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
    shell: !process.env.npm_execpath && process.platform === "win32",
  });
  return JSON.parse(stdout)[0];
}

function tarFileText(tarballDirectory, tarballFilename, member) {
  return execFileSync("tar", ["-xOf", tarballFilename, member], {
    encoding: "utf8",
    cwd: tarballDirectory,
    stdio: ["ignore", "pipe", "inherit"],
  });
}

function packedPackage(packDestination = fs.mkdtempSync(path.join(os.tmpdir(), "codex-bridge-pack-"))) {
  try {
    const pack = npmPack(packDestination);
    const files = new Set(pack.files.map((file) => file.path));
    const packageJson = JSON.parse(
      tarFileText(packDestination, pack.filename, "package/package.json")
    );
    return { files, packageJson };
  } finally {
    fs.rmSync(packDestination, { recursive: true, force: true });
  }
}

function main() {
  const { files, packageJson } = packedPackage();
  for (const required of requiredFilesForEnv()) {
    if (!files.has(required)) {
      throw new Error(`npm package is missing ${required}`);
    }
  }
  const scripts = packageJson.scripts || {};
  const scriptNames = Object.keys(scripts).sort();
  if (JSON.stringify(scriptNames) !== JSON.stringify(["postinstall"])) {
    throw new Error(`published package scripts must only contain postinstall, got ${scriptNames.join(",")}`);
  }
  console.log(`npm package contents ok (${files.size} files)`);
}

if (require.main === module) {
  main();
}

module.exports = { packedPackage, requiredFilesForEnv };
