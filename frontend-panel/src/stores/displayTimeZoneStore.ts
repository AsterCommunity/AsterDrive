import { create } from "zustand";
import { STORAGE_KEYS } from "@/config/app";
import { queuePreferenceSync } from "@/lib/preferenceSync";

export const DISPLAY_TIME_ZONE_BROWSER = "browser";
const FALLBACK_BROWSER_TIME_ZONE = "UTC";
const DISPLAY_TIME_ZONE_SUPPORT_CACHE = new Map<string, boolean>();

export const COMMON_DISPLAY_TIME_ZONES = [
	"UTC",
	"Asia/Shanghai",
	"Asia/Tokyo",
	"Europe/London",
	"Europe/Berlin",
	"America/Los_Angeles",
	"America/Denver",
	"America/Chicago",
	"America/New_York",
	"Australia/Sydney",
] as const;

function isSupportedDisplayTimeZone(timeZone: string): boolean {
	const cached = DISPLAY_TIME_ZONE_SUPPORT_CACHE.get(timeZone);
	if (cached !== undefined) {
		return cached;
	}

	try {
		new Intl.DateTimeFormat(undefined, { timeZone });
		DISPLAY_TIME_ZONE_SUPPORT_CACHE.set(timeZone, true);
		return true;
	} catch {
		DISPLAY_TIME_ZONE_SUPPORT_CACHE.set(timeZone, false);
		return false;
	}
}

function buildAllDisplayTimeZones(): string[] {
	const browserSupported =
		typeof Intl !== "undefined" && typeof Intl.supportedValuesOf === "function"
			? Intl.supportedValuesOf("timeZone")
			: [];

	return Array.from(
		new Set(
			[...COMMON_DISPLAY_TIME_ZONES, ...browserSupported].filter(
				isSupportedDisplayTimeZone,
			),
		),
	);
}

export const ALL_DISPLAY_TIME_ZONES = buildAllDisplayTimeZones();

export function resolveBrowserTimeZone(): string {
	if (typeof Intl === "undefined") {
		return FALLBACK_BROWSER_TIME_ZONE;
	}

	return (
		Intl.DateTimeFormat().resolvedOptions().timeZone ||
		FALLBACK_BROWSER_TIME_ZONE
	);
}

export function normalizeDisplayTimeZonePreference(
	value?: string | null,
): string {
	if (typeof value !== "string") {
		return DISPLAY_TIME_ZONE_BROWSER;
	}

	const trimmed = value.trim();
	if (!trimmed) {
		return DISPLAY_TIME_ZONE_BROWSER;
	}
	if (trimmed === DISPLAY_TIME_ZONE_BROWSER) {
		return DISPLAY_TIME_ZONE_BROWSER;
	}

	return isSupportedDisplayTimeZone(trimmed)
		? trimmed
		: DISPLAY_TIME_ZONE_BROWSER;
}

export function resolveActiveDisplayTimeZone(
	preference?: string | null,
): string {
	const normalized = normalizeDisplayTimeZonePreference(preference);
	return normalized === DISPLAY_TIME_ZONE_BROWSER
		? resolveBrowserTimeZone()
		: normalized;
}

function getStoredPreference(): string {
	if (typeof localStorage === "undefined") {
		return DISPLAY_TIME_ZONE_BROWSER;
	}

	return normalizeDisplayTimeZonePreference(
		localStorage.getItem(STORAGE_KEYS.displayTimeZone),
	);
}

function setStoredPreference(preference: string) {
	if (typeof localStorage === "undefined") {
		return;
	}

	localStorage.setItem(STORAGE_KEYS.displayTimeZone, preference);
}

interface DisplayTimeZoneState {
	preference: string;
	setPreference: (preference: string) => void;
	_applyFromServer: (preference: string | null | undefined) => void;
}

const initialPreference = getStoredPreference();

export const useDisplayTimeZoneStore = create<DisplayTimeZoneState>(
	(set, get) => ({
		preference: initialPreference,

		setPreference: (preference) => {
			const normalized = normalizeDisplayTimeZonePreference(preference);
			if (normalized === get().preference) {
				return;
			}

			setStoredPreference(normalized);
			set({ preference: normalized });
			queuePreferenceSync({ display_time_zone: normalized });
		},

		_applyFromServer: (preference) => {
			const normalized = normalizeDisplayTimeZonePreference(preference);
			setStoredPreference(normalized);
			set({ preference: normalized });
		},
	}),
);

export function getActiveDisplayTimeZone(): string {
	return resolveActiveDisplayTimeZone(
		useDisplayTimeZoneStore.getState().preference,
	);
}
