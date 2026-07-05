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

export function boolEnv(name, fallback) {
	const value = env(name, fallback ? "true" : "false").toLowerCase();
	if (["1", "true", "yes", "on"].includes(value)) {
		return true;
	}
	if (["0", "false", "no", "off"].includes(value)) {
		return false;
	}

	throw new Error(`invalid boolean env ${name}`);
}

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
	webdavListFolder: env("ASTER_BENCH_WEBDAV_LIST_FOLDER", "bench-webdav-list"),
	webdavListSize: intEnv("ASTER_BENCH_WEBDAV_LIST_SIZE", 1000),
	webdavRangeFile: env(
		"ASTER_BENCH_WEBDAV_RANGE_FILE",
		"webdav-range-5mb.bin",
	),
	webdavRangeFileBytes: intEnv(
		"ASTER_BENCH_WEBDAV_RANGE_FILE_BYTES",
		5 * 1024 * 1024,
	),
	downloadFolder: env("ASTER_BENCH_DOWNLOAD_FOLDER", "bench-download"),
	downloadFile: env("ASTER_BENCH_DOWNLOAD_FILE", "payload-5mb.bin"),
	rangeBytes: intEnv("ASTER_BENCH_RANGE_BYTES", 256 * 1024),
	rangeStrideBytes: intEnv("ASTER_BENCH_RANGE_STRIDE_BYTES", 1024 * 1024),
	directUploadFolder: env(
		"ASTER_BENCH_DIRECT_UPLOAD_FOLDER",
		"bench-upload-direct",
	),
	chunkedUploadFolder: env(
		"ASTER_BENCH_CHUNKED_UPLOAD_FOLDER",
		"bench-upload-chunked",
	),
	backgroundUploadFolder: env(
		"ASTER_BENCH_BACKGROUND_UPLOAD_FOLDER",
		"bench-background-upload",
	),
	archiveSourceFolder: env(
		"ASTER_BENCH_ARCHIVE_SOURCE_FOLDER",
		"bench-list-10000",
	),
	archiveTargetFolder: env(
		"ASTER_BENCH_ARCHIVE_TARGET_FOLDER",
		"bench-archive-output",
	),
	thumbnailFolder: env("ASTER_BENCH_THUMBNAIL_FOLDER", "bench-thumbnail"),
	thumbnailImageCount: intEnv("ASTER_BENCH_THUMBNAIL_IMAGE_COUNT", 128),
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
