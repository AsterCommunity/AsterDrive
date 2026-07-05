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
	createArchiveCompressTask,
	downloadFile,
	ensureRootFolder,
	findFileEntryInFolder,
	getThumbnail,
	listAdminTasks,
	listFolder,
	listWebdavAccounts,
	login,
	maybeRefreshSession,
	resolveRootFolderId,
	uniqueName,
	uploadDirect,
	webdavRequest,
} from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const foregroundDuration = new Trend("aster_mixed_bg_foreground_duration", true);
const foregroundBytes = new Counter("aster_mixed_bg_foreground_bytes");
const foregroundOperations = new Counter("aster_mixed_bg_foreground_operations");
const archiveTaskCreateDuration = new Trend(
	"aster_mixed_bg_archive_task_create_duration",
	true,
);
const thumbnailRequestDuration = new Trend(
	"aster_mixed_bg_thumbnail_request_duration",
	true,
);
const backgroundOperations = new Counter("aster_mixed_bg_background_operations");
const taskBacklog = new Gauge("aster_mixed_bg_task_backlog");

let foregroundState;
let archiveState;
let thumbnailState;
let probeState;

const duration = durationEnv("ASTER_BENCH_MIXED_BG_DURATION", "90s");
const uploadBytes = intEnv("ASTER_BENCH_MIXED_BG_UPLOAD_BYTES", 256 * 1024);
const uploadPayload = "B".repeat(uploadBytes);

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	scenarios: {
		foreground_mixed: {
			executor: "constant-vus",
			vus: intEnv("ASTER_BENCH_MIXED_BG_FOREGROUND_VUS", 10),
			duration,
			exec: "foregroundMixed",
		},
		background_archive: {
			executor: "constant-arrival-rate",
			rate: intEnv("ASTER_BENCH_MIXED_BG_ARCHIVE_RATE", 1),
			timeUnit: "1s",
			duration,
			preAllocatedVUs: intEnv("ASTER_BENCH_MIXED_BG_ARCHIVE_VUS", 2),
			maxVUs: intEnv("ASTER_BENCH_MIXED_BG_ARCHIVE_MAX_VUS", 8),
			exec: "backgroundArchive",
		},
		background_thumbnail: {
			executor: "constant-arrival-rate",
			rate: intEnv("ASTER_BENCH_MIXED_BG_THUMBNAIL_RATE", 3),
			timeUnit: "1s",
			duration,
			preAllocatedVUs: intEnv("ASTER_BENCH_MIXED_BG_THUMBNAIL_VUS", 3),
			maxVUs: intEnv("ASTER_BENCH_MIXED_BG_THUMBNAIL_MAX_VUS", 12),
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
		aster_mixed_bg_foreground_duration: [
			`p(95)<${intEnv("ASTER_BENCH_MIXED_BG_FOREGROUND_P95_MS", 2000)}`,
		],
		aster_mixed_bg_archive_task_create_duration: [
			`p(95)<${intEnv("ASTER_BENCH_MIXED_BG_ARCHIVE_CREATE_P95_MS", 2500)}`,
		],
		aster_mixed_bg_thumbnail_request_duration: [
			`p(95)<${intEnv("ASTER_BENCH_MIXED_BG_THUMBNAIL_REQUEST_P95_MS", 1500)}`,
		],
	},
};

export function setup() {
	const session = login();
	const downloadFolderId = resolveRootFolderId(session, benchConfig.downloadFolder);
	const archiveSourceFolderId = resolveRootFolderId(
		session,
		benchConfig.archiveSourceFolder,
	);
	const thumbnailFolderId = resolveRootFolderId(
		session,
		benchConfig.thumbnailFolder,
	);
	if (!downloadFolderId || !archiveSourceFolderId || !thumbnailFolderId) {
		throw new Error(
			"missing seeded benchmark folders; run bun tests/performance/seed.mjs first",
		);
	}

	const downloadFixture = findFileEntryInFolder(
		session,
		downloadFolderId,
		benchConfig.downloadFile,
	);
	if (!downloadFixture) {
		throw new Error(
			`missing seeded file ${benchConfig.downloadFile}; run bun tests/performance/seed.mjs first`,
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

	return {
		archiveSourceFolderId,
		archiveTargetFolderId: ensureRootFolder(
			session,
			benchConfig.archiveTargetFolder,
		),
		downloadFileId: downloadFixture.id,
		downloadFileSize: Number(downloadFixture.size),
		thumbnailFileIds,
		uploadFolderId: ensureRootFolder(session, benchConfig.backgroundUploadFolder),
		webdavFileSize: benchConfig.webdavRangeFileBytes,
	};
}

export function foregroundMixed(data) {
	if (!foregroundState) {
		foregroundState = {
			...data,
			session: login(),
		};
	}

	foregroundState.session = maybeRefreshSession(foregroundState.session);
	const startedAt = Date.now();
	const op = (exec.vu.idInTest + __ITER) % 3;

	if (op === 0) {
		downloadFile(foregroundState.session, foregroundState.downloadFileId);
		foregroundBytes.add(foregroundState.downloadFileSize, {
			operation: "rest_download",
		});
		foregroundOperations.add(1, { operation: "rest_download" });
	} else if (op === 1) {
		uploadDirect(foregroundState.session, {
			filename: uniqueName("mixed-bg-upload", "bin"),
			content: uploadPayload,
			mimeType: "application/octet-stream",
			folderId: foregroundState.uploadFolderId,
		});
		foregroundBytes.add(uploadBytes, { operation: "rest_upload" });
		foregroundOperations.add(1, { operation: "rest_upload" });
	} else {
		const response = webdavRequest("GET", benchConfig.webdavRangeFile, null, {
			responseType: "none",
		});
		if (response.status !== 200) {
			throw new Error(`webdav GET failed: ${response.status}`);
		}
		foregroundBytes.add(foregroundState.webdavFileSize, {
			operation: "webdav_read",
		});
		foregroundOperations.add(1, { operation: "webdav_read" });
	}

	foregroundDuration.add(Date.now() - startedAt);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export function backgroundArchive(data) {
	if (!archiveState) {
		archiveState = {
			...data,
			session: login(),
		};
	}

	archiveState.session = maybeRefreshSession(archiveState.session);
	const { response } = createArchiveCompressTask(archiveState.session, {
		folderIds: [archiveState.archiveSourceFolderId],
		archiveName: uniqueName("mixed-bg-dispatch-archive", "zip"),
		targetFolderId: archiveState.archiveTargetFolderId,
	});
	archiveTaskCreateDuration.add(response.timings.duration);
	backgroundOperations.add(1, { operation: "archive_compress" });
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
	backgroundOperations.add(1, {
		operation: "thumbnail",
		status: String(response.status),
	});
}

export function taskProbe() {
	if (!probeState) {
		probeState = {
			session: login(),
		};
	}

	probeState.session = maybeRefreshSession(probeState.session);
	for (const kind of ["archive_compress", "thumbnail_generate"]) {
		for (const status of ["pending", "processing", "retry"]) {
			const { body } = listAdminTasks(probeState.session, {
				kind,
				status,
				limit: 1,
				offset: 0,
			});
			taskBacklog.add(body.data.total, { kind, status });
		}
	}
	sleep(intEnv("ASTER_BENCH_MIXED_BG_PROBE_INTERVAL_MS", 5000) / 1000);
}

export const handleSummary = createSummary("mixed-background-rest-webdav", [
	"aster_mixed_bg_foreground_duration",
	"aster_mixed_bg_foreground_bytes",
	"aster_mixed_bg_foreground_operations",
	"aster_mixed_bg_archive_task_create_duration",
	"aster_mixed_bg_thumbnail_request_duration",
	"aster_mixed_bg_background_operations",
	"aster_mixed_bg_task_backlog",
]);
