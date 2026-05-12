import { beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	queuePreferenceSync: vi.fn(),
}));

vi.mock("@/lib/preferenceSync", () => ({
	queuePreferenceSync: mockState.queuePreferenceSync,
}));

function mockMatchMedia(initialMatches: boolean) {
	let matches = initialMatches;
	let listener: (() => void) | undefined;

	Object.defineProperty(window, "matchMedia", {
		configurable: true,
		writable: true,
		value: vi.fn().mockImplementation(() => ({
			get matches() {
				return matches;
			},
			media: "(prefers-color-scheme: dark)",
			onchange: null,
			addEventListener: vi.fn((_event: string, callback: () => void) => {
				listener = callback;
			}),
			removeEventListener: vi.fn(),
			addListener: vi.fn(),
			removeListener: vi.fn(),
			dispatchEvent: vi.fn(),
		})),
	});

	return {
		setMatches(next: boolean) {
			matches = next;
		},
		trigger() {
			listener?.();
		},
	};
}

async function loadThemeStore() {
	vi.resetModules();
	return await import("@/stores/themeStore");
}

describe("useThemeStore", () => {
	beforeEach(() => {
		localStorage.clear();
		document.documentElement.className = "";
		document.documentElement.removeAttribute("data-theme");
		mockState.queuePreferenceSync.mockReset();
	});

	it("persists and applies dark mode changes", async () => {
		mockMatchMedia(false);
		const { useThemeStore } = await loadThemeStore();

		useThemeStore.getState().setMode("dark");

		expect(useThemeStore.getState()).toMatchObject({
			mode: "dark",
			resolvedTheme: "dark",
		});
		expect(localStorage.getItem("aster-theme-mode")).toBe("dark");
		expect(document.documentElement.classList.contains("dark")).toBe(true);
		expect(document.documentElement.getAttribute("data-theme")).toBe("blue");
		expect(document.documentElement.style.getPropertyValue("--primary")).toBe(
			"#2563eb",
		);
		expect(mockState.queuePreferenceSync).toHaveBeenCalledWith({
			theme_mode: "dark",
		});
	});

	it("reacts to system theme changes after init", async () => {
		localStorage.setItem("aster-theme-mode", "system");
		localStorage.setItem("aster-color-preset", "#16a34a");
		const media = mockMatchMedia(true);
		const { useThemeStore } = await loadThemeStore();

		useThemeStore.getState().init();

		expect(useThemeStore.getState().resolvedTheme).toBe("dark");
		expect(document.documentElement.classList.contains("dark")).toBe(true);
		expect(document.documentElement.getAttribute("data-theme")).toBe("green");

		media.setMatches(false);
		media.trigger();

		expect(useThemeStore.getState().resolvedTheme).toBe("light");
		expect(document.documentElement.classList.contains("dark")).toBe(false);
	});

	it("falls back to defaults when stored theme preferences are invalid", async () => {
		localStorage.setItem("aster-theme-mode", "solarized");
		localStorage.setItem("aster-color-preset", "pink");
		mockMatchMedia(false);

		const { useThemeStore } = await loadThemeStore();

		expect(useThemeStore.getState()).toMatchObject({
			mode: "system",
			colorPreset: "#2563eb",
			resolvedTheme: "light",
		});
	});

	it("falls back to defaults when localStorage theme reads fail", async () => {
		const getItem = vi
			.spyOn(Storage.prototype, "getItem")
			.mockImplementation(() => {
				throw new Error("storage blocked");
			});
		mockMatchMedia(false);

		try {
			const { useThemeStore } = await loadThemeStore();

			expect(useThemeStore.getState()).toMatchObject({
				mode: "system",
				colorPreset: "#2563eb",
				resolvedTheme: "light",
			});
		} finally {
			getItem.mockRestore();
		}
	});

	it("keeps theme updates in memory when localStorage writes fail", async () => {
		const setItem = vi
			.spyOn(Storage.prototype, "setItem")
			.mockImplementation(() => {
				throw new Error("quota exceeded");
			});
		mockMatchMedia(false);

		try {
			const { useThemeStore } = await loadThemeStore();

			expect(() => useThemeStore.getState().setMode("dark")).not.toThrow();

			expect(useThemeStore.getState()).toMatchObject({
				mode: "dark",
				resolvedTheme: "dark",
			});
			expect(document.documentElement.classList.contains("dark")).toBe(true);
			expect(mockState.queuePreferenceSync).toHaveBeenCalledWith({
				theme_mode: "dark",
			});
		} finally {
			setItem.mockRestore();
		}
	});

	it("applies server preferences and persists them locally", async () => {
		mockMatchMedia(false);
		const { useThemeStore } = await loadThemeStore();

		useThemeStore.getState()._applyFromServer({
			mode: "light",
			colorPreset: "#f97316",
		});

		expect(useThemeStore.getState()).toMatchObject({
			mode: "light",
			colorPreset: "#f97316",
			resolvedTheme: "light",
		});
		expect(localStorage.getItem("aster-theme-mode")).toBe("light");
		expect(localStorage.getItem("aster-color-preset")).toBe("#f97316");
		expect(document.documentElement.getAttribute("data-theme")).toBe("orange");
	});

	it("normalizes legacy server color names before applying them", async () => {
		mockMatchMedia(false);
		const { useThemeStore } = await loadThemeStore();

		useThemeStore.getState()._applyFromServer({
			mode: "light",
			colorPreset: "green",
		});

		expect(useThemeStore.getState()).toMatchObject({
			colorPreset: "#16a34a",
			resolvedTheme: "light",
		});
		expect(localStorage.getItem("aster-color-preset")).toBe("#16a34a");
		expect(document.documentElement.getAttribute("data-theme")).toBe("green");
	});

	it("persists arbitrary custom colors", async () => {
		mockMatchMedia(false);
		const { useThemeStore } = await loadThemeStore();

		useThemeStore.getState().setColorPreset("#0f766e");

		expect(useThemeStore.getState().colorPreset).toBe("#0f766e");
		expect(localStorage.getItem("aster-color-preset")).toBe("#0f766e");
		expect(document.documentElement.getAttribute("data-theme")).toBe("custom");
		expect(document.documentElement.style.getPropertyValue("--primary")).toBe(
			"#0f766e",
		);
		expect(mockState.queuePreferenceSync).toHaveBeenCalledWith({
			color_preset: "#0f766e",
		});
	});

	it("normalizes invalid server theme preferences before applying them", async () => {
		mockMatchMedia(false);
		const { useThemeStore } = await loadThemeStore();

		useThemeStore.getState()._applyFromServer({
			mode: "solarized",
			colorPreset: "pink",
		});

		expect(useThemeStore.getState()).toMatchObject({
			mode: "system",
			colorPreset: "#2563eb",
			resolvedTheme: "light",
		});
		expect(localStorage.getItem("aster-theme-mode")).toBe("system");
		expect(localStorage.getItem("aster-color-preset")).toBe("#2563eb");
		expect(document.documentElement.getAttribute("data-theme")).toBe("blue");
	});

	it("derives the initial resolved theme from stored system preference", async () => {
		localStorage.setItem("aster-theme-mode", "system");
		const media = mockMatchMedia(true);
		const { useThemeStore } = await loadThemeStore();

		expect(useThemeStore.getState().resolvedTheme).toBe("dark");

		media.setMatches(false);
		useThemeStore.getState().init();

		expect(document.documentElement.classList.contains("dark")).toBe(false);
	});
});
