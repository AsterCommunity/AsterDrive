import { runtimeFlags } from "@/config/runtime";
import { logger } from "@/lib/logger";
import {
	adminRouteWarmupLoaders,
	filePreviewWarmupLoaders,
	loginSuccessPathWarmupLoaders,
	userFeatureWarmupLoaders,
	userRouteWarmupLoaders,
	type WarmupLoaderEntry,
} from "@/lib/pwaWarmupLoaders";

const IDLE_TIMEOUT_MS = 3000;
const CHUNK_DELAY_MS = 1200;
const RUNTIME_CHUNK_CACHE_NAME = "asset-chunks";

function logWarmup(message: string, extra?: unknown) {
	if (extra === undefined) {
		logger.debug(`[pwa-warmup] ${message}`);
		return;
	}
	logger.debug(`[pwa-warmup] ${message}`, extra);
}

function scheduleIdle(task: () => void) {
	if (typeof window === "undefined") return;

	if ("requestIdleCallback" in window) {
		window.requestIdleCallback(task, { timeout: IDLE_TIMEOUT_MS });
		return;
	}

	globalThis.setTimeout(task, CHUNK_DELAY_MS);
}

function readResourceEntries() {
	if (
		typeof performance === "undefined" ||
		typeof performance.getEntriesByType !== "function"
	) {
		return [];
	}

	return performance
		.getEntriesByType("resource")
		.filter(
			(entry): entry is PerformanceResourceTiming =>
				entry instanceof PerformanceResourceTiming,
		);
}

async function logCacheHit(
	entry: WarmupLoaderEntry,
	resourceCountBefore: number,
) {
	if (!runtimeFlags.isDev || typeof caches === "undefined") return;

	const resources = readResourceEntries();
	const newResources = resources.slice(resourceCountBefore);
	const scriptResource = [...newResources].reverse().find((resource) => {
		try {
			const url = new URL(resource.name);
			return (
				url.pathname.startsWith("/src/") || url.pathname.startsWith("/assets/")
			);
		} catch {
			return false;
		}
	});

	const scriptUrl = scriptResource?.name;
	const transferSize = scriptResource?.transferSize ?? null;
	const delivery =
		transferSize === 0
			? "cache-or-memory"
			: transferSize != null
				? "network"
				: "unknown";

	if (!scriptUrl) {
		logWarmup(`cache probe missed ${entry.label}`, {
			key: entry.key,
			delivery,
		});
		return;
	}

	try {
		const cache = await caches.open(RUNTIME_CHUNK_CACHE_NAME);
		const cacheMatch = await cache.match(scriptUrl, { ignoreSearch: true });
		logWarmup(`cache probe ${entry.label}`, {
			key: entry.key,
			url: scriptUrl,
			delivery,
			cachedInRuntime: cacheMatch != null,
			transferSize,
		});
		return;
	} catch (error) {
		logWarmup(`cache probe error ${entry.label}`, {
			key: entry.key,
			url: scriptUrl,
			error,
		});
		return;
	}
}

function warmSequentially(loaders: WarmupLoaderEntry[]) {
	let index = 0;

	const runNext = () => {
		const loader = loaders[index];
		if (!loader) {
			logWarmup("queue completed");
			return;
		}

		index += 1;
		const resourceCountBefore = readResourceEntries().length;
		logWarmup(`loading ${loader.label}`, {
			key: loader.key,
			index,
			total: loaders.length,
		});
		void loader
			.load()
			.then(
				async () => {
					logWarmup(`loaded ${loader.label}`, { key: loader.key });
					await logCacheHit(loader, resourceCountBefore);
				},
				(error: unknown) => {
					logWarmup(`failed ${loader.label}`, { key: loader.key, error });
				},
			)
			.finally(() => {
				scheduleIdle(runNext);
			});
	};

	scheduleIdle(runNext);
}

let warmedUserRoutes = false;
let warmedAdminRoutes = false;
let warmedLoginSuccessPath = false;
let warmedPreviewEngines = false;

export function warmupLoginSuccessPath() {
	if (typeof window === "undefined") return;
	if (warmedLoginSuccessPath) {
		logWarmup("skip duplicate login success path warmup");
		return;
	}

	warmedLoginSuccessPath = true;
	logWarmup(
		"start login success path warmup",
		loginSuccessPathWarmupLoaders.map((loader) => loader.key),
	);
	warmSequentially(loginSuccessPathWarmupLoaders);
}

export function warmupRouteChunks(role: "user" | "admin") {
	if (typeof window === "undefined") return;

	const routeLoaders = (() => {
		if (role === "user") {
			if (warmedUserRoutes) {
				logWarmup("skip duplicate user warmup");
				return null;
			}
			warmedUserRoutes = true;
			return userRouteWarmupLoaders;
		}

		if (warmedAdminRoutes) {
			logWarmup("skip duplicate admin warmup");
			return null;
		}

		const loaders = warmedUserRoutes
			? adminRouteWarmupLoaders
			: [...userRouteWarmupLoaders, ...adminRouteWarmupLoaders];
		warmedUserRoutes = true;
		warmedAdminRoutes = true;
		return loaders;
	})();
	if (routeLoaders == null) return;

	logWarmup(
		`start ${role} warmup`,
		routeLoaders.map((loader) => loader.key),
	);
	if (routeLoaders.length === 0) {
		logWarmup(`skip ${role} route warmup because no route queue is configured`);
		return;
	}
	warmSequentially(routeLoaders);
}

export function warmupPreviewEngines() {
	if (typeof window === "undefined") return;
	if (warmedPreviewEngines) {
		logWarmup("skip duplicate preview engines warmup");
		return;
	}

	warmedPreviewEngines = true;
	const loaders = [
		...userFeatureWarmupLoaders.filter(
			(loader) =>
				loader.key === "feature:file-preview" ||
				loader.key === "feature:language-icons",
		),
		...filePreviewWarmupLoaders,
	];

	logWarmup(
		"start preview engines warmup",
		loaders.map((loader) => loader.key),
	);
	warmSequentially(loaders);
}
