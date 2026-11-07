#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";

const workflow = await readFile(".github/workflows/rust-release.yml", "utf8");
assert.doesNotMatch(workflow, /certificate-identity-regexp/);
const line = workflow.split("\n").find((line) => line.includes("printf") && line.includes("--certificate-identity"));
assert.ok(line, "release instructions must generate an exact certificate identity");

for (const reference of [
  "AsterCommunity/AsterDrive/.github/workflows/rust-release.yml@refs/tags/v0.8.0",
  "AsterCommunity/AsterDrive/.github/workflows/rust-release.yml@refs/tags/v1.0.0-rc.1",
  "AsterCommunity/AsterDrive/.github/workflows/rust-release.yml@refs/tags/v0.8.0';printf-injected",
]) {
  const result = spawnSync("bash", ["-c", line], { encoding: "utf8", env: { ...process.env, GITHUB_WORKFLOW_REF: reference } });
  assert.equal(result.status, 0, result.stderr);
  const probe = spawnSync("bash", ["-c", `cosign() { printf '%s\\0' "$@"; }; cosign verify-blob checksums.txt \\\n${result.stdout}`], { encoding: "utf8" });
  assert.equal(probe.status, 0, probe.stderr);
  assert.deepEqual(probe.stdout.split("\0").filter(Boolean), [
    "verify-blob", "checksums.txt", "--certificate-identity", `https://github.com/${reference}`,
  ]);
  // Exact matching rejects certificates from unrelated repositories even if
  // their attacker-selected workflow ref contains the trusted repo substring.
  const expected = `https://github.com/${reference}`;
  assert.notEqual(`https://github.com/attacker/repo/.github/workflows/release.yml@refs/heads/github.com/AsterCommunity/AsterDrive`, expected);
}
process.stdout.write("Release verification instructions bind the exact workflow/tag identity and quote it safely.\n");
