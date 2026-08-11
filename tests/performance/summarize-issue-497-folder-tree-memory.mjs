import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const [, , resultDirectory, ...expectedScenarioArguments] = process.argv;
if (!resultDirectory || expectedScenarioArguments.length === 0) {
  throw new Error(
    "usage: node summarize-issue-497-folder-tree-memory.mjs RESULT_DIR SCENARIO:RESOURCE_COUNT...",
  );
}

const knownScenarios = new Set(["wide_files", "wide_folders", "deep_chain"]);
const expectedScenarios = expectedScenarioArguments.map((argument) => {
  const [scenario, resourceCountRaw, ...extra] = argument.split(":");
  const resourceCount = Number(resourceCountRaw);
  if (
    !knownScenarios.has(scenario) ||
    extra.length !== 0 ||
    !Number.isSafeInteger(resourceCount) ||
    resourceCount <= 0
  ) {
    throw new Error(
      `expected SCENARIO:RESOURCE_COUNT with a known scenario and positive integer count, got "${argument}"`,
    );
  }
  return { scenario, resourceCount };
});

const metrics = [];
for (const { scenario, resourceCount } of expectedScenarios) {
  const logPath = resolve(resultDirectory, `${scenario}-${resourceCount}.log`);
  const log = readFileSync(logPath, "utf8");
  for (const line of log.split("\n")) {
    const prefix = "ISSUE497_METRICS ";
    const offset = line.indexOf(prefix);
    if (offset >= 0) {
      metrics.push(JSON.parse(line.slice(offset + prefix.length)));
    }
  }
}

const operations = ["delete", "restore"];
const maxPeakRatioRaw = process.env.ASTER_ISSUE497_MAX_PEAK_RATIO ?? "1.25";
const maxPeakRatio = Number(maxPeakRatioRaw);
if (!Number.isFinite(maxPeakRatio) || maxPeakRatio <= 0) {
  throw new Error(
    `ASTER_ISSUE497_MAX_PEAK_RATIO must be a positive finite number, got "${maxPeakRatioRaw}"`,
  );
}

for (const { scenario, resourceCount } of expectedScenarios) {
  for (const operation of operations) {
    const matches = metrics.filter(
      (metric) =>
        metric.scenario === scenario &&
        metric.resource_count === resourceCount &&
        metric.operation === operation,
    );
    if (matches.length !== 1) {
      throw new Error(
        `expected one ${operation} metric for ${scenario}:${resourceCount}, got ${matches.length}`,
      );
    }
    const [metric] = matches;
    if (metric.status !== 202) {
      throw new Error(
        `${operation} for ${scenario}:${resourceCount} returned ${metric.status}, expected 202`,
      );
    }
    if (
      !Number.isSafeInteger(metric.folder_count) ||
      metric.folder_count <= 0 ||
      !Number.isSafeInteger(metric.file_count) ||
      metric.file_count < 0 ||
      metric.folder_count + metric.file_count !== resourceCount
    ) {
      throw new Error(
        `${operation} for ${scenario}:${resourceCount} recorded inconsistent folder/file counts`,
      );
    }
    if (
      !Number.isFinite(metric.heap_peak_growth_bytes) ||
      metric.heap_peak_growth_bytes <= 0
    ) {
      throw new Error(
        `${operation} for ${scenario}:${resourceCount} recorded unusable heap peak growth ${metric.heap_peak_growth_bytes}`,
      );
    }
  }
}

function formatMiB(bytes) {
  return (bytes / 1024 / 1024).toFixed(2);
}

function formatMs(microseconds) {
  return (microseconds / 1000).toFixed(2);
}

console.log(
  "| Scenario | Resources | Folders | Files | Operation | HTTP | Request | End-to-end | Heap peak growth | Cumulative allocation | DB/WAL peak growth |",
);
console.log("| --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |");
for (const { scenario, resourceCount } of expectedScenarios) {
  for (const operation of operations) {
    const metric = metrics.find(
      (candidate) =>
        candidate.scenario === scenario &&
        candidate.resource_count === resourceCount &&
        candidate.operation === operation,
    );
    console.log(
      `| ${scenario} | ${resourceCount.toLocaleString("en-US")} | ${metric.folder_count.toLocaleString("en-US")} | ${metric.file_count.toLocaleString("en-US")} | ${operation} | ${metric.status} | ${formatMs(metric.request_us)} ms | ${formatMs(metric.total_us)} ms | ${formatMiB(metric.heap_peak_growth_bytes)} MiB | ${formatMiB(metric.allocated_bytes)} MiB | ${formatMiB(metric.database_peak_growth_bytes)} MiB |`,
    );
  }
}

const wideFileMetrics = metrics.filter((metric) => metric.scenario === "wide_files");
for (const operation of operations) {
  const peaks = wideFileMetrics
    .filter((metric) => metric.operation === operation)
    .map((metric) => metric.heap_peak_growth_bytes);
  if (peaks.length === 0) {
    continue;
  }
  const minPeak = Math.min(...peaks);
  const maxPeak = Math.max(...peaks);
  const ratio = maxPeak / minPeak;
  console.log(`wide_files ${operation} heap peak max/min ratio: ${ratio.toFixed(4)}x`);
  if (!Number.isFinite(ratio) || ratio > maxPeakRatio) {
    throw new Error(
      `wide_files ${operation} heap peak ratio ${ratio.toFixed(4)} exceeds ${maxPeakRatio.toFixed(4)}`,
    );
  }
}
