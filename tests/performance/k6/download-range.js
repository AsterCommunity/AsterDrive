import { sleep } from "k6";
import exec from "k6/execution";
import { Counter, Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
} from "./lib/config.js";
import {
	downloadFileRange,
	findFileEntryInFolder,
	login,
	maybeRefreshSession,
	resolveRootFolderId,
} from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const rangeDownloadDuration = new Trend("aster_download_range_duration", true);
const rangeDownloadBytes = new Counter("aster_download_range_bytes");
let state;

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_DOWNLOAD_RANGE_VUS", 8),
	duration: durationEnv("ASTER_BENCH_DOWNLOAD_RANGE_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_download_range_duration: [
			`p(95)<${intEnv("ASTER_BENCH_DOWNLOAD_RANGE_P95_MS", 600)}`,
		],
	},
};

function rangeWindow(fileSize) {
	const rangeBytes = Math.min(benchConfig.rangeBytes, fileSize);
	if (rangeBytes <= 0) {
		throw new Error("download range benchmark requires a non-empty fixture");
	}

	const maxStart = fileSize - rangeBytes;
	const start =
		maxStart <= 0
			? 0
			: (exec.scenario.iterationInTest * benchConfig.rangeStrideBytes) %
				(maxStart + 1);
	return {
		start,
		end: start + rangeBytes - 1,
		length: rangeBytes,
	};
}

export function setup() {
	const session = login();
	const folderId = resolveRootFolderId(session, benchConfig.downloadFolder);
	if (!folderId) {
		throw new Error(
			`missing seeded folder ${benchConfig.downloadFolder}; run bun tests/performance/seed.mjs first`,
		);
	}

	const file = findFileEntryInFolder(session, folderId, benchConfig.downloadFile);
	if (!file) {
		throw new Error(
			`missing seeded file ${benchConfig.downloadFile}; run bun tests/performance/seed.mjs first`,
		);
	}

	return {
		session,
		fileId: file.id,
		fileSize: Number(file.size),
	};
}

export default function (data) {
	if (!state) {
		state = data;
	}

	state.session = maybeRefreshSession(state.session);
	const range = rangeWindow(state.fileSize);
	const response = downloadFileRange(
		state.session,
		state.fileId,
		range.start,
		range.end,
	);
	rangeDownloadDuration.add(response.timings.duration);
	rangeDownloadBytes.add(range.length);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("download-range", [
	"aster_download_range_duration",
	"aster_download_range_bytes",
]);
