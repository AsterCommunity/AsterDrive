import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import test from "node:test";

const scriptPath = join(
  dirname(fileURLToPath(import.meta.url)),
  "summarize-issue-497-folder-tree-memory.mjs",
);

function writeMetrics(directory, resourceCount, deletePeak, restorePeak) {
  const common = {
    resource_count: resourceCount,
    status: 202,
    request_us: 1_000,
    total_us: 2_000,
    allocated_bytes: 3_000,
    database_peak_growth_bytes: 4_000,
  };
  const lines = [
    { ...common, operation: "delete", heap_peak_growth_bytes: deletePeak },
    { ...common, operation: "restore", heap_peak_growth_bytes: restorePeak },
  ].map((metric) => `ISSUE497_METRICS ${JSON.stringify(metric)}`);
  writeFileSync(join(directory, `${resourceCount}.log`), `${lines.join("\n")}\n`);
}

function summarize(directory, sizes, maxPeakRatio) {
  return spawnSync(process.execPath, [scriptPath, directory, ...sizes.map(String)], {
    encoding: "utf8",
    env: {
      ...process.env,
      ASTER_ISSUE497_MAX_PEAK_RATIO: maxPeakRatio,
    },
  });
}

test("accepts finite positive peaks within the configured ratio", () => {
  const directory = mkdtempSync(join(tmpdir(), "issue-497-summary-valid-"));
  writeMetrics(directory, 100_000, 1_000, 2_000);
  writeMetrics(directory, 500_000, 1_100, 2_100);

  const result = summarize(directory, [100_000, 500_000], "1.25");

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /delete heap peak max\/min ratio: 1\.1000x/);
});

test("rejects a non-finite configured ratio", () => {
  const directory = mkdtempSync(join(tmpdir(), "issue-497-summary-config-"));
  writeMetrics(directory, 100_000, 1_000, 2_000);

  const result = summarize(directory, [100_000], "not-a-number");

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must be a positive finite number/);
});

test("rejects zero peak growth instead of accepting a NaN ratio", () => {
  const directory = mkdtempSync(join(tmpdir(), "issue-497-summary-zero-"));
  writeMetrics(directory, 100_000, 0, 0);

  const result = summarize(directory, [100_000], "1.25");

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /recorded unusable heap peak growth values/);
});
