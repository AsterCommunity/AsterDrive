import { create } from "zustand";
import { logger } from "@/lib/logger";
import { thumbnailSupportService } from "@/services/thumbnailSupportService";
import type { PublicThumbnailSupport } from "@/types/api";

export const THUMBNAIL_SUPPORT_CACHE_KEY = "aster-cached-thumbnail-support";
const THUMBNAIL_SUPPORT_REVALIDATE_INTERVAL_MS = 30_000;

interface CachedThumbnailSupportPayload {
	config: PublicThumbnailSupport;
	cachedAt?: number;
}

let inFlightLoad: Promise<void> | null = null;
let latestLoadToken = 0;
let lastRevalidationAttemptAt = 0;

interface ThumbnailSupportState {
	config: PublicThumbnailSupport | null;
	isLoaded: boolean;
	invalidate: () => void;
	load: (options?: { force?: boolean }) => Promise<void>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isStringArray(value: unknown): value is string[] {
	return (
		Array.isArray(value) && value.every((item) => typeof item === "string")
	);
}

function isThumbnailSupportConfig(
	value: unknown,
): value is PublicThumbnailSupport {
	if (!isRecord(value)) {
		return false;
	}

	return (
		typeof value.version === "number" &&
		Number.isFinite(value.version) &&
		(value.extensions === undefined || isStringArray(value.extensions))
	);
}

function readCachedThumbnailSupport(): CachedThumbnailSupportPayload | null {
	try {
		const raw = localStorage.getItem(THUMBNAIL_SUPPORT_CACHE_KEY);
		if (!raw) {
			return null;
		}

		const parsed = JSON.parse(raw) as CachedThumbnailSupportPayload | null;
		if (!isRecord(parsed) || !isThumbnailSupportConfig(parsed.config)) {
			localStorage.removeItem(THUMBNAIL_SUPPORT_CACHE_KEY);
			return null;
		}

		return {
			config: parsed.config,
			cachedAt:
				typeof parsed.cachedAt === "number" && Number.isFinite(parsed.cachedAt)
					? parsed.cachedAt
					: 0,
		};
	} catch {
		try {
			localStorage.removeItem(THUMBNAIL_SUPPORT_CACHE_KEY);
		} catch {
			// ignore storage failures
		}
		return null;
	}
}

function writeCachedThumbnailSupport(config: PublicThumbnailSupport) {
	try {
		localStorage.setItem(
			THUMBNAIL_SUPPORT_CACHE_KEY,
			JSON.stringify({
				config,
				cachedAt: Date.now(),
			} satisfies CachedThumbnailSupportPayload),
		);
	} catch {
		// ignore storage failures
	}
}

function clearCachedThumbnailSupport() {
	try {
		localStorage.removeItem(THUMBNAIL_SUPPORT_CACHE_KEY);
	} catch {
		// ignore storage failures
	}
}

const initialCachedPayload = readCachedThumbnailSupport();
const initialCachedConfig = initialCachedPayload?.config ?? null;

export const useThumbnailSupportStore = create<ThumbnailSupportState>(
	(set, get) => ({
		config: initialCachedConfig,
		isLoaded: initialCachedConfig !== null,

		invalidate: () => {
			clearCachedThumbnailSupport();
			lastRevalidationAttemptAt = 0;
			set({
				config: null,
				isLoaded: false,
			});
		},

		load: async ({ force = false } = {}) => {
			if (
				!force &&
				get().isLoaded &&
				Date.now() - lastRevalidationAttemptAt <
					THUMBNAIL_SUPPORT_REVALIDATE_INTERVAL_MS
			) {
				return;
			}
			if (!force && inFlightLoad) return inFlightLoad;

			const loadToken = latestLoadToken + 1;
			latestLoadToken = loadToken;
			let loadPromise: Promise<void> | null = null;
			loadPromise = (async () => {
				lastRevalidationAttemptAt = Date.now();
				try {
					const config = await thumbnailSupportService.get();
					if (latestLoadToken !== loadToken) return;
					writeCachedThumbnailSupport(config);
					set({
						config,
						isLoaded: true,
					});
				} catch (error) {
					logger.warn(
						"thumbnail support bootstrap failed, using cached support list when available",
						error,
					);
					if (latestLoadToken !== loadToken) return;
					set((state) =>
						state.isLoaded
							? state
							: {
									config: null,
									isLoaded: false,
								},
					);
				} finally {
					if (inFlightLoad === loadPromise) {
						inFlightLoad = null;
					}
				}
			})();

			inFlightLoad = loadPromise;

			return loadPromise;
		},
	}),
);
