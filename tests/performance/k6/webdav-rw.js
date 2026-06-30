import { sleep } from "k6";
import { Counter, Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
} from "./lib/config.js";
import { listWebdavAccounts, login, uniqueName, webdavRequest } from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const webdavPutDuration = new Trend("aster_webdav_put_duration", true);
const webdavGetDuration = new Trend("aster_webdav_get_duration", true);
const webdavDeleteDuration = new Trend("aster_webdav_delete_duration", true);
const webdavFlowDuration = new Trend("aster_webdav_flow_duration", true);
const webdavPutBytes = new Counter("aster_webdav_put_bytes");
const webdavGetBytes = new Counter("aster_webdav_get_bytes");
const payloadBytes = intEnv("ASTER_BENCH_WEBDAV_PAYLOAD_BYTES", 64 * 1024);
const payload = "W".repeat(payloadBytes);

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_WEBDAV_VUS", 4),
	duration: durationEnv("ASTER_BENCH_WEBDAV_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_webdav_put_duration: [
			`p(95)<${intEnv("ASTER_BENCH_WEBDAV_PUT_P95_MS", 1200)}`,
		],
		aster_webdav_get_duration: [
			`p(95)<${intEnv("ASTER_BENCH_WEBDAV_GET_P95_MS", 1200)}`,
		],
	},
};

export function setup() {
	const session = login();
	const { body } = listWebdavAccounts(session);
	const account = body.data.items.find(
		(item) => item.username === benchConfig.webdavUsername,
	);
	if (!account) {
		throw new Error(
			`missing WebDAV account ${benchConfig.webdavUsername}; run bun tests/performance/seed.mjs first`,
		);
	}

	return null;
}

export default function () {
	const startedAt = Date.now();
	const path = uniqueName("webdav", "txt");
	const putResponse = webdavRequest("PUT", path, payload, {
		headers: {
			"Content-Type": "text/plain",
		},
	});
	if (putResponse.status !== 201 && putResponse.status !== 204) {
		throw new Error(`webdav PUT failed: ${putResponse.status}`);
	}
	webdavPutDuration.add(putResponse.timings.duration);
	webdavPutBytes.add(payloadBytes);

	const getResponse = webdavRequest("GET", path);
	if (getResponse.status !== 200) {
		throw new Error(`webdav GET failed: ${getResponse.status}`);
	}
	webdavGetDuration.add(getResponse.timings.duration);
	webdavGetBytes.add(payloadBytes);

	const deleteResponse = webdavRequest("DELETE", path);
	if (deleteResponse.status !== 204) {
		throw new Error(`webdav DELETE failed: ${deleteResponse.status}`);
	}
	webdavDeleteDuration.add(deleteResponse.timings.duration);
	webdavFlowDuration.add(Date.now() - startedAt);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("webdav-rw", [
	"aster_webdav_put_duration",
	"aster_webdav_get_duration",
	"aster_webdav_delete_duration",
	"aster_webdav_flow_duration",
	"aster_webdav_put_bytes",
	"aster_webdav_get_bytes",
]);
