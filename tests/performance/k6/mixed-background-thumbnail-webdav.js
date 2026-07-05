import { sleep } from "k6";
import exec from "k6/execution";
import { Counter, Gauge, Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
} from "./lib/config.js";
import {
	getThumbnail,
	listAdminTasks,
	listFolder,
	listWebdavAccounts,
	login,
	maybeRefreshSession,
	resolveRootFolderId,
	webdavRequest,
} from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const webdavReadDuration = new Trend(
	"aster_mixed_thumbnail_webdav_read_duration",
	true,
);
const webdavReadBytes = new Counter("aster_mixed_thumbnail_webdav_read_bytes");
const thumbnailRequestDuration = new Trend(
	"aster_mixed_thumbnail_request_duration",
	true,
);
const thumbnailRequests = new Counter("aster_mixed_thumbnail_requests");
const taskBacklog = new Gauge("aster_mixed_thumbnail_task_backlog");

let webdavState;
let thumbnailState;
let probeState;

const duration = durationEnv("ASTER_BENCH_MIXED_THUMBNAIL_DURATION", "60s");

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	scenarios: {
		foreground_webdav_read: {
			executor: "constant-vus",
			vus: intEnv("ASTER_BENCH_MIXED_THUMBNAIL_WEBDAV_VUS", 8),
			duration,
			exec: "foregroundWebdavRead",
		},
		background_thumbnail: {
			executor: "constant-arrival-rate",
			rate: intEnv("ASTER_BENCH_MIXED_THUMBNAIL_REQUEST_RATE", 4),
			timeUnit: "1s",
			duration,
			preAllocatedVUs: intEnv("ASTER_BENCH_MIXED_THUMBNAIL_REQUEST_VUS", 4),
			maxVUs: intEnv("ASTER_BENCH_MIXED_THUMBNAIL_REQUEST_MAX_VUS", 16),
			exec: "backgroundThumbnail",
		},
		task_probe: {
			executor: "constant-vus",
			vus: 1,
			duration,
			exec: "taskProbe",
		},
	},
	thresholds: {
		http_req_failed: ["rate<0.02"],
		aster_mixed_thumbnail_webdav_read_duration: [
			`p(95)<${intEnv("ASTER_BENCH_MIXED_THUMBNAIL_WEBDAV_P95_MS", 1500)}`,
		],
		aster_mixed_thumbnail_request_duration: [
			`p(95)<${intEnv("ASTER_BENCH_MIXED_THUMBNAIL_REQUEST_P95_MS", 1500)}`,
		],
	},
};

export function setup() {
	const session = login();
	const { body: accountsBody } = listWebdavAccounts(session);
	const account = accountsBody.data.items.find(
		(item) => item.username === benchConfig.webdavUsername,
	);
	if (!account) {
		throw new Error(
			`missing WebDAV account ${benchConfig.webdavUsername}; run bun tests/performance/seed.mjs first`,
		);
	}

	const webdavProbe = webdavRequest("GET", benchConfig.webdavRangeFile, null, {
		headers: {
			Range: "bytes=0-0",
		},
		responseType: "none",
	});
	if (webdavProbe.status !== 206) {
		throw new Error(
			`missing WebDAV read fixture ${benchConfig.webdavRangeFile}; run bun tests/performance/seed.mjs first`,
		);
	}

	const thumbnailFolderId = resolveRootFolderId(
		session,
		benchConfig.thumbnailFolder,
	);
	if (!thumbnailFolderId) {
		throw new Error(
			`missing seeded folder ${benchConfig.thumbnailFolder}; run bun tests/performance/seed.mjs first`,
		);
	}

	const { body: thumbnailBody } = listFolder(session, thumbnailFolderId, {
		folder_limit: 0,
		file_limit: benchConfig.thumbnailImageCount,
		sort_by: "name",
		sort_order: "asc",
	});
	const thumbnailFileIds = thumbnailBody.data.files.map((file) => file.id);
	if (thumbnailFileIds.length === 0) {
		throw new Error(
			`missing thumbnail fixtures in ${benchConfig.thumbnailFolder}; run bun tests/performance/seed.mjs first`,
		);
	}

	return {
		thumbnailFileIds,
		webdavFileSize: benchConfig.webdavRangeFileBytes,
	};
}

export function foregroundWebdavRead(data) {
	if (!webdavState) {
		webdavState = data;
	}

	const response = webdavRequest("GET", benchConfig.webdavRangeFile, null, {
		responseType: "none",
	});
	if (response.status !== 200) {
		throw new Error(`webdav GET failed: ${response.status}`);
	}

	webdavReadDuration.add(response.timings.duration);
	webdavReadBytes.add(webdavState.webdavFileSize);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export function backgroundThumbnail(data) {
	if (!thumbnailState) {
		thumbnailState = {
			...data,
			session: login(),
		};
	}

	thumbnailState.session = maybeRefreshSession(thumbnailState.session);
	const index =
		exec.scenario.iterationInTest % thumbnailState.thumbnailFileIds.length;
	const response = getThumbnail(
		thumbnailState.session,
		thumbnailState.thumbnailFileIds[index],
	);
	thumbnailRequestDuration.add(response.timings.duration);
	thumbnailRequests.add(1, { status: String(response.status) });
}

export function taskProbe() {
	if (!probeState) {
		probeState = {
			session: login(),
		};
	}

	probeState.session = maybeRefreshSession(probeState.session);
	for (const status of ["pending", "processing", "retry"]) {
		const { body } = listAdminTasks(probeState.session, {
			kind: "thumbnail_generate",
			status,
			limit: 1,
			offset: 0,
		});
		taskBacklog.add(body.data.total, { status });
	}
	sleep(intEnv("ASTER_BENCH_MIXED_THUMBNAIL_PROBE_INTERVAL_MS", 5000) / 1000);
}

export const handleSummary = createSummary("mixed-background-thumbnail-webdav", [
	"aster_mixed_thumbnail_webdav_read_duration",
	"aster_mixed_thumbnail_webdav_read_bytes",
	"aster_mixed_thumbnail_request_duration",
	"aster_mixed_thumbnail_requests",
	"aster_mixed_thumbnail_task_backlog",
]);
