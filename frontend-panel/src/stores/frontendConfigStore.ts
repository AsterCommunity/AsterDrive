import { create } from "zustand";
import {
	type AppliedBranding,
	applyBranding,
	DEFAULT_BRANDING,
	resolveBranding,
} from "@/lib/branding";
import { logger } from "@/lib/logger";
import {
	normalizePublicSiteUrls,
	setPublicSiteUrls,
} from "@/lib/publicSiteUrl";
import { frontendConfigService } from "@/services/frontendConfigService";
import type {
	PublicBranding,
	PublicFrontendConfig,
	PublicImagePreviewPreference,
} from "@/types/api";

export const FRONTEND_CONFIG_CACHE_KEY = "aster-cached-frontend-config:v1";
const FRONTEND_CONFIG_REVALIDATE_INTERVAL_MS = 30_000;
const DEFAULT_IMAGE_PREVIEW_PREFERENCE: PublicImagePreviewPreference =
	"original_first";

interface CachedFrontendConfigPayload {
	config: PublicFrontendConfig;
	cachedAt?: number;
}

interface FrontendConfigState {
	allowUserRegistration: boolean;
	branding: AppliedBranding;
	config: PublicFrontendConfig | null;
	imagePreviewPreference: PublicImagePreviewPreference;
	isLoaded: boolean;
	passkeyLoginEnabled: boolean;
	siteUrl: string | null;
	invalidate: () => void;
	load: (options?: { force?: boolean }) => Promise<void>;
}

let inFlightLoad: Promise<void> | null = null;
let lastRevalidationAttemptAt = 0;

function isImagePreviewPreference(
	value: unknown,
): value is PublicImagePreviewPreference {
	return value === "preview_first" || value === "original_first";
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isStringArray(value: unknown): value is string[] {
	return (
		Array.isArray(value) && value.every((item) => typeof item === "string")
	);
}

function isPublicBranding(value: unknown): value is PublicBranding {
	if (!isRecord(value)) {
		return false;
	}

	const passkeyLoginEnabled = value.passkey_login_enabled;

	return (
		typeof value.allow_user_registration === "boolean" &&
		typeof value.description === "string" &&
		typeof value.favicon_url === "string" &&
		(passkeyLoginEnabled === undefined ||
			typeof passkeyLoginEnabled === "boolean") &&
		isStringArray(value.site_urls) &&
		typeof value.title === "string" &&
		typeof value.wordmark_dark_url === "string" &&
		typeof value.wordmark_light_url === "string"
	);
}

function isFrontendConfig(value: unknown): value is PublicFrontendConfig {
	return (
		isRecord(value) &&
		typeof value.version === "number" &&
		Number.isFinite(value.version) &&
		isPublicBranding(value.branding) &&
		isRecord(value.media) &&
		isImagePreviewPreference(value.media.image_preview_preference)
	);
}

function readCachedFrontendConfig(): CachedFrontendConfigPayload | null {
	if (typeof window === "undefined") return null;

	try {
		const raw = localStorage.getItem(FRONTEND_CONFIG_CACHE_KEY);
		if (!raw) return null;

		const parsed = JSON.parse(raw) as CachedFrontendConfigPayload | null;
		if (!isRecord(parsed) || !isFrontendConfig(parsed.config)) {
			localStorage.removeItem(FRONTEND_CONFIG_CACHE_KEY);
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
			localStorage.removeItem(FRONTEND_CONFIG_CACHE_KEY);
		} catch {
			// ignore storage failures
		}
		return null;
	}
}

function writeCachedFrontendConfig(config: PublicFrontendConfig) {
	if (typeof window === "undefined") return;

	try {
		localStorage.setItem(
			FRONTEND_CONFIG_CACHE_KEY,
			JSON.stringify({
				config,
				cachedAt: Date.now(),
			} satisfies CachedFrontendConfigPayload),
		);
	} catch {
		// ignore storage failures
	}
}

function clearCachedFrontendConfig() {
	if (typeof window === "undefined") return;

	try {
		localStorage.removeItem(FRONTEND_CONFIG_CACHE_KEY);
	} catch {
		// ignore storage failures
	}
}

function applyFrontendConfig(config: PublicFrontendConfig) {
	const branding = resolveBranding(config.branding);
	const siteUrl = setPublicSiteUrls(config.branding.site_urls);
	applyBranding(branding);
	return {
		allowUserRegistration: config.branding.allow_user_registration ?? true,
		branding,
		config,
		imagePreviewPreference: config.media.image_preview_preference,
		isLoaded: true,
		passkeyLoginEnabled: config.branding.passkey_login_enabled ?? true,
		siteUrl,
	};
}

function fallbackState() {
	setPublicSiteUrls(null);
	applyBranding(DEFAULT_BRANDING);
	return {
		allowUserRegistration: true,
		branding: DEFAULT_BRANDING,
		config: null,
		imagePreviewPreference: DEFAULT_IMAGE_PREVIEW_PREFERENCE,
		isLoaded: true,
		passkeyLoginEnabled: true,
		siteUrl: null,
	};
}

function shouldSkipRevalidation(force: boolean, isLoaded: boolean) {
	if (force || !isLoaded) return false;
	return (
		Date.now() - lastRevalidationAttemptAt <
		FRONTEND_CONFIG_REVALIDATE_INTERVAL_MS
	);
}

const initialCachedPayload = readCachedFrontendConfig();
const initialCachedConfig = initialCachedPayload?.config ?? null;
const initialBranding = resolveBranding(initialCachedConfig?.branding ?? null);
const initialSiteUrl = initialCachedConfig
	? (normalizePublicSiteUrls(initialCachedConfig.branding.site_urls)[0] ?? null)
	: null;

export const useFrontendConfigStore = create<FrontendConfigState>(
	(set, get) => ({
		allowUserRegistration:
			initialCachedConfig?.branding.allow_user_registration ?? true,
		branding: initialBranding,
		config: initialCachedConfig,
		imagePreviewPreference:
			initialCachedConfig?.media.image_preview_preference ??
			DEFAULT_IMAGE_PREVIEW_PREFERENCE,
		isLoaded: initialCachedConfig !== null,
		passkeyLoginEnabled:
			initialCachedConfig?.branding.passkey_login_enabled ?? true,
		siteUrl: initialSiteUrl,

		invalidate: () => {
			clearCachedFrontendConfig();
			lastRevalidationAttemptAt = 0;
			set({
				allowUserRegistration: true,
				branding: DEFAULT_BRANDING,
				config: null,
				imagePreviewPreference: DEFAULT_IMAGE_PREVIEW_PREFERENCE,
				isLoaded: false,
				passkeyLoginEnabled: true,
				siteUrl: null,
			});
		},

		load: async ({ force = false } = {}) => {
			if (shouldSkipRevalidation(force, get().isLoaded)) return;
			if (inFlightLoad) return inFlightLoad;

			inFlightLoad = (async () => {
				lastRevalidationAttemptAt = Date.now();
				try {
					const config = await frontendConfigService.get();
					writeCachedFrontendConfig(config);
					set(applyFrontendConfig(config));
				} catch (error) {
					logger.warn(
						"frontend config bootstrap failed, using cached/defaults",
						error,
					);
					if (get().isLoaded) return;
					set(fallbackState());
				} finally {
					inFlightLoad = null;
				}
			})();

			return inFlightLoad;
		},
	}),
);

export function initFrontendConfigRuntime() {
	if (typeof window === "undefined" || !initialCachedConfig) return;
	const siteUrl = setPublicSiteUrls(initialCachedConfig.branding.site_urls);
	applyBranding(initialBranding);
	useFrontendConfigStore.setState({ siteUrl });
}

export function setFrontendSiteUrlState(siteUrl: string | null) {
	useFrontendConfigStore.setState({ siteUrl });
}
