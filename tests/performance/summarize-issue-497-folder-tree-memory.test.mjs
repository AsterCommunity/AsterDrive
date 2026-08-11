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

function metricPair({
  scenario,
  resourceCount,
  folderCount,
  fileCount,
  deletePeak = 1_000,
  restorePeak = 2_000,
  status = 202,
}) {
  const common = {
    scenario,
    resource_count: resourceCount,
    folder_count: folderCount,
    file_count: fileCount,
    status,
    request_us: 1_000,
    total_us: 2_000,
    allocated_bytes: 3_000,
    database_peak_growth_bytes: 4_000,
  };
  return [
    { ...common, operation: "delete", heap_peak_growth_bytes: deletePeak },
    { ...common, operation: "restore", heap_peak_growth_bytes: restorePeak },
  ];
}

function writeMetrics(directory, scenario, resourceCount, metrics) {
  const lines = metrics.map(
    (metric) => `ISSUE497_METRICS ${JSON.stringify(metric)}`,
  );
  writeFileSync(
    join(directory, `${scenario}-${resourceCount}.log`),
    `${lines.join("\n")}\n`,
  );
}

function summarize(directory, scenarios, maxPeakRatio = "1.25") {
  return spawnSync(process.execPath, [scriptPath, directory, ...scenarios], {
    encoding: "utf8",
    env: {
      ...process.env,
      ASTER_ISSUE497_MAX_PEAK_RATIO: maxPeakRatio,
    },
  });
}

test("accepts the wide-file and shape-boundary matrix", () => {
  const directory = mkdtempSync(join(tmpdir(), "issue-497-summary-valid-"));
  writeMetrics(
    directory,
    "wide_files",
    100_000,
    metricPair({
      scenario: "wide_files",
      resourceCount: 100_000,
      folderCount: 1,
      fileCount: 99_999,
      deletePeak: 1_000,
      restorePeak: 2_000,
    }),
  );
  writeMetrics(
    directory,
    "wide_files",
    500_000,
    metricPair({
      scenario: "wide_files",
      resourceCount: 500_000,
      folderCount: 1,
      fileCount: 499_999,
      deletePeak: 1_100,
      restorePeak: 2_100,
    }),
  );
  writeMetrics(
    directory,
    "wide_folders",
    2_002,
    metricPair({
      scenario: "wide_folders",
      resourceCount: 2_002,
      folderCount: 2_002,
      fileCount: 0,
    }),
  );
  writeMetrics(
    directory,
    "deep_chain",
    130,
    metricPair({
      scenario: "deep_chain",
      resourceCount: 130,
      folderCount: 130,
      fileCount: 0,
    }),
  );

  const result = summarize(directory, [
    "wide_files:100000",
    "wide_files:500000",
    "wide_folders:2002",
    "deep_chain:130",
  ]);

  assert.equal(result.status, 0, result.stderr);
  assert.match(
    result.stdout,
    /wide_files delete heap peak max\/min ratio: 1\.1000x/,
  );
  assert.match(result.stdout, /\| wide_folders \| 2,002 \| 2,002 \| 0 \|/);
  assert.match(result.stdout, /\| deep_chain \| 130 \| 130 \| 0 \|/);
});

test("rejects a missing operation for an expected scenario", () => {
  const directory = mkdtempSync(join(tmpdir(), "issue-497-summary-missing-"));
  const [deleteMetric] = metricPair({
    scenario: "deep_chain",
    resourceCount: 130,
    folderCount: 130,
    fileCount: 0,
  });
  writeMetrics(directory, "deep_chain", 130, [deleteMetric]);

  const result = summarize(directory, ["deep_chain:130"]);

  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /expected one restore metric for deep_chain:130, got 0/,
  );
});

test("rejects duplicate metrics", () => {
  const directory = mkdtempSync(join(tmpdir(), "issue-497-summary-duplicate-"));
  const metrics = metricPair({
    scenario: "wide_folders",
    resourceCount: 2_002,
    folderCount: 2_002,
    fileCount: 0,
  });
  writeMetrics(directory, "wide_folders", 2_002, [metrics[0], ...metrics]);

  const result = summarize(directory, ["wide_folders:2002"]);

  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /expected one delete metric for wide_folders:2002, got 2/,
  );
});

test("rejects a non-202 operation", () => {
  const directory = mkdtempSync(join(tmpdir(), "issue-497-summary-status-"));
  writeMetrics(
    directory,
    "deep_chain",
    130,
    metricPair({
      scenario: "deep_chain",
      resourceCount: 130,
      folderCount: 130,
      fileCount: 0,
      status: 200,
    }),
  );

  const result = summarize(directory, ["deep_chain:130"]);

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /returned 200, expected 202/);
});

test("rejects inconsistent fixture counts", () => {
  const directory = mkdtempSync(join(tmpdir(), "issue-497-summary-counts-"));
  writeMetrics(
    directory,
    "wide_folders",
    2_002,
    metricPair({
      scenario: "wide_folders",
      resourceCount: 2_002,
      folderCount: 2_001,
      fileCount: 0,
    }),
  );

  const result = summarize(directory, ["wide_folders:2002"]);

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /recorded inconsistent folder\/file counts/);
});

test("rejects a non-finite configured ratio", () => {
  const directory = mkdtempSync(join(tmpdir(), "issue-497-summary-config-"));
  writeMetrics(
    directory,
    "wide_files",
    100_000,
    metricPair({
      scenario: "wide_files",
      resourceCount: 100_000,
      folderCount: 1,
      fileCount: 99_999,
    }),
  );

  const result = summarize(directory, ["wide_files:100000"], "not-a-number");

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must be a positive finite number/);
});

test("rejects zero peak growth", () => {
  const directory = mkdtempSync(join(tmpdir(), "issue-497-summary-zero-"));
  writeMetrics(
    directory,
    "wide_files",
    100_000,
    metricPair({
      scenario: "wide_files",
      resourceCount: 100_000,
      folderCount: 1,
      fileCount: 99_999,
      deletePeak: 0,
    }),
  );

  const result = summarize(directory, ["wide_files:100000"]);

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /recorded unusable heap peak growth 0/);
});

test("rejects a wide-file ratio above the configured limit", () => {
  const directory = mkdtempSync(join(tmpdir(), "issue-497-summary-ratio-"));
  for (const [resourceCount, peak] of [
    [100_000, 1_000],
    [500_000, 1_500],
  ]) {
    writeMetrics(
      directory,
      "wide_files",
      resourceCount,
      metricPair({
        scenario: "wide_files",
        resourceCount,
        folderCount: 1,
        fileCount: resourceCount - 1,
        deletePeak: peak,
        restorePeak: peak,
      }),
    );
  }

  const result = summarize(directory, [
    "wide_files:100000",
    "wide_files:500000",
  ]);

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /heap peak ratio 1\.5000 exceeds 1\.2500/);
});
