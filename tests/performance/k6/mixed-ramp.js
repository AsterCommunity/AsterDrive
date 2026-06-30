import { sleep } from "k6";
import exec from "k6/execution";
import { Counter, Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
	listFolderName,
	stagesEnv,
} from "./lib/config.js";
import {
	downloadFile,
	ensureRootFolder,
	findFileEntryInFolder,
	listFolder,
	login,
	maybeRefreshSession,
	refreshSession,
	resolveRootFolderId,
	search,
	uniqueName,
	uploadDirect,
} from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const mixedRampDuration = new Trend("aster_mixed_ramp_flow_duration", true);
const mixedRampBytes = new Counter("aster_mixed_ramp_bytes");
const mixedRampOperations = new Counter("aster_mixed_ramp_operations");
const listSize = intEnv("ASTER_BENCH_MIXED_RAMP_LIST_SIZE", 10000);
const listLimit = intEnv("ASTER_BENCH_MIXED_RAMP_LIST_LIMIT", 100);
const searchLimit = intEnv("ASTER_BENCH_MIXED_RAMP_SEARCH_LIMIT", 50);
const uploadBytes = intEnv("ASTER_BENCH_MIXED_RAMP_UPLOAD_BYTES", 32 * 1024);
const uploadPayload = "M".repeat(uploadBytes);
let state;

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	scenarios: {
		mixed_ramp: {
			executor: "ramping-vus",
			stages: stagesEnv(
				"ASTER_BENCH_MIXED_RAMP_STAGES",
				"1:20s,8:30s,32:30s,64:45s,0:15s",
			),
			gracefulRampDown: durationEnv(
				"ASTER_BENCH_MIXED_RAMP_GRACEFUL_RAMP_DOWN",
				"10s",
			),
		},
	},
	thresholds: {
		http_req_failed: ["rate<0.02"],
		aster_mixed_ramp_flow_duration: [
			`p(95)<${intEnv("ASTER_BENCH_MIXED_RAMP_FLOW_P95_MS", 2500)}`,
		],
	},
};

export function setup() {
	const session = login();
	const listFolderId = resolveRootFolderId(session, listFolderName(listSize));
	const downloadFolderId = resolveRootFolderId(session, benchConfig.downloadFolder);
	if (!listFolderId || !downloadFolderId) {
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

	const uploadFolderId = ensureRootFolder(
		session,
		__ENV.ASTER_BENCH_MIXED_RAMP_UPLOAD_FOLDER || "bench-upload-ramp",
	);
	return {
		listFolderId,
		downloadFileId: downloadFixture.id,
		downloadFileSize: Number(downloadFixture.size),
		uploadFolderId,
	};
}

export default function (data) {
	if (!state) {
		state = {
			...data,
			session: login(),
		};
	}

	state.session = maybeRefreshSession(state.session);
	const startedAt = Date.now();
	const op = (exec.vu.idInTest + __ITER) % 5;

	switch (op) {
		case 0:
			listFolder(state.session, state.listFolderId, {
				folder_limit: 0,
				file_limit: listLimit,
				sort_by: "name",
				sort_order: "asc",
			});
			mixedRampOperations.add(1, { operation: "list" });
			break;
		case 1:
			search(state.session, {
				q: benchConfig.searchTerm,
				folder_id: state.listFolderId,
				limit: searchLimit,
			});
			mixedRampOperations.add(1, { operation: "search" });
			break;
		case 2:
			downloadFile(state.session, state.downloadFileId);
			mixedRampBytes.add(state.downloadFileSize, { direction: "download" });
			mixedRampOperations.add(1, { operation: "download" });
			break;
		case 3:
			uploadDirect(state.session, {
				filename: uniqueName("ramp-upload", "bin"),
				content: uploadPayload,
				mimeType: "application/octet-stream",
				folderId: state.uploadFolderId,
			});
			mixedRampBytes.add(uploadBytes, { direction: "upload" });
			mixedRampOperations.add(1, { operation: "upload" });
			break;
		default:
			state.session = refreshSession(state.session);
			mixedRampOperations.add(1, { operation: "refresh" });
			break;
	}

	mixedRampDuration.add(Date.now() - startedAt);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("mixed-ramp", [
	"aster_mixed_ramp_flow_duration",
	"aster_mixed_ramp_bytes",
	"aster_mixed_ramp_operations",
]);
