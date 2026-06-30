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
	login,
	maybeRefreshSession,
	resolveRootFolderId,
	search,
} from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const searchDuration = new Trend("aster_search_duration", true);
const datasetSize = intEnv("ASTER_BENCH_SEARCH_DATASET_SIZE", 10000);
let state;

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_SEARCH_VUS", 8),
	duration: durationEnv("ASTER_BENCH_SEARCH_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_search_duration: [
			`p(95)<${intEnv("ASTER_BENCH_SEARCH_P95_MS", 400)}`,
		],
	},
};

export function setup() {
	const session = login();
	const folderId = resolveRootFolderId(session, listFolderName(datasetSize));
	if (!folderId) {
		throw new Error(
			`missing seeded folder ${listFolderName(datasetSize)}; run bun tests/performance/seed.mjs first`,
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
	const { response } = search(state.session, {
		q: benchConfig.searchTerm,
		folder_id: state.folderId,
		limit: intEnv("ASTER_BENCH_SEARCH_LIMIT", 50),
	});
	searchDuration.add(response.timings.duration);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("search", [
	"aster_search_duration",
]);
