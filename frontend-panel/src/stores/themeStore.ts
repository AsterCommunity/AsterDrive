import { create } from "zustand";
import { STORAGE_KEYS } from "@/config/app";
import { queuePreferenceSync } from "@/lib/preferenceSync";
import { readLocalStorage, writeLocalStorage } from "@/lib/storage";

const THEME_MODES = {
	light: "light",
	dark: "dark",
	system: "system",
} as const;

const COLOR_PRESETS = {
	blue: "#2563eb",
	green: "#16a34a",
	purple: "#9333ea",
	orange: "#f97316",
} as const;

type ThemeMode = (typeof THEME_MODES)[keyof typeof THEME_MODES];
type ColorPreset = `#${string}`;
type ResolvedTheme = "light" | "dark";

const THEME_MODE_VALUES = Object.values(THEME_MODES);
const HEX_COLOR_RE = /^#[0-9a-f]{6}$/i;

const FALLBACK_THEME_TRANSITION_CLASS = "theme-switching";
const FALLBACK_THEME_TRANSITION_DURATION_MS = 220;

let fallbackThemeTransitionTimer: ReturnType<typeof setTimeout> | null = null;

interface ThemeState {
	mode: ThemeMode;
	colorPreset: ColorPreset;
	resolvedTheme: ResolvedTheme;
	setMode: (mode: ThemeMode) => void;
	setColorPreset: (preset: ColorPreset) => void;
	init: () => void;
	_applyFromServer: (prefs: { mode?: unknown; colorPreset?: unknown }) => void;
}

function isThemeMode(value: unknown): value is ThemeMode {
	return (
		typeof value === "string" && THEME_MODE_VALUES.includes(value as ThemeMode)
	);
}

export function isColorPreset(value: unknown): value is ColorPreset {
	return typeof value === "string" && HEX_COLOR_RE.test(value.trim());
}

function normalizeThemeMode(value: unknown, fallback: ThemeMode): ThemeMode {
	return isThemeMode(value) ? value : fallback;
}

function normalizeColorPreset(
	value: unknown,
	fallback: ColorPreset,
): ColorPreset {
	if (typeof value !== "string") return fallback;
	const normalized = value.trim().toLowerCase();
	if (normalized in COLOR_PRESETS) {
		return COLOR_PRESETS[normalized as keyof typeof COLOR_PRESETS];
	}
	return isColorPreset(normalized) ? (normalized as ColorPreset) : fallback;
}

function getStoredThemeMode(key: string, fallback: ThemeMode): ThemeMode {
	return normalizeThemeMode(readLocalStorage(key), fallback);
}

function getStoredColorPreset(key: string, fallback: ColorPreset): ColorPreset {
	return normalizeColorPreset(readLocalStorage(key), fallback);
}

function prefersDarkMode() {
	if (typeof matchMedia !== "function") return false;
	return matchMedia("(prefers-color-scheme: dark)").matches;
}

function resolveTheme(mode: ThemeMode): ResolvedTheme {
	const isDark = mode === "dark" || (mode === "system" && prefersDarkMode());

	return isDark ? "dark" : "light";
}

function colorToPresetName(color: ColorPreset) {
	for (const [name, presetColor] of Object.entries(COLOR_PRESETS)) {
		if (presetColor === color) return name;
	}
	return "custom";
}

function parseHexColor(hex: ColorPreset) {
	return {
		r: Number.parseInt(hex.slice(1, 3), 16),
		g: Number.parseInt(hex.slice(3, 5), 16),
		b: Number.parseInt(hex.slice(5, 7), 16),
	};
}

function linearizeSrgb(value: number) {
	const channel = value / 255;
	return channel <= 0.03928
		? channel / 12.92
		: ((channel + 0.055) / 1.055) ** 2.4;
}

function relativeLuminance(color: ColorPreset) {
	const { r, g, b } = parseHexColor(color);
	return (
		0.2126 * linearizeSrgb(r) +
		0.7152 * linearizeSrgb(g) +
		0.0722 * linearizeSrgb(b)
	);
}

function readableForeground(color: ColorPreset) {
	return relativeLuminance(color) > 0.46
		? "oklch(0.15 0.018 255)"
		: "oklch(0.985 0 0)";
}

function themeColorVariables(color: ColorPreset, resolvedTheme: ResolvedTheme) {
	const accent =
		resolvedTheme === "dark"
			? `color-mix(in oklab, ${color} 28%, black)`
			: `color-mix(in oklab, ${color} 10%, white)`;
	const sidebarAccent =
		resolvedTheme === "dark"
			? `color-mix(in oklab, ${color} 24%, black)`
			: `color-mix(in oklab, ${color} 9%, white)`;
	const accentForeground =
		resolvedTheme === "dark"
			? "oklch(0.96 0.01 255)"
			: `color-mix(in oklab, ${color} 48%, black)`;

	return {
		"--primary": color,
		"--primary-foreground": readableForeground(color),
		"--accent": accent,
		"--accent-foreground": accentForeground,
		"--ring": color,
		"--chart-1": color,
		"--sidebar-primary": color,
		"--sidebar-accent": sidebarAccent,
		"--sidebar-accent-foreground": accentForeground,
	};
}

function commitTheme(resolvedTheme: ResolvedTheme, preset: ColorPreset) {
	const html = document.documentElement;

	if (resolvedTheme === "dark") {
		html.classList.add("dark");
	} else {
		html.classList.remove("dark");
	}
	html.setAttribute("data-theme", colorToPresetName(preset));
	for (const [name, value] of Object.entries(
		themeColorVariables(preset, resolvedTheme),
	)) {
		html.style.setProperty(name, value);
	}
}

function prefersReducedMotion() {
	if (typeof matchMedia !== "function") return false;
	return matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function clearFallbackThemeTransition() {
	document.documentElement.classList.remove(FALLBACK_THEME_TRANSITION_CLASS);
	if (fallbackThemeTransitionTimer !== null) {
		clearTimeout(fallbackThemeTransitionTimer);
		fallbackThemeTransitionTimer = null;
	}
}

function runThemeTransition(
	updateCallback: () => void,
	options: { animate?: boolean } = {},
) {
	if (
		typeof document === "undefined" ||
		!options.animate ||
		prefersReducedMotion()
	) {
		updateCallback();
		return;
	}

	const html = document.documentElement;
	clearFallbackThemeTransition();
	html.classList.add(FALLBACK_THEME_TRANSITION_CLASS);
	updateCallback();
	fallbackThemeTransitionTimer = setTimeout(() => {
		clearFallbackThemeTransition();
	}, FALLBACK_THEME_TRANSITION_DURATION_MS);
}

function applyTheme(
	mode: ThemeMode,
	preset: ColorPreset,
	options: { animate?: boolean } = {},
): ResolvedTheme {
	const resolvedTheme = resolveTheme(mode);
	runThemeTransition(() => {
		commitTheme(resolvedTheme, preset);
	}, options);
	return resolvedTheme;
}

export type { ColorPreset, ThemeMode };
export { COLOR_PRESETS, THEME_MODES };

const initialMode = getStoredThemeMode(STORAGE_KEYS.themeMode, "system");
const initialColorPreset = getStoredColorPreset(
	STORAGE_KEYS.colorPreset,
	COLOR_PRESETS.blue,
);
const initialResolvedTheme = resolveTheme(initialMode);

export const useThemeStore = create<ThemeState>((set, get) => ({
	mode: initialMode,
	colorPreset: initialColorPreset,
	resolvedTheme: initialResolvedTheme,

	setMode: (mode) => {
		const nextMode = normalizeThemeMode(mode, get().mode);
		if (nextMode !== mode) return;
		writeLocalStorage(STORAGE_KEYS.themeMode, nextMode);
		const resolved = applyTheme(nextMode, get().colorPreset, { animate: true });
		set({ mode: nextMode, resolvedTheme: resolved });
		queuePreferenceSync({ theme_mode: nextMode });
	},

	setColorPreset: (preset) => {
		const nextPreset = normalizeColorPreset(preset, get().colorPreset);
		if (nextPreset !== preset) return;
		writeLocalStorage(STORAGE_KEYS.colorPreset, nextPreset);
		applyTheme(get().mode, nextPreset, { animate: true });
		set({ colorPreset: nextPreset });
		queuePreferenceSync({ color_preset: nextPreset });
	},

	init: () => {
		const { mode, colorPreset } = get();
		const resolved = applyTheme(mode, colorPreset);
		set({ resolvedTheme: resolved });

		if (typeof matchMedia !== "function") return;

		const mq = matchMedia("(prefers-color-scheme: dark)");
		const handler = () => {
			if (get().mode === "system") {
				const r = applyTheme("system", get().colorPreset, { animate: true });
				set({ resolvedTheme: r });
			}
		};
		mq.addEventListener("change", handler);
	},

	_applyFromServer: ({ mode, colorPreset }) => {
		const nextMode = normalizeThemeMode(mode, get().mode);
		const nextColorPreset = normalizeColorPreset(
			colorPreset,
			get().colorPreset,
		);
		writeLocalStorage(STORAGE_KEYS.themeMode, nextMode);
		writeLocalStorage(STORAGE_KEYS.colorPreset, nextColorPreset);
		const resolved = applyTheme(nextMode, nextColorPreset);
		set({
			mode: nextMode,
			colorPreset: nextColorPreset,
			resolvedTheme: resolved,
		});
	},
}));
