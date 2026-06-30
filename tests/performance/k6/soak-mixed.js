import { Counter, Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
	listFolderName,
} from "./lib/config.js";
import {
	downloadFile,
	ensureRootFolder,
	findFileInFolder,
	login,
	maybeRefreshSession,
	refreshSession,
	resolveRootFolderId,
	search,
	uniqueName,
	uploadDirect,
	listFolder,
} from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const soakDuration = new Trend("aster_soak_flow_duration", true);
const soakOps = new Counter("aster_soak_operations");
let state;

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_SOAK_VUS", 6),
	duration: durationEnv("ASTER_BENCH_SOAK_DURATION", "10m"),
	thresholds: {
		http_req_failed: ["rate<0.02"],
		aster_soak_flow_duration: [
			`p(95)<${intEnv("ASTER_BENCH_SOAK_FLOW_P95_MS", 2500)}`,
		],
	},
};

export function setup() {
	const session = login();
	const listFolderId = resolveRootFolderId(session, listFolderName(10000));
	const downloadFolderId = resolveRootFolderId(session, benchConfig.downloadFolder);
	if (!listFolderId || !downloadFolderId) {
		throw new Error(
			"missing seeded benchmark folders; run bun tests/performance/seed.mjs first",
		);
	}

	const downloadFileId = findFileInFolder(
		session,
		downloadFolderId,
		benchConfig.downloadFile,
	);
	if (!downloadFileId) {
		throw new Error(
			`missing seeded file ${benchConfig.downloadFile}; run bun tests/performance/seed.mjs first`,
		);
	}

	const uploadFolderId = ensureRootFolder(
		session,
		env("ASTER_BENCH_SOAK_UPLOAD_FOLDER", "bench-upload-soak"),
	);
	return {
		listFolderId,
		downloadFileId,
		uploadFolderId,
	};
}

function env(name, fallback) {
	const value = __ENV[name];
	return value === undefined || value === "" ? fallback : value;
}

export default function (data) {
	if (!state) {
		state = {
			...data,
			session: login(),
		};
	}

	const op = __ITER % 5;
	const startedAt = Date.now();
	state.session = maybeRefreshSession(state.session);

	switch (op) {
		case 0:
			listFolder(state.session, state.listFolderId, {
				folder_limit: 0,
				file_limit: 100,
				sort_by: "name",
				sort_order: "asc",
			});
			soakOps.add(1, { operation: "list" });
			break;
		case 1:
			search(state.session, {
				q: benchConfig.searchTerm,
				folder_id: state.listFolderId,
				limit: 50,
			});
			soakOps.add(1, { operation: "search" });
			break;
		case 2:
			downloadFile(state.session, state.downloadFileId);
			soakOps.add(1, { operation: "download" });
			break;
		case 3:
			uploadDirect(state.session, {
				filename: uniqueName("soak-upload", "txt"),
				content: "soak-payload",
				folderId: state.uploadFolderId,
			});
			soakOps.add(1, { operation: "upload" });
			break;
		default:
			state.session = refreshSession(state.session);
			soakOps.add(1, { operation: "refresh" });
			break;
	}

	soakDuration.add(Date.now() - startedAt);
}

export const handleSummary = createSummary("soak-mixed", [
	"aster_soak_flow_duration",
	"aster_soak_operations",
]);
