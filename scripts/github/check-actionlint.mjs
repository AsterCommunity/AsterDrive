#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";

const baseline = JSON.parse(
  await readFile(new URL("./actionlint-baseline.json", import.meta.url), "utf8"),
);
const result = spawnSync("actionlint", ["-format", "{{json .}}"], {
  cwd: process.cwd(),
  encoding: "utf8",
});
if (result.error) throw result.error;

let findings;
try {
  findings = JSON.parse(result.stdout || "[]");
} catch {
  process.stderr.write(result.stdout);
  process.stderr.write(result.stderr);
  process.exit(1);
}

const expected = baseline.flatMap((entry) =>
  Array.from({ length: entry.count || 1 }, () => ({
    filepath: entry.filepath,
    line: entry.line,
    code: entry.code,
  })),
);
const actual = findings.map((finding) => ({
  filepath: finding.filepath,
  line: finding.line,
  code: finding.message.match(/\b(SC\d+)\b/)?.[1] || finding.kind,
}));

const key = (finding) => `${finding.filepath}:${finding.line}:${finding.code}`;
const expectedKeys = expected.map(key).sort();
const actualKeys = actual.map(key).sort();
if (JSON.stringify(actualKeys) !== JSON.stringify(expectedKeys)) {
  process.stderr.write("actionlint findings differ from the reviewed baseline.\n");
  process.stderr.write(`Expected:\n${expectedKeys.join("\n")}\n`);
  process.stderr.write(`Actual:\n${actualKeys.join("\n")}\n`);
  process.exit(1);
}

process.stdout.write(`actionlint passed with ${actual.length} reviewed baseline finding(s).\n`);

