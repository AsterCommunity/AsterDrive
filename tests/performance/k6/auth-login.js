import { sleep } from "k6";
import { Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
} from "./lib/config.js";
import { login } from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const loginDuration = new Trend("aster_auth_login_duration", true);

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_AUTH_LOGIN_VUS", 10),
	duration: durationEnv("ASTER_BENCH_AUTH_LOGIN_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_auth_login_duration: [
			`p(95)<${intEnv("ASTER_BENCH_AUTH_LOGIN_P95_MS", 500)}`,
		],
	},
};

export default function () {
	const session = login();
	loginDuration.add(session.lastDuration);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("auth-login", [
	"aster_auth_login_duration",
]);
