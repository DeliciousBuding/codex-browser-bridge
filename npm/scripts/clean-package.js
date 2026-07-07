#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

const packageRoot = path.resolve(__dirname, "..");
const packageJsonPath = path.join(packageRoot, "package.json");
const packageJsonBackupPath = path.join(packageRoot, ".package.json.dev-backup");

for (const entry of ["README.md", "LICENSE", "examples", "skills"]) {
  fs.rmSync(path.join(packageRoot, entry), { recursive: true, force: true });
}

if (fs.existsSync(packageJsonBackupPath)) {
  fs.copyFileSync(packageJsonBackupPath, packageJsonPath);
  fs.rmSync(packageJsonBackupPath, { force: true });
}
