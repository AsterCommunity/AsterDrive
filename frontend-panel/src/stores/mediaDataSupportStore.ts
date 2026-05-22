import { create } from "zustand";
import { logger } from "@/lib/logger";
import { mediaDataSupportService } from "@/services/mediaDataSupportService";
import type { PublicMediaDataSupport } from "@/types/api";

export const MEDIA_DATA_SUPPORT_CACHE_KEY =
	"aster-cached-media-data-support:v1";
const MEDIA_DATA_SUPPORT_REVALIDATE_INTERVAL_MS = 30_000;

interface CachedMediaDataSupportPayload {
	config: PublicMediaDataSupport;
	cachedAt?: number;
}

let inFlightLoad: Promise<void> | null = null;
let latestLoadToken = 0;
let lastRevalidationAttemptAt = 0;

interface MediaDataSupportState {
	config: PublicMediaDataSupport | null;
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

function isKindSupport(value: unknown) {
	return (
		isRecord(value) &&
		typeof value.enabled === "boolean" &&
		(value.match === "extensions" || value.match === "any") &&
		(value.extensions === undefined || isStringArray(value.extensions))
	);
}

function isMediaDataSupportConfig(
	value: unknown,
): value is PublicMediaDataSupport {
	if (!isRecord(value) || !isRecord(value.kinds)) {
		return false;
	}

	return (
		typeof value.version === "number" &&
		Number.isFinite(value.version) &&
		typeof value.enabled === "boolean" &&
		typeof value.max_source_bytes === "number" &&
		Number.isFinite(value.max_source_bytes) &&
		isKindSupport(value.kinds.image) &&
		isKindSupport(value.kinds.audio) &&
		isKindSupport(value.kinds.video)
	);
}

function readCachedMediaDataSupport(): CachedMediaDataSupportPayload | null {
	try {
		const raw = localStorage.getItem(MEDIA_DATA_SUPPORT_CACHE_KEY);
		if (!raw) {
			return null;
		}

		const parsed = JSON.parse(raw) as CachedMediaDataSupportPayload | null;
		if (!isRecord(parsed) || !isMediaDataSupportConfig(parsed.config)) {
			localStorage.removeItem(MEDIA_DATA_SUPPORT_CACHE_KEY);
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
			localStorage.removeItem(MEDIA_DATA_SUPPORT_CACHE_KEY);
		} catch {
			// ignore storage failures
		}
		return null;
	}
}

function writeCachedMediaDataSupport(config: PublicMediaDataSupport) {
	try {
		localStorage.setItem(
			MEDIA_DATA_SUPPORT_CACHE_KEY,
			JSON.stringify({
				config,
				cachedAt: Date.now(),
			} satisfies CachedMediaDataSupportPayload),
		);
	} catch {
		// ignore storage failures
	}
}

function clearCachedMediaDataSupport() {
	try {
		localStorage.removeItem(MEDIA_DATA_SUPPORT_CACHE_KEY);
	} catch {
		// ignore storage failures
	}
}

const initialCachedPayload = readCachedMediaDataSupport();
const initialCachedConfig = initialCachedPayload?.config ?? null;

export const useMediaDataSupportStore = create<MediaDataSupportState>(
	(set, get) => ({
		config: initialCachedConfig,
		isLoaded: initialCachedConfig !== null,

		invalidate: () => {
			latestLoadToken += 1;
			inFlightLoad = null;
			clearCachedMediaDataSupport();
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
					MEDIA_DATA_SUPPORT_REVALIDATE_INTERVAL_MS
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
					const config = await mediaDataSupportService.get();
					if (latestLoadToken === loadToken) {
						writeCachedMediaDataSupport(config);
						set({
							config,
							isLoaded: true,
						});
					}
				} catch (error) {
					logger.warn(
						"media data support bootstrap failed, using cached support list when available",
						error,
					);
					if (latestLoadToken === loadToken) {
						set((state) =>
							state.isLoaded
								? state
								: {
										config: null,
										isLoaded: false,
									},
						);
					}
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
