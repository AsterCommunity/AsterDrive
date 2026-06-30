import { sleep } from "k6";
import { Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
	listFolderName,
} from "./lib/config.js";
import {
	listFolder,
	login,
	maybeRefreshSession,
	resolveRootFolderId,
} from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const listDuration = new Trend("aster_folder_list_duration", true);
const listSize = intEnv("ASTER_BENCH_LIST_SIZE", 1000);
const fileLimit = intEnv("ASTER_BENCH_LIST_FILE_LIMIT", 100);
let state;

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_FOLDER_LIST_VUS", 8),
	duration: durationEnv("ASTER_BENCH_FOLDER_LIST_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_folder_list_duration: [
			`p(95)<${intEnv("ASTER_BENCH_FOLDER_LIST_P95_MS", 400)}`,
		],
	},
};

export function setup() {
	const session = login();
	const folderId = resolveRootFolderId(session, listFolderName(listSize));
	if (!folderId) {
		throw new Error(
			`missing seeded folder ${listFolderName(listSize)}; run bun tests/performance/seed.mjs first`,
		);
	}

	return {
		session,
		folderId,
	};
}

export default function (data) {
	if (!state) {
		state = data;
	}

	state.session = maybeRefreshSession(state.session);
	const { response } = listFolder(state.session, state.folderId, {
		folder_limit: 0,
		file_limit: fileLimit,
		sort_by: "name",
		sort_order: "asc",
	});
	listDuration.add(response.timings.duration);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary(`folder-list-${listSize}`, [
	"aster_folder_list_duration",
]);
