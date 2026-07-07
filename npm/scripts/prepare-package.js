#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

const packageRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(packageRoot, "..");
const packageJsonPath = path.join(packageRoot, "package.json");
const packageJsonBackupPath = path.join(packageRoot, ".package.json.dev-backup");

function copyFile(from, to) {
  fs.copyFileSync(path.join(repoRoot, from), path.join(packageRoot, to));
}

function packageJsonForPublish(pkg) {
  return {
    ...pkg,
    scripts: {
      postinstall: pkg.scripts.postinstall,
    },
  };
}

function stagePackageJson() {
  if (!fs.existsSync(packageJsonBackupPath)) {
    fs.copyFileSync(packageJsonPath, packageJsonBackupPath);
  }
  const pkg = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
  fs.writeFileSync(
    packageJsonPath,
    `${JSON.stringify(packageJsonForPublish(pkg), null, 2)}\n`
  );
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

  stagePackageJson();
}

if (require.main === module) {
  main();
}

module.exports = { main, packageJsonForPublish };
