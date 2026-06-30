import { sleep } from "k6";
import { Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
} from "./lib/config.js";
import { login, refreshSession } from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const refreshDuration = new Trend("aster_auth_refresh_duration", true);

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_AUTH_REFRESH_VUS", 10),
	duration: durationEnv("ASTER_BENCH_AUTH_REFRESH_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_auth_refresh_duration: [
			`p(95)<${intEnv("ASTER_BENCH_AUTH_REFRESH_P95_MS", 500)}`,
		],
	},
};

export default function () {
	const session = login();
	const refreshed = refreshSession(session);
	refreshDuration.add(refreshed.lastDuration);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("auth-refresh", [
	"aster_auth_refresh_duration",
]);
