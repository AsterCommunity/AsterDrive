import { sleep } from "k6";
import { Counter, Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
} from "./lib/config.js";
import {
	ensureRootFolder,
	login,
	maybeRefreshSession,
	uniqueName,
	uploadViaSession,
} from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const uploadDuration = new Trend("aster_upload_stream_duration", true);
const uploadTransferredBytes = new Counter("aster_upload_stream_bytes");
const uploadBytes = intEnv("ASTER_BENCH_STREAM_UPLOAD_BYTES", 1024 * 1024);
const payload = "U".repeat(uploadBytes);
let state;

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_STREAM_UPLOAD_VUS", 4),
	duration: durationEnv("ASTER_BENCH_STREAM_UPLOAD_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_upload_stream_duration: [
			`p(95)<${intEnv("ASTER_BENCH_STREAM_UPLOAD_P95_MS", 1500)}`,
		],
	},
};

export function setup() {
	const session = login();
	const folderId = ensureRootFolder(session, benchConfig.streamUploadFolder);
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
	const { response } = uploadViaSession(state.session, {
		filename: uniqueName("direct-upload", "bin"),
		content: payload,
		mimeType: "application/octet-stream",
		folderId: state.folderId,
	});
	uploadDuration.add(response.timings.duration);
	uploadTransferredBytes.add(uploadBytes);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("upload-stream", [
	"aster_upload_stream_duration",
	"aster_upload_stream_bytes",
]);
