import { sleep } from "k6";
import { Counter, Gauge, Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	boolEnv,
	durationEnv,
	intEnv,
} from "./lib/config.js";
import {
	createStoragePolicyMigrationTask,
	ensureRootFolder,
	listAdminTasks,
	login,
	maybeRefreshSession,
	uniqueName,
	uploadDirect,
} from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const uploadDuration = new Trend(
	"aster_mixed_storage_migration_upload_duration",
	true,
);
const uploadBytesCounter = new Counter(
	"aster_mixed_storage_migration_upload_bytes",
);
const migrationTaskCreateDuration = new Trend(
	"aster_mixed_storage_migration_task_create_duration",
	true,
);
const migrationTasksCreated = new Counter(
	"aster_mixed_storage_migration_tasks_created",
);
const taskBacklog = new Gauge("aster_mixed_storage_migration_task_backlog");

let uploadState;
let migrationState;
let probeState;

const duration = durationEnv("ASTER_BENCH_MIXED_STORAGE_MIGRATION_DURATION", "90s");
const uploadBytes = intEnv(
	"ASTER_BENCH_MIXED_STORAGE_MIGRATION_UPLOAD_BYTES",
	512 * 1024,
);
const uploadPayload = "S".repeat(uploadBytes);

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	scenarios: {
		foreground_upload: {
			executor: "constant-vus",
			vus: intEnv("ASTER_BENCH_MIXED_STORAGE_MIGRATION_UPLOAD_VUS", 4),
			duration,
			exec: "foregroundUpload",
		},
		background_migration: {
			executor: "shared-iterations",
			vus: 1,
			iterations: intEnv("ASTER_BENCH_MIXED_STORAGE_MIGRATION_TASKS", 1),
			exec: "backgroundMigration",
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
		aster_mixed_storage_migration_upload_duration: [
			`p(95)<${intEnv("ASTER_BENCH_MIXED_STORAGE_MIGRATION_UPLOAD_P95_MS", 2000)}`,
		],
		aster_mixed_storage_migration_task_create_duration: [
			`p(95)<${intEnv("ASTER_BENCH_MIXED_STORAGE_MIGRATION_TASK_CREATE_P95_MS", 3000)}`,
		],
	},
};

export function setup() {
	const sourcePolicyId = intEnv(
		"ASTER_BENCH_STORAGE_MIGRATION_SOURCE_POLICY_ID",
		0,
	);
	const targetPolicyId = intEnv(
		"ASTER_BENCH_STORAGE_MIGRATION_TARGET_POLICY_ID",
		0,
	);
	if (sourcePolicyId <= 0 || targetPolicyId <= 0) {
		throw new Error(
			"set ASTER_BENCH_STORAGE_MIGRATION_SOURCE_POLICY_ID and ASTER_BENCH_STORAGE_MIGRATION_TARGET_POLICY_ID before running this benchmark",
		);
	}

	const session = login();
	const uploadFolderId = ensureRootFolder(
		session,
		benchConfig.backgroundUploadFolder,
	);
	return {
		sourcePolicyId,
		targetPolicyId,
		deleteSourceAfterSuccess: boolEnv(
			"ASTER_BENCH_STORAGE_MIGRATION_DELETE_SOURCE_AFTER_SUCCESS",
			false,
		),
		uploadFolderId,
	};
}

export function foregroundUpload(data) {
	if (!uploadState) {
		uploadState = {
			...data,
			session: login(),
		};
	}

	uploadState.session = maybeRefreshSession(uploadState.session);
	const { response } = uploadDirect(uploadState.session, {
		filename: uniqueName("migration-mixed-upload", "bin"),
		content: uploadPayload,
		mimeType: "application/octet-stream",
		folderId: uploadState.uploadFolderId,
	});
	uploadDuration.add(response.timings.duration);
	uploadBytesCounter.add(uploadBytes);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export function backgroundMigration(data) {
	if (!migrationState) {
		migrationState = {
			...data,
			session: login(),
		};
	}

	migrationState.session = maybeRefreshSession(migrationState.session);
	const { response } = createStoragePolicyMigrationTask(migrationState.session, {
		sourcePolicyId: migrationState.sourcePolicyId,
		targetPolicyId: migrationState.targetPolicyId,
		deleteSourceAfterSuccess: migrationState.deleteSourceAfterSuccess,
	});
	migrationTaskCreateDuration.add(response.timings.duration);
	migrationTasksCreated.add(1);
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
			kind: "storage_policy_migration",
			status,
			limit: 1,
			offset: 0,
		});
		taskBacklog.add(body.data.total, { status });
	}
	sleep(
		intEnv("ASTER_BENCH_MIXED_STORAGE_MIGRATION_PROBE_INTERVAL_MS", 5000) /
			1000,
	);
}

export const handleSummary = createSummary(
	"mixed-background-storage-migration-upload",
	[
		"aster_mixed_storage_migration_upload_duration",
		"aster_mixed_storage_migration_upload_bytes",
		"aster_mixed_storage_migration_task_create_duration",
		"aster_mixed_storage_migration_tasks_created",
		"aster_mixed_storage_migration_task_backlog",
	],
);
