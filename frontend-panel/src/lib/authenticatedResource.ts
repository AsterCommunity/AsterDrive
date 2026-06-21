import {
	resolveApiResourceUrl,
	shouldSendResourceCredentials,
} from "@/lib/apiUrl";
import { isSessionAuthFailure } from "@/lib/authErrors";
import { logger } from "@/lib/logger";
import { useAuthStore } from "@/stores/authStore";

const AUTHENTICATED_RESOURCE_PROBE_CACHE_MS = 5_000;

type ProbeCacheEntry = {
	expiresAt: number;
	promise: Promise<void>;
};

type PrepareAuthenticatedResourceOptions = {
	signal?: AbortSignal;
};

const probeCache = new Map<string, ProbeCacheEntry>();

function shouldProbeAuthenticatedResource(path: string) {
	return shouldSendResourceCredentials(path);
}

function probeCacheKey(path: string) {
	const sessionKey = useAuthStore.getState().expiresAt ?? "unknown";
	return `${sessionKey}:${path}`;
}

function probeStatusError(status: number) {
	return {
		status,
		response: { status },
	};
}

function abortError() {
	return new DOMException("The operation was aborted.", "AbortError");
}

function throwIfAborted(signal?: AbortSignal) {
	if (signal?.aborted) {
		throw abortError();
	}
}

async function probeAuthenticatedResource(path: string, signal?: AbortSignal) {
	throwIfAborted(signal);
	const response = await fetch(resolveApiResourceUrl(path), {
		headers: {
			Range: "bytes=0-0",
		},
		credentials: "include",
		signal,
	});
	throwIfAborted(signal);
	await response.body?.cancel();
	throwIfAborted(signal);
	if (response.status === 206 || response.status === 416) return;
	throw probeStatusError(response.status);
}

function rejectOnAbort<T>(promise: Promise<T>, signal?: AbortSignal) {
	if (!signal) return promise;
	if (signal.aborted) return Promise.reject(abortError());

	return new Promise<T>((resolve, reject) => {
		const handleAbort = () => reject(abortError());
		signal.addEventListener("abort", handleAbort, { once: true });
		promise.then(
			(value) => {
				signal.removeEventListener("abort", handleAbort);
				resolve(value);
			},
			(error) => {
				signal.removeEventListener("abort", handleAbort);
				reject(error);
			},
		);
	});
}

export async function prepareAuthenticatedResource(
	path: string,
	options: PrepareAuthenticatedResourceOptions = {},
): Promise<void> {
	if (!shouldProbeAuthenticatedResource(path)) return;

	await useAuthStore.getState().ensureFreshSession();
	throwIfAborted(options.signal);

	const cacheKey = probeCacheKey(path);
	const now = Date.now();
	const cached = probeCache.get(cacheKey);
	if (cached && cached.expiresAt > now) {
		await rejectOnAbort(cached.promise, options.signal);
		return;
	}
	if (cached) {
		probeCache.delete(cacheKey);
	}

	let probe: Promise<void>;
	probe = (async () => {
		try {
			await probeAuthenticatedResource(path, options.signal);
		} catch (error) {
			if (!isSessionAuthFailure(error)) {
				throw error;
			}
			await useAuthStore.getState().refreshToken();
			throwIfAborted(options.signal);
			await probeAuthenticatedResource(path, options.signal);
		}
	})()
		.then(() => {
			const finalCacheKey = probeCacheKey(path);
			if (finalCacheKey !== cacheKey) {
				probeCache.delete(cacheKey);
				probeCache.set(finalCacheKey, {
					expiresAt: Date.now() + AUTHENTICATED_RESOURCE_PROBE_CACHE_MS,
					promise: probe,
				});
			}
		})
		.catch((error) => {
			if (isSessionAuthFailure(error)) {
				throw error;
			}
			logger.error("authenticated resource probe failed", path, error);
			throw error;
		});

	probeCache.set(cacheKey, {
		expiresAt: now + AUTHENTICATED_RESOURCE_PROBE_CACHE_MS,
		promise: probe,
	});

	await rejectOnAbort(probe, options.signal);
}
