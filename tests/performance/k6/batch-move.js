import { sleep } from "k6";
import { Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
} from "./lib/config.js";
import {
	batchMove,
	createFolder,
	ensureRootFolder,
	login,
	maybeRefreshSession,
	uniqueName,
	uploadDirect,
} from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const batchDuration = new Trend("aster_batch_move_duration", true);
const fileCount = intEnv("ASTER_BENCH_BATCH_FILE_COUNT", 10);
let state;

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_BATCH_VUS", 3),
	duration: durationEnv("ASTER_BENCH_BATCH_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_batch_move_duration: [
			`p(95)<${intEnv("ASTER_BENCH_BATCH_P95_MS", 2000)}`,
		],
	},
};

export function setup() {
	const session = login();
	const targetFolderId = ensureRootFolder(session, benchConfig.batchTargetFolder);
	return {
		session,
		targetFolderId,
	};
}

export default function (data) {
	if (!state) {
		state = data;
	}

	state.session = maybeRefreshSession(state.session);
	const sourceFolder = createFolder(
		state.session,
		uniqueName("batch-src", "dir"),
		null,
	).body.data;
	const fileIds = [];
	for (let index = 0; index < fileCount; index += 1) {
		const { body } = uploadDirect(state.session, {
			filename: uniqueName(`batch-file-${index}`, "txt"),
			content: `batch-${index}`,
			folderId: sourceFolder.id,
		});
		fileIds.push(body.data.id);
	}

	const { response } = batchMove(
		state.session,
		fileIds,
		[],
		state.targetFolderId,
	);
	batchDuration.add(response.timings.duration);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("batch-move", [
	"aster_batch_move_duration",
]);
