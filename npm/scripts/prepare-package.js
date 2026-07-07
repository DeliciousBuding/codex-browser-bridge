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

function stagePackageJson(options = {}) {
  const targetPackageJsonPath = options.packageJsonPath || packageJsonPath;
  const targetPackageJsonBackupPath = options.packageJsonBackupPath || packageJsonBackupPath;
  if (fs.existsSync(targetPackageJsonBackupPath)) {
    throw new Error(
      `stale package backup exists at ${targetPackageJsonBackupPath}; run node scripts/clean-package.js or inspect it before packing`
    );
  }
  fs.copyFileSync(targetPackageJsonPath, targetPackageJsonBackupPath);
  const pkg = JSON.parse(fs.readFileSync(targetPackageJsonPath, "utf8"));
  fs.writeFileSync(
    targetPackageJsonPath,
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

module.exports = { main, packageJsonForPublish, stagePackageJson };
