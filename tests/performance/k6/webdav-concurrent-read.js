import { sleep } from "k6";
import { Counter, Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
} from "./lib/config.js";
import { listWebdavAccounts, login, webdavRequest } from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const webdavReadDuration = new Trend("aster_webdav_read_duration", true);
const webdavReadBytes = new Counter("aster_webdav_read_bytes");

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_WEBDAV_READ_VUS", 8),
	duration: durationEnv("ASTER_BENCH_WEBDAV_READ_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_webdav_read_duration: [
			`p(95)<${intEnv("ASTER_BENCH_WEBDAV_READ_P95_MS", 1000)}`,
		],
	},
};

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
			`missing WebDAV read fixture ${benchConfig.webdavRangeFile}; run bun tests/performance/seed.mjs first`,
		);
	}

	return {
		fileSize: benchConfig.webdavRangeFileBytes,
	};
}

export default function (data) {
	const response = webdavRequest("GET", benchConfig.webdavRangeFile, null, {
		responseType: "none",
	});
	if (response.status !== 200) {
		throw new Error(`webdav GET failed: ${response.status}`);
	}

	webdavReadDuration.add(response.timings.duration);
	webdavReadBytes.add(data.fileSize);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("webdav-concurrent-read", [
	"aster_webdav_read_duration",
	"aster_webdav_read_bytes",
]);
