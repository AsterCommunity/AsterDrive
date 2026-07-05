import { sleep } from "k6";
import { Counter, Trend } from "k6/metrics";

import {
	benchConfig,
	benchSummaryTrendStats,
	durationEnv,
	intEnv,
} from "./lib/config.js";
import { listWebdavAccounts, login, webdavRequest } from "./lib/client.js";
import { createSummary } from "./lib/summary.js";

const propfindDuration = new Trend("aster_webdav_propfind_duration", true);
const propfindResponseBytes = new Counter("aster_webdav_propfind_response_bytes");

const propfindBody = `<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:getcontentlength/>
    <D:getcontenttype/>
    <D:getetag/>
    <D:getlastmodified/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>`;

export const options = {
	summaryTrendStats: benchSummaryTrendStats,
	vus: intEnv("ASTER_BENCH_WEBDAV_PROPFIND_VUS", 4),
	duration: durationEnv("ASTER_BENCH_WEBDAV_PROPFIND_DURATION", "30s"),
	thresholds: {
		http_req_failed: ["rate<0.01"],
		aster_webdav_propfind_duration: [
			`p(95)<${intEnv("ASTER_BENCH_WEBDAV_PROPFIND_P95_MS", 1500)}`,
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
	const response = webdavRequest(
		"PROPFIND",
		benchConfig.webdavListFolder,
		propfindBody,
		{
			headers: {
				"Content-Type": "application/xml; charset=utf-8",
				Depth: "1",
			},
		},
	);
	if (response.status !== 207) {
		throw new Error(`webdav PROPFIND Depth:1 failed: ${response.status}`);
	}

	propfindDuration.add(response.timings.duration);
	propfindResponseBytes.add(response.body?.length ?? 0);

	if (benchConfig.thinkTimeMs > 0) {
		sleep(benchConfig.thinkTimeMs / 1000);
	}
}

export const handleSummary = createSummary("webdav-propfind-large", [
	"aster_webdav_propfind_duration",
	"aster_webdav_propfind_response_bytes",
]);
