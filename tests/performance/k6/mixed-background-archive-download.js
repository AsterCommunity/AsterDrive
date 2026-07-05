import { sleep } from "k6";
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
	listAdminTasks,
	login,
	maybeRefreshSession,
	resolveRootFolderId,
	uniqueName,
} from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const foregroundDownloadDuration = new Trend(
	"aster_mixed_archive_download_duration",
	true,
);
const foregroundDownloadBytes = new Counter("aster_mixed_archive_download_bytes");
const archiveTaskCreateDuration = new Trend(
	"aster_mixed_archive_task_create_duration",
	true,
);
const archiveTasksCreated = new Counter("aster_mixed_archive_tasks_created");
const taskBacklog = new Gauge("aster_mixed_archive_task_backlog");

let foregroundState;
let archiveState;
let probeState;

const duration = durationEnv("ASTER_BENCH_MIXED_ARCHIVE_DURATION", "60s");

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	scenarios: {
		foreground_download: {
			executor: "constant-vus",
			vus: intEnv("ASTER_BENCH_MIXED_ARCHIVE_DOWNLOAD_VUS", 6),
			duration,
			exec: "foregroundDownload",
		},
		background_archive: {
			executor: "constant-arrival-rate",
			rate: intEnv("ASTER_BENCH_MIXED_ARCHIVE_TASK_RATE", 1),
			timeUnit: "1s",
			duration,
			preAllocatedVUs: intEnv("ASTER_BENCH_MIXED_ARCHIVE_TASK_VUS", 2),
			maxVUs: intEnv("ASTER_BENCH_MIXED_ARCHIVE_TASK_MAX_VUS", 8),
			exec: "backgroundArchive",
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
		aster_mixed_archive_download_duration: [
			`p(95)<${intEnv("ASTER_BENCH_MIXED_ARCHIVE_DOWNLOAD_P95_MS", 1500)}`,
		],
		aster_mixed_archive_task_create_duration: [
			`p(95)<${intEnv("ASTER_BENCH_MIXED_ARCHIVE_TASK_CREATE_P95_MS", 2000)}`,
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
	if (!downloadFolderId || !archiveSourceFolderId) {
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

	const archiveTargetFolderId = ensureRootFolder(
		session,
		benchConfig.archiveTargetFolder,
	);
	return {
		downloadFileId: downloadFixture.id,
		downloadFileSize: Number(downloadFixture.size),
		archiveSourceFolderId,
		archiveTargetFolderId,
	};
}

export function foregroundDownload(data) {
	if (!foregroundState) {
		foregroundState = {
			...data,
			session: login(),
		};
	}

	foregroundState.session = maybeRefreshSession(foregroundState.session);
	const response = downloadFile(
		foregroundState.session,
		foregroundState.downloadFileId,
	);
	foregroundDownloadDuration.add(response.timings.duration);
	foregroundDownloadBytes.add(foregroundState.downloadFileSize);

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
		archiveName: uniqueName("mixed-bg-archive", "zip"),
		targetFolderId: archiveState.archiveTargetFolderId,
	});
	archiveTaskCreateDuration.add(response.timings.duration);
	archiveTasksCreated.add(1);
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
			kind: "archive_compress",
			status,
			limit: 1,
			offset: 0,
		});
		taskBacklog.add(body.data.total, { status });
	}
	sleep(intEnv("ASTER_BENCH_MIXED_ARCHIVE_PROBE_INTERVAL_MS", 5000) / 1000);
}

export const handleSummary = createSummary("mixed-background-archive-download", [
	"aster_mixed_archive_download_duration",
	"aster_mixed_archive_download_bytes",
	"aster_mixed_archive_task_create_duration",
	"aster_mixed_archive_tasks_created",
	"aster_mixed_archive_task_backlog",
]);
