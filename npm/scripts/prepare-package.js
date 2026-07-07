#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

const packageRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(packageRoot, "..");

function copyFile(from, to) {
  fs.copyFileSync(path.join(repoRoot, from), path.join(packageRoot, to));
}

function main() {
  copyFile("README.md", "README.md");
  copyFile("LICENSE", "LICENSE");

  const skillTarget = path.join(packageRoot, "skills");
  fs.rmSync(skillTarget, { recursive: true, force: true });
  fs.cpSync(path.join(repoRoot, "skills"), skillTarget, { recursive: true });

  const examplesTarget = path.join(packageRoot, "examples");
  fs.rmSync(examplesTarget, { recursive: true, force: true });
  fs.cpSync(path.join(repoRoot, "examples"), examplesTarget, { recursive: true });
}

if (require.main === module) {
  main();
}

module.exports = { main };
