import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { gzipSync } from "node:zlib";

const DIST_DIR = path.resolve("dist");
const ASSET_PREFIX = "assets/";
const ROUTER_SOURCE = path.resolve("src/router/index.tsx");
const WARMUP_SOURCE = path.resolve("src/lib/pwaWarmupLoaders.ts");

const BUDGETS = {
	entryRawBytes: 25 * 1024,
	entryGzipBytes: 8 * 1024,
	loginRawBytes: 65 * 1024,
	loginGzipBytes: 20 * 1024,
	precacheEntries: 12,
	precacheBytes: 700 * 1024,
};

const PRECACHE_FORBIDDEN = [
	/Admin/,
	/FileBrowser/,
	/MusicPlayer/,
	/PdfPreview/,
	/WorkspaceOutlet/,
	/pwaWarmup/,
	/vendor-react-icons-/,
	/pdf\.worker/,
	/^pdfjs\//,
	/^static\/preview-apps\//,
	/^static\/external-auth\//,
];

const STARTUP_FORBIDDEN = [
	/Admin/,
	/FileBrowser/,
	/MusicPlayer/,
	/PdfPreview/,
	/WorkspaceOutlet/,
	/pwaWarmup/,
	/settings-common/,
	/preview-apps/,
	/archive-preview/,
	/office-preview/,
	/wopi/i,
];

const LOGIN_FORBIDDEN = [
	/Admin/,
	/FileBrowser/,
	/MusicPlayer/,
	/PdfPreview/,
	/WorkspaceOutlet/,
	/pwaWarmup/,
	/settings-common/,
	/preview-apps/,
	/archive-preview/,
	/office-preview/,
	/wopi/i,
];

function fail(message) {
	throw new Error(`[startup-audit] ${message}`);
}

function readDistFile(relativePath) {
	const filePath = path.join(DIST_DIR, relativePath);
	if (!existsSync(filePath)) fail(`missing dist file: ${relativePath}`);
	return readFileSync(filePath, "utf8");
}

function readSourceFile(filePath) {
	if (!existsSync(filePath)) fail(`missing source file: ${filePath}`);
	return readFileSync(filePath, "utf8");
}

function fileSize(relativePath) {
	return statSync(path.join(DIST_DIR, relativePath)).size;
}

function gzipSize(relativePath) {
	return gzipSync(readFileSync(path.join(DIST_DIR, relativePath))).length;
}

function formatBytes(bytes) {
	return `${(bytes / 1024).toFixed(2)} KiB`;
}

function assertBudget(label, actual, max) {
	if (actual > max) {
		fail(`${label} is ${formatBytes(actual)}, over budget ${formatBytes(max)}`);
	}
}

function assertCountBudget(label, actual, max) {
	if (actual > max) {
		fail(`${label} is ${actual}, over budget ${max}`);
	}
}

function extractEntryScript() {
	const html = readDistFile("index.html");
	const match = html.match(/<script[^>]+type="module"[^>]+src="([^"]+)"/);
	if (!match) fail("could not find module entry script in index.html");
	return normalizeAssetPath(match[1]);
}

function normalizeAssetPath(value, fromDir = "") {
	const withoutQuery = value.split("?")[0];
	const normalized = path.posix.normalize(
		withoutQuery.startsWith("/")
			? withoutQuery.slice(1)
			: path.posix.join(fromDir, withoutQuery),
	);
	if (normalized.startsWith("..")) {
		fail(`asset path escaped dist: ${value}`);
	}
	return normalized;
}

function extractStaticImports(relativePath) {
	const source = readDistFile(relativePath);
	const dir = path.posix.dirname(relativePath);
	const imports = new Set();
	const patterns = [
		/import\s+(?:[^"'()]+?\s+from\s*)?["']([^"']+)["']/g,
		/export\s+[^"'()]+?\s+from\s*["']([^"']+)["']/g,
	];

	for (const pattern of patterns) {
		for (const match of source.matchAll(pattern)) {
			const specifier = match[1];
			if (!specifier.startsWith(".")) continue;
			imports.add(normalizeAssetPath(specifier, dir));
		}
	}

	return imports;
}

function collectStaticGraph(entryPath) {
	const seen = new Set();
	const queue = [entryPath];

	while (queue.length > 0) {
		const current = queue.shift();
		if (!current || seen.has(current)) continue;
		seen.add(current);

		for (const imported of extractStaticImports(current)) {
			if (!seen.has(imported)) queue.push(imported);
		}
	}

	return seen;
}

function findAsset(prefix) {
	const entryHtml = readDistFile("index.html");
	const entryNames = [
		...entryHtml.matchAll(/(?:src|href)="\/?(assets\/[^"]+)"/g),
	]
		.map((match) => match[1])
		.filter((name) => name.startsWith(`${ASSET_PREFIX}${prefix}`));
	if (entryNames.length > 0) return entryNames[0];

	const assetDir = path.join(DIST_DIR, ASSET_PREFIX);
	const candidates = [];
	for (const name of readdirSyncCompat(assetDir)) {
		if (name.startsWith(prefix) && name.endsWith(".js")) {
			candidates.push(`${ASSET_PREFIX}${name}`);
		}
	}
	if (candidates.length === 0)
		fail(`could not find asset with prefix: ${prefix}`);
	if (candidates.length > 1) {
		fail(
			`multiple assets found for prefix ${prefix}: ${candidates.join(", ")}`,
		);
	}
	return candidates[0];
}

function readdirSyncCompat(dir) {
	if (!existsSync(dir)) fail(`missing asset directory: ${dir}`);
	return readdirSync(dir);
}

function extractPrecacheEntries() {
	const sw = readDistFile("sw.js");
	const entries = [];
	for (const match of sw.matchAll(/(?:"url"|url)\s*:\s*"([^"]+)"/g)) {
		entries.push(normalizeAssetPath(match[1]));
	}
	if (entries.length === 0) fail("could not find precache entries in sw.js");
	return entries;
}

function assertNoForbidden(label, entries, patterns) {
	const violations = [...entries].filter((entry) =>
		patterns.some((pattern) => pattern.test(entry)),
	);
	if (violations.length > 0) {
		fail(`${label} contains forbidden assets: ${violations.join(", ")}`);
	}
}

function auditPrecache() {
	const entries = extractPrecacheEntries();
	const totalBytes = entries.reduce((sum, entry) => sum + fileSize(entry), 0);

	assertCountBudget(
		"service worker precache entry count",
		entries.length,
		BUDGETS.precacheEntries,
	);
	assertBudget(
		"service worker precache size",
		totalBytes,
		BUDGETS.precacheBytes,
	);
	assertNoForbidden("service worker precache", entries, PRECACHE_FORBIDDEN);

	return { entries, totalBytes };
}

function auditStartupGraphs() {
	const entry = extractEntryScript();
	const login = findAsset("LoginPage-");
	const entryGraph = collectStaticGraph(entry);
	const loginGraph = new Set([...entryGraph, ...collectStaticGraph(login)]);

	assertNoForbidden("startup graph", entryGraph, STARTUP_FORBIDDEN);
	assertNoForbidden("login graph", loginGraph, LOGIN_FORBIDDEN);

	assertBudget("entry chunk raw size", fileSize(entry), BUDGETS.entryRawBytes);
	assertBudget(
		"entry chunk gzip size",
		gzipSize(entry),
		BUDGETS.entryGzipBytes,
	);
	assertBudget("login chunk raw size", fileSize(login), BUDGETS.loginRawBytes);
	assertBudget(
		"login chunk gzip size",
		gzipSize(login),
		BUDGETS.loginGzipBytes,
	);

	return { entry, entryGraph, login, loginGraph };
}

function auditWarmupCoverage() {
	const routerSource = readSourceFile(ROUTER_SOURCE);
	const warmupSource = readSourceFile(WARMUP_SOURCE);
	const routeImports = new Set();
	const warmupImports = new Set();

	for (const match of routerSource.matchAll(/import\("@\/pages\/([^"]+)"\)/g)) {
		routeImports.add(match[1]);
	}
	for (const match of warmupSource.matchAll(/import\("@\/pages\/([^"]+)"\)/g)) {
		warmupImports.add(match[1]);
	}

	const missing = [...routeImports].filter((page) => !warmupImports.has(page));
	if (missing.length > 0) {
		fail(`warmup is missing lazy route pages: ${missing.join(", ")}`);
	}

	return {
		routeCount: routeImports.size,
		warmupRouteCount: warmupImports.size,
	};
}

function main() {
	if (!existsSync(DIST_DIR))
		fail("dist directory does not exist; run build first");

	const precache = auditPrecache();
	const startup = auditStartupGraphs();
	const warmup = auditWarmupCoverage();

	console.log("[startup-audit] ok");
	console.log(
		`[startup-audit] precache: ${precache.entries.length} entries, ${formatBytes(
			precache.totalBytes,
		)}`,
	);
	console.log(
		`[startup-audit] entry: ${startup.entry} (${formatBytes(
			fileSize(startup.entry),
		)} raw, ${formatBytes(gzipSize(startup.entry))} gzip), static graph ${
			startup.entryGraph.size
		} files`,
	);
	console.log(
		`[startup-audit] login: ${startup.login} (${formatBytes(
			fileSize(startup.login),
		)} raw, ${formatBytes(gzipSize(startup.login))} gzip), graph ${
			startup.loginGraph.size
		} files`,
	);
	console.log(
		`[startup-audit] warmup coverage: ${warmup.routeCount}/${warmup.warmupRouteCount} route pages covered`,
	);
}

main();
