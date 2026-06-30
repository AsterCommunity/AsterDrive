export function env(name, fallback) {
	const value = __ENV[name];
	return value === undefined || value === "" ? fallback : value;
}

export function intEnv(name, fallback) {
	const parsed = Number.parseInt(env(name, String(fallback)), 10);
	if (Number.isNaN(parsed)) {
		throw new Error(`invalid integer env ${name}`);
	}

	return parsed;
}

export function durationEnv(name, fallback) {
	return env(name, fallback);
}

export function stagesEnv(name, fallback) {
	return env(name, fallback)
		.split(",")
		.map((entry) => entry.trim())
		.filter(Boolean)
		.map((entry) => {
			const [targetRaw, durationRaw] = entry.split(":").map((part) => part.trim());
			const target = Number.parseInt(targetRaw, 10);
			if (Number.isNaN(target) || target < 0 || !durationRaw) {
				throw new Error(`invalid stage env ${name}: ${entry}`);
			}

			return {
				target,
				duration: durationRaw,
			};
		});
}

export const benchSummaryTrendStats = [
	"avg",
	"min",
	"med",
	"p(90)",
	"p(95)",
	"p(99)",
	"p(99.9)",
	"max",
];

function stripTrailingSlash(value) {
	return value.endsWith("/") ? value.slice(0, -1) : value;
}

export const benchConfig = {
	baseUrl: stripTrailingSlash(
		env("ASTER_BENCH_BASE_URL", "http://127.0.0.1:3000"),
	),
	username: env("ASTER_BENCH_USERNAME", "bench_user"),
	password: env("ASTER_BENCH_PASSWORD", "bench-pass-1234"),
	searchTerm: env("ASTER_BENCH_SEARCH_TERM", "needle"),
	webdavUsername: env("ASTER_BENCH_WEBDAV_USERNAME", "bench_webdav"),
	webdavPassword: env(
		"ASTER_BENCH_WEBDAV_PASSWORD",
		"bench_webdav_pass123",
	),
	webdavPrefix: env("ASTER_BENCH_WEBDAV_PREFIX", "/webdav"),
	downloadFolder: env("ASTER_BENCH_DOWNLOAD_FOLDER", "bench-download"),
	downloadFile: env("ASTER_BENCH_DOWNLOAD_FILE", "payload-5mb.bin"),
	directUploadFolder: env(
		"ASTER_BENCH_DIRECT_UPLOAD_FOLDER",
		"bench-upload-direct",
	),
	chunkedUploadFolder: env(
		"ASTER_BENCH_CHUNKED_UPLOAD_FOLDER",
		"bench-upload-chunked",
	),
	batchTargetFolder: env(
		"ASTER_BENCH_BATCH_TARGET_FOLDER",
		"bench-batch-target",
	),
	thinkTimeMs: intEnv("ASTER_BENCH_THINK_TIME_MS", 0),
	listFolderPrefix: env("ASTER_BENCH_LIST_FOLDER_PREFIX", "bench-list"),
};

export function listFolderName(size) {
	return `${benchConfig.listFolderPrefix}-${size}`;
}
