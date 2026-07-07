#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");

const workflowsDir = path.join(process.cwd(), ".github", "workflows");
const shaRef = /^[0-9a-f]{40}$/;
const failures = [];

function workflowFiles(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && /\.ya?ml$/i.test(entry.name))
    .map((entry) => path.join(dir, entry.name));
}

for (const file of workflowFiles(workflowsDir)) {
  const rel = path.relative(process.cwd(), file).replace(/\\/g, "/");
  const lines = fs.readFileSync(file, "utf8").split(/\r?\n/);

  lines.forEach((line, index) => {
    const match = line.match(/^\s*(?:-\s*)?uses:\s*([^#\s]+)/);
    if (!match) return;

    const spec = match[1].replace(/^["']|["']$/g, "");
    if (spec.startsWith("./") || spec.startsWith("../") || spec.startsWith("docker://")) {
      return;
    }

    const at = spec.lastIndexOf("@");
    if (at < 0) {
      failures.push(`${rel}:${index + 1}: missing @<sha> in ${spec}`);
      return;
    }

    const ref = spec.slice(at + 1);
    if (!shaRef.test(ref)) {
      failures.push(`${rel}:${index + 1}: action ref must be a full commit SHA: ${spec}`);
    }
  });
}

if (failures.length) {
  console.error("GitHub Actions must pin external actions to full commit SHAs:");
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log("github action pins ok");
