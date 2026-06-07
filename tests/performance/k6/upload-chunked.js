import { sleep } from "k6";
import { Counter, Trend } from "k6/metrics";

import {
	completeUpload,
	ensureRootFolder,
	initChunkedUpload,
	login,
	maybeRefreshSession,
	uniqueName,
	uploadChunk,
} from "./lib/client.js";
import { benchConfig, durationEnv, intEnv } from "./lib/config.js";
import { createSummary } from "./lib/summary.js";

const flowDuration = new Trend("aster_upload_chunked_flow_duration", true);
const initDuration = new Trend("aster_upload_chunked_init_duration", true);
const chunkDuration = new Trend("aster_upload_chunked_chunk_duration", true);
const completeDuration = new Trend(
	"aster_upload_chunked_complete_duration",
	true,
);
const clientGapDuration = new Trend(
	"aster_upload_chunked_client_gap_duration",
	true,
);
const uploadTransferredBytes = new Counter("aster_upload_chunked_bytes");
const totalBytes = intEnv("ASTER_BENCH_CHUNKED_TOTAL_BYTES", 10 * 1024 * 1024);
let state;

export const options = {
	vus: intEnv("ASTER_BENCH_CHUNKED_UPLOAD_VUS", 3),
	duration: durationEnv("ASTER_BENCH_CHUNKED_UPLOAD_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_upload_chunked_flow_duration: [
			`p(95)<${intEnv("ASTER_BENCH_CHUNKED_UPLOAD_P95_MS", 4000)}`,
		],
	},
};

function makeChunk(length, ordinal) {
	const seed = String.fromCharCode(65 + (ordinal % 26));
	return seed.repeat(length);
}

export function setup() {
	const session = login();
	const folderId = ensureRootFolder(session, benchConfig.chunkedUploadFolder);
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
	const startedAt = Date.now();
	const { response: initResponse, body } = initChunkedUpload(state.session, {
		filename: uniqueName("chunked-upload", "bin"),
		totalSize: totalBytes,
		folderId: state.folderId,
	});
	initDuration.add(initResponse.timings.duration);
	const sessionData = body.data;
	if (sessionData.mode !== "chunked") {
		throw new Error(
			`expected chunked upload mode, got ${sessionData.mode}; increase ASTER_BENCH_CHUNKED_TOTAL_BYTES`,
		);
	}

	let chunkDurationTotal = 0;
	for (let index = 0; index < sessionData.total_chunks; index += 1) {
		const remaining = totalBytes - sessionData.chunk_size * index;
		const chunkSize =
			index === sessionData.total_chunks - 1
				? remaining
				: sessionData.chunk_size;
		const { response } = uploadChunk(
			state.session,
			sessionData.upload_id,
			index,
			makeChunk(chunkSize, index),
		);
		chunkDuration.add(response.timings.duration);
		chunkDurationTotal += response.timings.duration;
	}
	const { response: completeResponse } = completeUpload(
		state.session,
		sessionData.upload_id,
	);
	completeDuration.add(completeResponse.timings.duration);
	const flowElapsed = Date.now() - startedAt;
	flowDuration.add(flowElapsed);
	clientGapDuration.add(
		Math.max(
			0,
			flowElapsed -
				initResponse.timings.duration -
				chunkDurationTotal -
				completeResponse.timings.duration,
		),
	);
	uploadTransferredBytes.add(totalBytes);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("upload-chunked", [
	"aster_upload_chunked_flow_duration",
	"aster_upload_chunked_init_duration",
	"aster_upload_chunked_chunk_duration",
	"aster_upload_chunked_complete_duration",
	"aster_upload_chunked_client_gap_duration",
	"aster_upload_chunked_bytes",
]);
