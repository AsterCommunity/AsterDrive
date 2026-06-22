import { useEffect, useState } from "react";
import { config } from "@/config/app";
import { shouldSendResourceCredentials } from "@/lib/apiUrl";
import { logger } from "@/lib/logger";
import {
	type ResourcePath,
	resourceCacheKey,
	resourceCanonicalEtag,
	resourceRequestPath,
} from "@/lib/resourceRequest";
import { api } from "@/services/http";

type BlobFetchLane = "default" | "preview" | "thumbnail";

interface BlobCacheEntry {
	blob?: Blob;
	lane?: BlobFetchLane;
	missing?: boolean;
	needsRefresh?: boolean;
	objectUrl?: string;
	etag?: string | null;
	promise?: Promise<string>;
	refCount: number;
	refreshTimer?: ReturnType<typeof setTimeout>;
	revokeTimer?: ReturnType<typeof setTimeout>;
}

interface PersistedBlobCacheEntry {
	blob: Blob;
	etag: string | null;
}

interface FetchBlobUrlOptions {
	lane: BlobFetchLane;
	logErrors?: boolean;
	notifyOnChange?: boolean;
	owner: BlobCacheEntry;
	cacheKey: string;
	canonicalEtag?: string | null;
	previousBlob?: Blob;
	previousEtag: string | null;
	previousObjectUrl?: string;
	requestPath: string;
}

interface BlobUrlOptions {
	lane?: BlobFetchLane;
}

const BLOB_URL_REVOKE_DELAY = 30_000;
const BLOB_FETCH_LIMITS: Record<BlobFetchLane, number> = {
	default: 6,
	preview: 3,
	thumbnail: 1,
};
const PERSISTED_THUMBNAIL_CACHE_NAME = "aster-thumbnail-blobs-v1";
const PERSISTED_THUMBNAIL_CACHE_PREFIX = "/__asterdrive_thumbnail_cache__/";
const THUMBNAIL_CACHE_NAMESPACE_KEY = "aster-thumbnail-cache-namespace";
const blobUrlCache = new Map<string, BlobCacheEntry>();
const blobUrlListeners = new Map<string, Set<() => void>>();
const pendingBlobFetches: Record<BlobFetchLane, Array<() => void>> = {
	default: [],
	preview: [],
	thumbnail: [],
};
const activeBlobFetches: Record<BlobFetchLane, number> = {
	default: 0,
	preview: 0,
	thumbnail: 0,
};

function shouldPersistBlobInSession(lane: BlobFetchLane) {
	return lane === "thumbnail";
}

function shouldPersistBlobOnDisk(lane: BlobFetchLane) {
	return lane === "thumbnail";
}

function isMissingThumbnailResponse(status: number, lane: BlobFetchLane) {
	return lane === "thumbnail" && status === 404;
}

function cacheStorageAvailable() {
	return (
		typeof globalThis.caches === "object" &&
		globalThis.caches !== null &&
		typeof globalThis.caches.open === "function"
	);
}

function currentPersistedThumbnailNamespace() {
	try {
		const storage = globalThis.sessionStorage;
		if (!storage) return null;
		const existing = storage?.getItem(THUMBNAIL_CACHE_NAMESPACE_KEY);
		if (existing) return existing;

		const next =
			typeof globalThis.crypto?.randomUUID === "function"
				? globalThis.crypto.randomUUID()
				: `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
		void cleanupStalePersistedThumbnailNamespaces(next);
		storage?.setItem(THUMBNAIL_CACHE_NAMESPACE_KEY, next);
		return next;
	} catch {
		// Ignore storage/parse errors and disable disk persistence for this request.
	}
	return null;
}

function persistentThumbnailRequest(path: string) {
	const namespace = currentPersistedThumbnailNamespace();
	if (!namespace) return null;
	const origin = globalThis.location?.origin ?? "http://localhost";
	const cacheKey = `${namespace}|${config.apiBaseUrl}|${path}`;
	return new Request(
		`${origin}${PERSISTED_THUMBNAIL_CACHE_PREFIX}${encodeURIComponent(cacheKey)}`,
	);
}

async function cleanupStalePersistedThumbnailNamespaces(namespace: string) {
	if (!cacheStorageAvailable()) return;
	try {
		const cache = await globalThis.caches.open(PERSISTED_THUMBNAIL_CACHE_NAME);
		const requests = await cache.keys();
		await Promise.all(
			requests.map(async (request) => {
				const markerIndex = request.url.indexOf(
					PERSISTED_THUMBNAIL_CACHE_PREFIX,
				);
				if (markerIndex < 0) return;
				const encodedKey = request.url.slice(
					markerIndex + PERSISTED_THUMBNAIL_CACHE_PREFIX.length,
				);
				const cacheKey = decodeURIComponent(encodedKey);
				const requestNamespace = cacheKey.split("|", 1)[0];
				if (requestNamespace && requestNamespace !== namespace) {
					await cache.delete(request);
				}
			}),
		);
	} catch (error) {
		logger.warn("thumbnail cache namespace cleanup failed", error);
	}
}

async function openPersistedThumbnailCache() {
	if (!cacheStorageAvailable()) return null;
	try {
		return await globalThis.caches.open(PERSISTED_THUMBNAIL_CACHE_NAME);
	} catch (error) {
		logger.warn("thumbnail cache open failed", error);
		return null;
	}
}

async function readPersistedThumbnail(
	path: string,
): Promise<PersistedBlobCacheEntry | null> {
	const cache = await openPersistedThumbnailCache();
	if (!cache) return null;

	try {
		const request = persistentThumbnailRequest(path);
		if (!request) return null;
		const response = await cache.match(request);
		if (!response?.ok) return null;
		return {
			blob: await response.blob(),
			etag: response.headers.get("ETag"),
		};
	} catch (error) {
		logger.warn("thumbnail cache read failed", path, error);
		return null;
	}
}

async function writePersistedThumbnail(
	path: string,
	blob: Blob,
	etag: string | null,
) {
	const cache = await openPersistedThumbnailCache();
	if (!cache) return;

	try {
		const request = persistentThumbnailRequest(path);
		if (!request) return;
		const headers = new Headers({
			"Content-Type": blob.type || "image/webp",
		});
		if (etag) headers.set("ETag", etag);
		await cache.put(
			request,
			new Response(blob, {
				headers,
				status: 200,
			}),
		);
	} catch (error) {
		logger.warn("thumbnail cache write failed", path, error);
	}
}

async function deletePersistedThumbnail(path: string) {
	const cache = await openPersistedThumbnailCache();
	if (!cache) return;

	try {
		const request = persistentThumbnailRequest(path);
		if (!request) return;
		await cache.delete(request);
	} catch (error) {
		logger.warn("thumbnail cache delete failed", path, error);
	}
}

export async function clearPersistedBlobUrlCache() {
	if (!cacheStorageAvailable()) return;
	try {
		await globalThis.caches.delete(PERSISTED_THUMBNAIL_CACHE_NAME);
	} catch (error) {
		logger.warn("thumbnail cache clear failed", error);
	}
}

function scheduleBlobFetch<T>(lane: BlobFetchLane, task: () => Promise<T>) {
	return new Promise<T>((resolve, reject) => {
		const run = () => {
			activeBlobFetches[lane] += 1;
			task()
				.then(resolve, reject)
				.finally(() => {
					activeBlobFetches[lane] = Math.max(0, activeBlobFetches[lane] - 1);
					const next = pendingBlobFetches[lane].shift();
					next?.();
				});
		};

		if (activeBlobFetches[lane] < BLOB_FETCH_LIMITS[lane]) {
			run();
			return;
		}

		pendingBlobFetches[lane].push(run);
	});
}

function revokeEntry(path: string, entry: BlobCacheEntry) {
	if (entry.revokeTimer) {
		clearTimeout(entry.revokeTimer);
		entry.revokeTimer = undefined;
	}
	if (entry.refreshTimer) {
		clearTimeout(entry.refreshTimer);
		entry.refreshTimer = undefined;
	}
	if (entry.objectUrl) {
		URL.revokeObjectURL(entry.objectUrl);
	}
	blobUrlCache.delete(path);
}

function subscribeBlobUrlInvalidation(path: string, listener: () => void) {
	let listeners = blobUrlListeners.get(path);
	if (!listeners) {
		listeners = new Set();
		blobUrlListeners.set(path, listeners);
	}
	listeners.add(listener);

	return () => {
		const current = blobUrlListeners.get(path);
		if (!current) return;
		current.delete(listener);
		if (current.size === 0) {
			blobUrlListeners.delete(path);
		}
	};
}

function notifyBlobUrlInvalidation(path?: string) {
	if (path) {
		for (const listener of blobUrlListeners.get(path) ?? []) {
			listener();
		}
		return;
	}

	const listeners = new Set<() => void>();
	for (const pathListeners of blobUrlListeners.values()) {
		for (const listener of pathListeners) {
			listeners.add(listener);
		}
	}
	for (const listener of listeners) {
		listener();
	}
}

async function fetchBlobUrlFromNetwork({
	lane,
	logErrors = true,
	notifyOnChange = false,
	owner,
	cacheKey,
	canonicalEtag,
	previousBlob,
	previousEtag,
	previousObjectUrl,
	requestPath,
}: FetchBlobUrlOptions): Promise<string> {
	const headers: Record<string, string> = {};
	if (previousObjectUrl && previousEtag && !canonicalEtag) {
		headers["If-None-Match"] = previousEtag;
	}
	const MAX_RETRIES = 5;

	const fetchWithRetry = async (attempt: number): Promise<string> => {
		const response = await scheduleBlobFetch(lane, () =>
			api.client.get(requestPath, {
				headers,
				responseType: "blob",
				withCredentials: shouldSendResourceCredentials(requestPath),
				validateStatus: (status) =>
					status === 200 ||
					status === 304 ||
					status === 202 ||
					isMissingThumbnailResponse(status, lane),
			}),
		);

		// 202 = 缩略图正在后台生成，稍后重试
		if (response.status === 202) {
			const retryAfter = Number(response.headers["retry-after"]) || 2;
			if (attempt >= MAX_RETRIES) {
				const current = blobUrlCache.get(cacheKey);
				if (current && current === owner && !current.refreshTimer) {
					current.promise = undefined;
					current.refreshTimer = setTimeout(() => {
						const latest = blobUrlCache.get(cacheKey);
						if (!latest || latest !== owner) return;
						latest.refreshTimer = undefined;
						latest.needsRefresh = true;
						notifyBlobUrlInvalidation(cacheKey);
					}, retryAfter * 1000);
				}
				return previousObjectUrl ?? "";
			}
			await new Promise((r) => setTimeout(r, retryAfter * 1000));
			return fetchWithRetry(attempt + 1);
		}

		const current = blobUrlCache.get(cacheKey);
		if (!current || current !== owner) {
			return "";
		}

		if (isMissingThumbnailResponse(response.status, lane)) {
			current.blob = undefined;
			current.objectUrl = undefined;
			current.etag = null;
			current.missing = true;
			current.needsRefresh = false;
			current.promise = undefined;
			if (previousObjectUrl) {
				URL.revokeObjectURL(previousObjectUrl);
			}
			if (shouldPersistBlobOnDisk(lane))
				void deletePersistedThumbnail(cacheKey);
			notifyBlobUrlInvalidation(cacheKey);
			return "";
		}

		if (response.status === 304 && previousObjectUrl) {
			current.objectUrl = previousObjectUrl;
			current.blob = previousBlob;
			current.etag = previousEtag;
			current.missing = false;
			current.needsRefresh = false;
			current.promise = undefined;
			return previousObjectUrl;
		}

		const blob =
			response.data instanceof Blob
				? response.data
				: new Blob([response.data as BlobPart]);
		if (blobUrlCache.get(cacheKey) !== owner) {
			return "";
		}
		const objectUrl = URL.createObjectURL(blob);
		if (blobUrlCache.get(cacheKey) !== owner) {
			URL.revokeObjectURL(objectUrl);
			return "";
		}
		current.blob = blob;
		current.objectUrl = objectUrl;
		current.etag = canonicalEtag ?? response.headers.etag ?? null;
		current.missing = false;
		current.needsRefresh = false;
		current.promise = undefined;
		if (previousObjectUrl && previousObjectUrl !== objectUrl) {
			URL.revokeObjectURL(previousObjectUrl);
		}
		if (shouldPersistBlobOnDisk(lane) && blobUrlCache.get(cacheKey) === owner) {
			await writePersistedThumbnail(cacheKey, blob, current.etag ?? null);
		}
		if (notifyOnChange) notifyBlobUrlInvalidation(cacheKey);
		return objectUrl;
	};

	return fetchWithRetry(0).catch((error: unknown) => {
		if (logErrors) logger.warn("blob fetch failed", requestPath, error);
		const current = blobUrlCache.get(cacheKey);
		if (current) {
			current.promise = undefined;
			current.blob = previousBlob;
			current.objectUrl = previousObjectUrl;
			current.etag = previousEtag;
			current.missing = false;
			current.needsRefresh = false;
			if (!current.objectUrl && current.refCount <= 0) {
				blobUrlCache.delete(cacheKey);
			}
		}
		throw error;
	});
}

async function acquireBlobUrl(
	resource: ResourcePath,
	lane: BlobFetchLane,
): Promise<string> {
	const cacheKey = resourceCacheKey(resource);
	const requestPath = resourceRequestPath(resource);
	const canonicalEtag = resourceCanonicalEtag(resource);
	const cached = blobUrlCache.get(cacheKey);
	if (cached?.revokeTimer) {
		clearTimeout(cached.revokeTimer);
		cached.revokeTimer = undefined;
	}
	if (
		cached?.objectUrl &&
		!cached.needsRefresh &&
		cached.etag &&
		canonicalEtag &&
		cached.etag === canonicalEtag
	) {
		cached.lane = lane;
		cached.refCount += 1;
		return cached.objectUrl;
	}
	if (
		cached?.objectUrl &&
		!cached.needsRefresh &&
		!canonicalEtag &&
		(cached.refCount > 0 ||
			shouldPersistBlobInSession(cached.lane ?? lane) ||
			shouldPersistBlobInSession(lane))
	) {
		cached.lane = lane;
		cached.refCount += 1;
		return cached.objectUrl;
	}
	if (
		cached?.missing &&
		!cached.needsRefresh &&
		!canonicalEtag &&
		(cached.refCount > 0 ||
			shouldPersistBlobInSession(cached.lane ?? lane) ||
			shouldPersistBlobInSession(lane))
	) {
		cached.lane = lane;
		cached.refCount += 1;
		return "";
	}
	if (cached?.promise) {
		cached.lane = lane;
		cached.refCount += 1;
		return cached.promise;
	}

	const entry: BlobCacheEntry = cached ?? { refCount: 0 };
	if (entry.refreshTimer) {
		clearTimeout(entry.refreshTimer);
		entry.refreshTimer = undefined;
	}
	entry.needsRefresh = false;
	entry.lane = lane;
	entry.refCount += 1;
	const previousBlob = entry.blob;
	const previousObjectUrl = entry.objectUrl;
	const previousEtag = entry.etag ?? null;

	const promise = (async () => {
		if (shouldPersistBlobOnDisk(lane) && !previousObjectUrl) {
			const persisted = await readPersistedThumbnail(cacheKey);
			if (blobUrlCache.get(cacheKey) !== entry) {
				return "";
			}
			if (persisted) {
				const objectUrl = URL.createObjectURL(persisted.blob);
				if (blobUrlCache.get(cacheKey) !== entry) {
					URL.revokeObjectURL(objectUrl);
					return "";
				}
				entry.blob = persisted.blob;
				entry.objectUrl = objectUrl;
				entry.etag = persisted.etag;
				entry.missing = false;
				entry.needsRefresh = false;
				entry.promise = undefined;

				void fetchBlobUrlFromNetwork({
					lane,
					logErrors: false,
					notifyOnChange: true,
					owner: entry,
					cacheKey,
					canonicalEtag,
					previousBlob: persisted.blob,
					previousEtag: persisted.etag,
					previousObjectUrl: objectUrl,
					requestPath,
				}).catch(() => {});

				return objectUrl;
			}
		}

		return fetchBlobUrlFromNetwork({
			lane,
			owner: entry,
			cacheKey,
			canonicalEtag,
			previousBlob,
			previousEtag,
			previousObjectUrl,
			requestPath,
		});
	})();
	entry.promise = promise;
	blobUrlCache.set(cacheKey, entry);
	return promise;
}

function releaseBlobUrl(path: string) {
	const cached = blobUrlCache.get(path);
	if (!cached) return;
	cached.refCount = Math.max(0, cached.refCount - 1);
	if (cached.refCount > 0) return;
	if (shouldPersistBlobInSession(cached.lane ?? "default")) {
		if (cached.revokeTimer) {
			clearTimeout(cached.revokeTimer);
			cached.revokeTimer = undefined;
		}
		return;
	}
	if (cached.revokeTimer) clearTimeout(cached.revokeTimer);
	cached.revokeTimer = setTimeout(() => {
		const current = blobUrlCache.get(path);
		if (!current || current.refCount > 0) return;
		revokeEntry(path, current);
	}, BLOB_URL_REVOKE_DELAY);
}

export function invalidateBlobUrl(path?: string) {
	if (path) {
		const cached = blobUrlCache.get(path);
		if (cached) revokeEntry(path, cached);
		void deletePersistedThumbnail(path);
		notifyBlobUrlInvalidation(path);
		return;
	}
	for (const [cachePath, entry] of blobUrlCache.entries()) {
		revokeEntry(cachePath, entry);
	}
	void clearPersistedBlobUrlCache();
	notifyBlobUrlInvalidation();
}

export function clearBlobUrlCache() {
	for (const [cachePath, entry] of blobUrlCache.entries()) {
		revokeEntry(cachePath, entry);
	}
}

export function useBlobUrl(
	resource: ResourcePath | null,
	options?: BlobUrlOptions,
) {
	const [blob, setBlob] = useState<Blob | null>(null);
	const [blobUrl, setBlobUrl] = useState<string | null>(null);
	const [error, setError] = useState(false);
	const [loading, setLoading] = useState(false);
	const [retryCount, setRetryCount] = useState(0);
	const lane = options?.lane ?? "default";
	const cacheKey = resource ? resourceCacheKey(resource) : null;
	const requestPath = resource ? resourceRequestPath(resource) : null;
	const canonicalEtag = resource ? resourceCanonicalEtag(resource) : null;

	const retry = () => {
		setError(false);
		if (cacheKey) {
			invalidateBlobUrl(cacheKey);
		}
	};

	// biome-ignore lint/correctness/useExhaustiveDependencies: retryCount is an intentional re-fetch trigger
	useEffect(() => {
		setBlob(null);
		setBlobUrl(null);
		setError(false);
		if (!cacheKey || !requestPath) {
			setLoading(false);
			return;
		}
		const effectiveResource: ResourcePath = {
			cacheKey,
			etag: canonicalEtag,
			requestPath,
		};

		const unsubscribe = subscribeBlobUrlInvalidation(cacheKey, () => {
			setBlob(null);
			setBlobUrl(null);
			setError(false);
			setRetryCount((n) => n + 1);
		});

		const cached = blobUrlCache.get(cacheKey);
		const cachedMatchesCanonical =
			!canonicalEtag || cached?.etag === canonicalEtag;
		if (cached?.objectUrl && cachedMatchesCanonical) {
			setBlob(cached.blob ?? null);
			setBlobUrl(cached.objectUrl);
			setLoading(false);
		}

		let cancelled = false;
		setLoading(cached?.objectUrl === undefined || !cachedMatchesCanonical);
		acquireBlobUrl(effectiveResource, lane)
			.then((nextBlobUrl) => {
				if (cancelled) return;
				setBlob(blobUrlCache.get(cacheKey)?.blob ?? null);
				setBlobUrl(nextBlobUrl || null);
			})
			.catch(() => {
				if (cancelled) return;
				setBlob(null);
				setError(true);
			})
			.finally(() => {
				if (cancelled) return;
				setLoading(false);
			});

		return () => {
			cancelled = true;
			unsubscribe();
			releaseBlobUrl(cacheKey);
		};
	}, [cacheKey, canonicalEtag, lane, requestPath, retryCount]);

	return { blob, blobUrl, error, loading, retry };
}
