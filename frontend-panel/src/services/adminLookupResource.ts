import {
	type AdminLookupKind,
	type AdminLookupOption,
	adminLookupService,
} from "@/services/adminLookupService";

export const ADMIN_LOOKUP_CACHE_TTL_MS = 30_000;
const MAX_CACHE_ENTRIES = 100;

// Owned by the authenticated admin route, never persisted across sessions.
function createLookupCache<T>() {
	type Entry = {
		controller: AbortController;
		consumers: Set<symbol>;
		promise: Promise<T>;
		loadedAt: number;
		settled: boolean;
	};
	const entries = new Map<string, Entry>();
	return {
		load(
			key: string,
			signal: AbortSignal,
			fetch: (signal: AbortSignal) => Promise<T>,
			force = false,
		): Promise<T> {
			if (signal.aborted)
				return Promise.reject(new DOMException("Aborted", "AbortError"));
			let entry = entries.get(key);
			if (
				force ||
				entry?.controller.signal.aborted ||
				(entry?.settled &&
					Date.now() - entry.loadedAt >= ADMIN_LOOKUP_CACHE_TTL_MS)
			)
				entry = undefined;
			if (!entry) {
				const controller = new AbortController();
				const next: Entry = {
					controller,
					consumers: new Set(),
					loadedAt: 0,
					settled: false,
					promise: fetch(controller.signal).then(
						(value) => {
							next.settled = true;
							next.loadedAt = Date.now();
							return value;
						},
						(error: unknown) => {
							if (entries.get(key) === next) entries.delete(key);
							throw error;
						},
					),
				};
				entry = next;
			}
			entries.delete(key);
			entries.set(key, entry);
			if (entries.size > MAX_CACHE_ENTRIES) {
				const oldest = entries.keys().next().value;
				if (oldest !== undefined) entries.delete(oldest);
			}
			const current = entry;
			const consumer = Symbol();
			current.consumers.add(consumer);
			return new Promise<T>((resolve, reject) => {
				const release = () => {
					signal.removeEventListener("abort", abort);
					current.consumers.delete(consumer);
				};
				const abort = () => {
					release();
					if (!current.settled && current.consumers.size === 0) {
						current.controller.abort();
						if (entries.get(key) === current) entries.delete(key);
					}
					reject(new DOMException("Aborted", "AbortError"));
				};
				signal.addEventListener("abort", abort, { once: true });
				current.promise.then(
					(value) => {
						release();
						resolve(value);
					},
					(error: unknown) => {
						release();
						reject(error);
					},
				);
			});
		},
	};
}

export function createAdminLookupResource() {
	const pages =
		createLookupCache<Awaited<ReturnType<typeof adminLookupService.search>>>();
	const identities = createLookupCache<AdminLookupOption>();
	return {
		search(
			kind: AdminLookupKind,
			keyword: string,
			offset: number,
			signal: AbortSignal,
			force = false,
		) {
			const normalized = keyword.trim();
			return pages.load(
				JSON.stringify([kind, normalized, offset]),
				signal,
				(sharedSignal) =>
					adminLookupService.search(kind, normalized, offset, sharedSignal),
				force,
			);
		},
		get(kind: AdminLookupKind, id: number, signal: AbortSignal, force = false) {
			return identities.load(
				JSON.stringify([kind, id]),
				signal,
				(sharedSignal) => adminLookupService.get(kind, id, sharedSignal),
				force,
			);
		},
	};
}
