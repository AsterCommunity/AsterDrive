import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import test from "node:test";

test("benchmark outputs use private fresh directories and reject writable parents", { skip: process.platform === "win32" }, () => {
  const root = mkdtempSync(join(tmpdir(), "aster-benchmark-safety-"));
  try {
    const bin = join(root, "bin");
    mkdirSync(bin, { mode: 0o700 });
    writeFileSync(join(bin, "node"), "#!/bin/sh\nprintf 'fixture summary\\n'\n", { mode: 0o700 });
    writeFileSync(join(bin, "cargo"), "#!/bin/sh\nprintf 'fixture metric\\n'\n", { mode: 0o700 });
    const script = join(dirname(fileURLToPath(import.meta.url)), "run-issue-497-folder-tree-memory.sh");
    const revision = spawnSync("git", ["rev-parse", "--short=12", "HEAD"], { encoding: "utf8" }).stdout.trim();
    const legacy = join(root, `asterdrive-issue-497-${revision}`);
    mkdirSync(legacy);
    const victim = join(root, "victim");
    writeFileSync(victim, "unchanged");
    symlinkSync(victim, join(legacy, "wide_files-1.log"));
    const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, ASTER_ISSUE497_RESULT_DIR: root, ASTER_ISSUE497_RESOURCE_COUNTS: "1", ASTER_ISSUE497_SCENARIOS: "wide_files" };
    for (let attempt = 0; attempt < 2; attempt++) {
      const run = spawnSync("bash", [script], { env, encoding: "utf8" });
      assert.equal(run.status, 0, run.stderr);
    }
    const directories = readdirSync(root).filter((name) => name.startsWith(`asterdrive-issue-497-${revision}.`));
    assert.equal(directories.length, 2);
    for (const name of directories) {
      const output = join(root, name);
      assert.equal(statSync(output).mode & 0o777, 0o700);
      assert.equal(statSync(join(output, "wide_files-1.log")).mode & 0o777, 0o600);
      assert.equal(statSync(join(output, "summary.md")).mode & 0o777, 0o600);
    }
    assert.equal(readFileSync(victim, "utf8"), "unchanged");
    chmodSync(root, 0o777);
    const rejected = spawnSync("bash", [script], { env, encoding: "utf8" });
    assert.notEqual(rejected.status, 0);
    assert.match(rejected.stderr, /unsafe result directory ancestor/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
