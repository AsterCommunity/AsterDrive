import { create } from "zustand";
import { logger } from "@/lib/logger";
import { previewAppsService } from "@/services/previewAppsService";
import type { PublicPreviewAppsConfig } from "@/types/api";

export const PREVIEW_APPS_CACHE_KEY = "aster-cached-preview-apps";
const PREVIEW_APPS_REVALIDATE_INTERVAL_MS = 30_000;

interface CachedPreviewAppsPayload {
	config: PublicPreviewAppsConfig;
	cachedAt?: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasLocalizedLabels(value: unknown) {
	if (!isRecord(value)) {
		return false;
	}

	return Object.values(value).some(
		(item) => typeof item === "string" && item.trim().length > 0,
	);
}

function isPreviewAppsConfig(value: unknown): value is PublicPreviewAppsConfig {
	if (!isRecord(value)) {
		return false;
	}

	const { apps, version } = value;
	if (
		typeof version !== "number" ||
		!Array.isArray(apps) ||
		apps.length === 0
	) {
		return false;
	}

	return apps.every(
		(app) =>
			isRecord(app) &&
			typeof app.key === "string" &&
			app.key.trim().length > 0 &&
			typeof app.icon === "string" &&
			typeof app.provider === "string" &&
			app.provider.trim().length > 0 &&
			hasLocalizedLabels(app.labels),
	);
}

function readCachedPreviewApps(): CachedPreviewAppsPayload | null {
	try {
		const raw = localStorage.getItem(PREVIEW_APPS_CACHE_KEY);
		if (!raw) {
			return null;
		}

		const parsed = JSON.parse(raw) as CachedPreviewAppsPayload | null;
		if (!parsed || typeof parsed !== "object" || !("config" in parsed)) {
			localStorage.removeItem(PREVIEW_APPS_CACHE_KEY);
			return null;
		}

		if (!isPreviewAppsConfig(parsed.config)) {
			localStorage.removeItem(PREVIEW_APPS_CACHE_KEY);
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
			localStorage.removeItem(PREVIEW_APPS_CACHE_KEY);
		} catch {
			// ignore storage failures
		}
		return null;
	}
}

function writeCachedPreviewApps(config: PublicPreviewAppsConfig) {
	try {
		localStorage.setItem(
			PREVIEW_APPS_CACHE_KEY,
			JSON.stringify({
				config,
				cachedAt: Date.now(),
			} satisfies CachedPreviewAppsPayload),
		);
	} catch {
		// ignore storage failures
	}
}

function clearCachedPreviewApps() {
	try {
		localStorage.removeItem(PREVIEW_APPS_CACHE_KEY);
	} catch {
		// ignore storage failures
	}
}

const initialCachedPayload = readCachedPreviewApps();
const initialCachedConfig = initialCachedPayload?.config ?? null;
let inFlightLoad: Promise<void> | null = null;
let lastRevalidationAttemptAt = 0;

interface PreviewAppState {
	config: PublicPreviewAppsConfig | null;
	isLoaded: boolean;
	invalidate: () => void;
	load: (options?: { force?: boolean }) => Promise<void>;
}

export const usePreviewAppStore = create<PreviewAppState>((set) => ({
	config: initialCachedConfig,
	isLoaded: initialCachedConfig !== null,

	invalidate: () => {
		clearCachedPreviewApps();
		lastRevalidationAttemptAt = 0;
		set({
			config: null,
			isLoaded: false,
		});
	},

	load: async ({ force = false } = {}) => {
		if (
			!force &&
			usePreviewAppStore.getState().isLoaded &&
			Date.now() - lastRevalidationAttemptAt <
				PREVIEW_APPS_REVALIDATE_INTERVAL_MS
		) {
			return;
		}
		if (inFlightLoad) return inFlightLoad;

		inFlightLoad = (async () => {
			lastRevalidationAttemptAt = Date.now();
			try {
				const config = await previewAppsService.get();
				writeCachedPreviewApps(config);
				set({
					config,
					isLoaded: true,
				});
			} catch (error) {
				logger.warn(
					"preview apps bootstrap failed, using local fallback",
					error,
				);
				set((state) =>
					state.isLoaded
						? state
						: {
								config: null,
								isLoaded: true,
							},
				);
			} finally {
				inFlightLoad = null;
			}
		})();

		return inFlightLoad;
	},
}));
