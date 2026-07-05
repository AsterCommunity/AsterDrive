import { sleep } from "k6";
import exec from "k6/execution";
import { Counter, Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
} from "./lib/config.js";
import { listWebdavAccounts, login, webdavRequest } from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const webdavRangeDuration = new Trend("aster_webdav_range_duration", true);
const webdavRangeBytes = new Counter("aster_webdav_range_bytes");

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_WEBDAV_RANGE_VUS", 8),
	duration: durationEnv("ASTER_BENCH_WEBDAV_RANGE_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_webdav_range_duration: [
			`p(95)<${intEnv("ASTER_BENCH_WEBDAV_RANGE_P95_MS", 800)}`,
		],
	},
};

function rangeWindow(fileSize) {
	const rangeBytes = Math.min(benchConfig.rangeBytes, fileSize);
	if (rangeBytes <= 0) {
		throw new Error("WebDAV range benchmark requires a non-empty fixture");
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

function parseContentRangeTotal(contentRange) {
	const match = contentRange?.match(/^bytes \d+-\d+\/(\d+)$/);
	if (!match) {
		throw new Error(`invalid WebDAV content-range header: ${contentRange}`);
	}

	const total = Number.parseInt(match[1], 10);
	if (!Number.isFinite(total) || total <= 0) {
		throw new Error(`invalid WebDAV content-range total: ${contentRange}`);
	}

	return total;
}

export function setup() {
	const session = login();
	const { body } = listWebdavAccounts(session);
	const account = body.data.items.find(
		(item) => item.username === benchConfig.webdavUsername,
	);
	if (!account) {
		throw new Error(
			`missing WebDAV account ${benchConfig.webdavUsername}; run bun tests/performance/seed.mjs first`,
		);
	}

	const response = webdavRequest("GET", benchConfig.webdavRangeFile, null, {
		headers: {
			Range: "bytes=0-0",
		},
		responseType: "none",
	});
	if (response.status !== 206) {
		throw new Error(
			`missing WebDAV range fixture ${benchConfig.webdavRangeFile}; run bun tests/performance/seed.mjs first`,
		);
	}

	return {
		fileSize: parseContentRangeTotal(response.headers["Content-Range"]),
	};
}

export default function (data) {
	const range = rangeWindow(data.fileSize);
	const response = webdavRequest("GET", benchConfig.webdavRangeFile, null, {
		headers: {
			Range: `bytes=${range.start}-${range.end}`,
		},
		responseType: "none",
	});
	if (
		response.status !== 206 ||
		!response.headers["Content-Range"]?.startsWith(
			`bytes ${range.start}-${range.end}/`,
		)
	) {
		throw new Error(
			`webdav range GET failed: ${response.status} ${response.headers["Content-Range"]}`,
		);
	}

	webdavRangeDuration.add(response.timings.duration);
	webdavRangeBytes.add(range.length);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("webdav-range-read", [
	"aster_webdav_range_duration",
	"aster_webdav_range_bytes",
]);
