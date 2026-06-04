import { create } from "zustand";
import {
	type AppliedBranding,
	applyBranding,
	DEFAULT_BRANDING,
	resolveBranding,
} from "@/lib/branding";
import { logger } from "@/lib/logger";
import { setPublicSiteUrls } from "@/lib/publicSiteUrl";
import { brandingService } from "@/services/brandingService";
import type { PublicBranding } from "@/types/api";

export const BRANDING_CACHE_KEY = "aster-cached-branding";
const BRANDING_REVALIDATE_INTERVAL_MS = 30_000;

interface CachedBrandingPayload {
	branding: PublicBranding;
	cachedAt?: number;
}

let inFlightLoad: Promise<void> | null = null;
let lastRevalidationAttemptAt = 0;

interface BrandingState {
	allowUserRegistration: boolean;
	branding: AppliedBranding;
	isLoaded: boolean;
	passkeyLoginEnabled: boolean;
	siteUrl: string | null;
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

function readCachedBranding(): CachedBrandingPayload | null {
	try {
		const raw = localStorage.getItem(BRANDING_CACHE_KEY);
		if (!raw) {
			return null;
		}

		const parsed = JSON.parse(raw) as CachedBrandingPayload | null;
		if (!isRecord(parsed) || !isPublicBranding(parsed.branding)) {
			localStorage.removeItem(BRANDING_CACHE_KEY);
			return null;
		}

		return {
			branding: parsed.branding,
			cachedAt:
				typeof parsed.cachedAt === "number" && Number.isFinite(parsed.cachedAt)
					? parsed.cachedAt
					: 0,
		};
	} catch {
		try {
			localStorage.removeItem(BRANDING_CACHE_KEY);
		} catch {
			// ignore storage failures
		}
		return null;
	}
}

function writeCachedBranding(branding: PublicBranding) {
	try {
		localStorage.setItem(
			BRANDING_CACHE_KEY,
			JSON.stringify({
				branding,
				cachedAt: Date.now(),
			} satisfies CachedBrandingPayload),
		);
	} catch {
		// ignore storage failures
	}
}

function clearCachedBranding() {
	try {
		localStorage.removeItem(BRANDING_CACHE_KEY);
	} catch {
		// ignore storage failures
	}
}

function shouldSkipRevalidation(force: boolean, isLoaded: boolean) {
	if (force || !isLoaded) {
		return false;
	}

	return (
		Date.now() - lastRevalidationAttemptAt < BRANDING_REVALIDATE_INTERVAL_MS
	);
}

function applyPublicBranding(publicBranding: PublicBranding) {
	const branding = resolveBranding(publicBranding);
	const siteUrl = setPublicSiteUrls(publicBranding.site_urls);
	applyBranding(branding);
	return {
		allowUserRegistration: publicBranding.allow_user_registration ?? true,
		branding,
		isLoaded: true,
		passkeyLoginEnabled: publicBranding.passkey_login_enabled ?? true,
		siteUrl,
	};
}

const initialCachedBranding = readCachedBranding();
const initialPublicBranding = initialCachedBranding?.branding ?? null;
const initialBranding = resolveBranding(initialPublicBranding);
const initialSiteUrl = initialPublicBranding
	? setPublicSiteUrls(initialPublicBranding.site_urls)
	: null;
if (initialPublicBranding) {
	applyBranding(initialBranding);
}

export const useBrandingStore = create<BrandingState>((set, get) => ({
	allowUserRegistration: initialPublicBranding?.allow_user_registration ?? true,
	branding: initialBranding,
	isLoaded: initialPublicBranding !== null,
	passkeyLoginEnabled: initialPublicBranding?.passkey_login_enabled ?? true,
	siteUrl: initialSiteUrl,

	invalidate: () => {
		clearCachedBranding();
		lastRevalidationAttemptAt = 0;
		set({
			allowUserRegistration: true,
			branding: DEFAULT_BRANDING,
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
				const publicBranding = await brandingService.get();
				writeCachedBranding(publicBranding);
				set(applyPublicBranding(publicBranding));
			} catch (error) {
				logger.warn("branding bootstrap failed, using cached/defaults", error);
				if (get().isLoaded) {
					return;
				}
				const fallbackBranding = resolveBranding(null);
				setPublicSiteUrls(null);
				applyBranding(fallbackBranding);
				set({
					allowUserRegistration: true,
					branding: fallbackBranding,
					isLoaded: true,
					passkeyLoginEnabled: true,
					siteUrl: null,
				});
			} finally {
				inFlightLoad = null;
			}
		})();

		return inFlightLoad;
	},
}));
