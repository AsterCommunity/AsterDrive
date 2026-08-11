import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const [, , resultDirectory, ...expectedSizeArguments] = process.argv;
if (!resultDirectory || expectedSizeArguments.length === 0) {
  throw new Error(
    "usage: node summarize-issue-497-folder-tree-memory.mjs RESULT_DIR SIZE...",
  );
}

const expectedSizes = expectedSizeArguments.map((value) => Number.parseInt(value, 10));
const metrics = [];

for (const resourceCount of expectedSizes) {
  const logPath = resolve(resultDirectory, `${resourceCount}.log`);
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

for (const resourceCount of expectedSizes) {
  for (const operation of operations) {
    const matches = metrics.filter(
      (metric) =>
        metric.resource_count === resourceCount && metric.operation === operation,
    );
    if (matches.length !== 1) {
      throw new Error(
        `expected one ${operation} metric for ${resourceCount} resources, got ${matches.length}`,
      );
    }
    if (matches[0].status !== 202) {
      throw new Error(
        `${operation} for ${resourceCount} resources returned ${matches[0].status}, expected 202`,
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
  "| Resources | Operation | HTTP | End-to-end | Heap peak growth | Cumulative allocation | DB/WAL peak growth |",
);
console.log("| ---: | --- | ---: | ---: | ---: | ---: | ---: |");
for (const resourceCount of expectedSizes) {
  for (const operation of operations) {
    const metric = metrics.find(
      (candidate) =>
        candidate.resource_count === resourceCount &&
        candidate.operation === operation,
    );
    console.log(
      `| ${resourceCount.toLocaleString("en-US")} | ${operation} | ${formatMs(metric.request_us)} ms | ${formatMs(metric.total_us)} ms | ${formatMiB(metric.heap_peak_growth_bytes)} MiB | ${formatMiB(metric.allocated_bytes)} MiB | ${formatMiB(metric.database_peak_growth_bytes)} MiB |`,
    );
  }
}

for (const operation of operations) {
  const peaks = metrics
    .filter((metric) => metric.operation === operation)
    .map((metric) => metric.heap_peak_growth_bytes);
  const minPeak = Math.min(...peaks);
  const maxPeak = Math.max(...peaks);
  if (!Number.isFinite(minPeak) || minPeak <= 0 || !Number.isFinite(maxPeak)) {
    throw new Error(
      `${operation} recorded unusable heap peak growth values (${peaks.join(", ")})`,
    );
  }
  const ratio = maxPeak / minPeak;
  console.log(`${operation} heap peak max/min ratio: ${ratio.toFixed(4)}x`);
  if (!Number.isFinite(ratio) || ratio > maxPeakRatio) {
    throw new Error(
      `${operation} heap peak ratio ${ratio.toFixed(4)} exceeds ${maxPeakRatio.toFixed(4)}`,
    );
  }
}
