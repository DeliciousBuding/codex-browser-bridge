#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

const packageRoot = path.resolve(__dirname, "..");

for (const entry of ["README.md", "LICENSE", "examples", "skills"]) {
  fs.rmSync(path.join(packageRoot, entry), { recursive: true, force: true });
}
