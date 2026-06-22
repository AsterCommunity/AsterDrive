import { useCallback, useEffect, useRef, useState } from "react";
import { shouldSendResourceCredentials } from "@/lib/apiUrl";
import {
	type ResourcePath,
	resourceCacheKey,
	resourceCanonicalEtag,
	resourceConditionalHeaders,
	resourceCredentials,
	resourceRequestPath,
} from "@/lib/resourceRequest";
import { api } from "@/services/http";

interface TextCacheValue {
	content: string;
	etag: string | null;
}

interface TextCacheEntry extends Partial<TextCacheValue> {
	promise?: Promise<TextCacheValue>;
}

const textContentCache = new Map<string, TextCacheEntry>();
const textContentListeners = new Map<string, Set<() => void>>();

function subscribeTextContentInvalidation(path: string, listener: () => void) {
	let listeners = textContentListeners.get(path);
	if (!listeners) {
		listeners = new Set();
		textContentListeners.set(path, listeners);
	}
	listeners.add(listener);

	return () => {
		const current = textContentListeners.get(path);
		if (!current) return;
		current.delete(listener);
		if (current.size === 0) {
			textContentListeners.delete(path);
		}
	};
}

function notifyTextContentInvalidation(path?: string) {
	if (path) {
		for (const listener of textContentListeners.get(path) ?? []) {
			listener();
		}
		return;
	}

	const listeners = new Set<() => void>();
	for (const pathListeners of textContentListeners.values()) {
		for (const listener of pathListeners) {
			listeners.add(listener);
		}
	}
	for (const listener of listeners) {
		listener();
	}
}

async function fetchTextContent(
	resource: ResourcePath,
	force = false,
): Promise<TextCacheValue> {
	const cacheKey = resourceCacheKey(resource);
	const requestPath = resourceRequestPath(resource);
	const canonicalEtag = resourceCanonicalEtag(resource);
	const cached = textContentCache.get(cacheKey);
	if (!force && cached?.promise) {
		return cached.promise;
	}
	if (
		!force &&
		canonicalEtag &&
		cached?.content !== undefined &&
		cached.etag === canonicalEtag
	) {
		return {
			content: cached.content,
			etag: cached.etag ?? null,
		};
	}
	const headers: Record<string, string> = {};
	if (
		!force &&
		cached?.etag &&
		!canonicalEtag &&
		resourceConditionalHeaders(resource) === "allowed"
	) {
		headers["If-None-Match"] = cached.etag;
	}

	const promise = api.client
		.get(requestPath, {
			headers,
			responseType: "text",
			withCredentials: resourceCredentials(
				resource,
				shouldSendResourceCredentials,
			),
			validateStatus: (status) => status === 200 || status === 304,
		})
		.then((response) => {
			if (response.status === 304 && cached?.content !== undefined) {
				const next = {
					content: cached.content,
					etag: cached.etag ?? null,
				};
				textContentCache.set(cacheKey, next);
				return next;
			}
			const next = {
				content: response.data as string,
				etag: canonicalEtag ?? response.headers.etag ?? null,
			};
			textContentCache.set(cacheKey, next);
			return next;
		})
		.catch((error: unknown) => {
			if (cached?.content !== undefined) {
				textContentCache.set(cacheKey, {
					content: cached.content,
					etag: cached.etag ?? null,
				});
			} else {
				textContentCache.delete(cacheKey);
			}
			throw error;
		});

	textContentCache.set(cacheKey, {
		content: cached?.content,
		etag: cached?.etag ?? null,
		promise,
	});

	return promise;
}

export function invalidateTextContent(path?: string) {
	if (path) {
		textContentCache.delete(path);
		notifyTextContentInvalidation(path);
		return;
	}
	textContentCache.clear();
	notifyTextContentInvalidation();
}

export function clearTextContentCache() {
	textContentCache.clear();
}

export function useTextContent(resource: ResourcePath | null) {
	const [content, setContentState] = useState<string | null>(null);
	const [etag, setEtagState] = useState<string | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState(false);
	const requestIdRef = useRef(0);
	const cacheKey = resource ? resourceCacheKey(resource) : null;
	const requestPath = resource ? resourceRequestPath(resource) : null;
	const canonicalEtag = resource ? resourceCanonicalEtag(resource) : null;
	const requestCredentials =
		resource && resourceCredentials(resource, shouldSendResourceCredentials)
			? "include"
			: "omit";
	const requestConditionalHeaders = resource
		? resourceConditionalHeaders(resource)
		: "allowed";

	const setContent = useCallback(
		(value: string | null | ((prev: string | null) => string | null)) => {
			setContentState((prev) => {
				const next = typeof value === "function" ? value(prev) : value;
				if (cacheKey && next !== null) {
					const cached = textContentCache.get(cacheKey);
					textContentCache.set(cacheKey, {
						content: next,
						etag: cached?.etag ?? null,
						promise: cached?.promise,
					});
				}
				return next;
			});
		},
		[cacheKey],
	);

	const setEtag = useCallback(
		(value: string | null | ((prev: string | null) => string | null)) => {
			setEtagState((prev) => {
				const next = typeof value === "function" ? value(prev) : value;
				if (cacheKey) {
					const cached = textContentCache.get(cacheKey);
					if (cached?.content !== undefined) {
						textContentCache.set(cacheKey, {
							content: cached.content,
							etag: next,
							promise: cached.promise,
						});
					}
				}
				return next;
			});
		},
		[cacheKey],
	);

	const load = useCallback(
		async (force = false) => {
			requestIdRef.current += 1;
			const requestId = requestIdRef.current;
			if (!cacheKey || !requestPath) {
				setContentState(null);
				setEtagState(null);
				setLoading(false);
				setError(false);
				return;
			}
			const effectiveResource: ResourcePath = {
				cacheKey,
				etag: canonicalEtag,
				credentials: requestCredentials,
				conditionalHeaders: requestConditionalHeaders,
				requestPath,
			};

			const cached = textContentCache.get(cacheKey);
			const cachedMatchesCanonical =
				!canonicalEtag || cached?.etag === canonicalEtag;
			if (cached?.content !== undefined && cachedMatchesCanonical) {
				setContentState(cached.content);
				setEtagState(cached.etag ?? null);
				setLoading(false);
				setError(false);
			}

			setLoading(cached?.content === undefined || !cachedMatchesCanonical);
			setError(false);
			try {
				const next = await fetchTextContent(effectiveResource, force);
				if (requestId !== requestIdRef.current) return;
				setContentState(next.content);
				setEtagState(next.etag);
			} catch {
				if (requestId !== requestIdRef.current) return;
				setError(true);
			} finally {
				if (requestId === requestIdRef.current) {
					setLoading(false);
				}
			}
		},
		[
			cacheKey,
			canonicalEtag,
			requestConditionalHeaders,
			requestCredentials,
			requestPath,
		],
	);

	const reload = useCallback(async () => {
		await load(true);
	}, [load]);

	useEffect(() => {
		if (!cacheKey || !requestPath) {
			void load();
			return;
		}

		const unsubscribe = subscribeTextContentInvalidation(cacheKey, () => {
			void load(true);
		});

		load();

		return () => {
			unsubscribe();
		};
	}, [cacheKey, load, requestPath]);

	return {
		content,
		etag,
		loading,
		error,
		reload,
		setContent,
		setEtag,
	};
}
