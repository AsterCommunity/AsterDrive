import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_BRANDING } from "@/lib/branding";

const mockState = vi.hoisted(() => ({
	applyBranding: vi.fn(),
	get: vi.fn(),
	setPublicSiteUrls: vi.fn(),
	warn: vi.fn(),
}));

vi.mock("@/services/frontendConfigService", () => ({
	frontendConfigService: {
		get: (...args: unknown[]) => mockState.get(...args),
	},
}));

vi.mock("@/lib/branding", async (importOriginal) => {
	const actual = await importOriginal<typeof import("@/lib/branding")>();
	return {
		...actual,
		applyBranding: (...args: unknown[]) => mockState.applyBranding(...args),
	};
});

vi.mock("@/lib/publicSiteUrl", async (importOriginal) => {
	const actual = await importOriginal<typeof import("@/lib/publicSiteUrl")>();
	return {
		...actual,
		setPublicSiteUrls: (...args: unknown[]) =>
			mockState.setPublicSiteUrls(...args),
	};
});

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: (...args: unknown[]) => mockState.warn(...args),
	},
}));

const branding = {
	allow_user_registration: true,
	description: "Private drive",
	favicon_url: "/favicon.ico",
	passkey_login_enabled: true,
	site_urls: ["https://drive.example"],
	title: "AsterDrive",
	wordmark_dark_url: "/wordmark-dark.svg",
	wordmark_light_url: "/wordmark-light.svg",
};

const frontendConfig = {
	branding,
	media: {
		image_preview_preference: "original_first",
	},
	version: 1,
};

const appliedBranding = {
	description: "Private drive",
	faviconUrl: "/favicon.ico",
	title: "AsterDrive",
	wordmarkDarkUrl: "/wordmark-dark.svg",
	wordmarkLightUrl: "/wordmark-light.svg",
};

async function loadStore() {
	vi.resetModules();
	return await import("@/stores/frontendConfigStore");
}

describe("frontendConfigStore", () => {
	beforeEach(() => {
		localStorage.clear();
		mockState.applyBranding.mockReset();
		mockState.get.mockReset();
		mockState.setPublicSiteUrls.mockReset();
		mockState.warn.mockReset();
		mockState.setPublicSiteUrls.mockReturnValue("https://drive.example");
		vi.useRealTimers();
	});

	it("loads frontend config and applies branding", async () => {
		mockState.get.mockResolvedValue(frontendConfig);

		const { useFrontendConfigStore } = await loadStore();

		expect(useFrontendConfigStore.getState().isLoaded).toBe(false);
		expect(useFrontendConfigStore.getState().imagePreviewPreference).toBe(
			"original_first",
		);

		await useFrontendConfigStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);
		expect(mockState.applyBranding).toHaveBeenCalledTimes(1);
		expect(mockState.setPublicSiteUrls).toHaveBeenCalledWith(
			branding.site_urls,
		);
		expect(useFrontendConfigStore.getState().config).toEqual(frontendConfig);
		expect(useFrontendConfigStore.getState().branding).toEqual(appliedBranding);
		expect(useFrontendConfigStore.getState().imagePreviewPreference).toBe(
			"original_first",
		);
		expect(useFrontendConfigStore.getState().isLoaded).toBe(true);
	});

	it("hydrates cached config immediately and revalidates it", async () => {
		const cachedConfig = {
			...frontendConfig,
			media: { image_preview_preference: "preview_first" },
		};
		localStorage.setItem(
			"aster-cached-frontend-config:v1",
			JSON.stringify({ config: cachedConfig, cachedAt: Date.now() }),
		);
		mockState.get.mockResolvedValue(frontendConfig);

		const {
			FRONTEND_CONFIG_CACHE_KEY,
			initFrontendConfigRuntime,
			useFrontendConfigStore,
		} = await loadStore();

		expect(useFrontendConfigStore.getState().config).toEqual(cachedConfig);
		expect(useFrontendConfigStore.getState().imagePreviewPreference).toBe(
			"preview_first",
		);
		expect(mockState.applyBranding).not.toHaveBeenCalled();
		expect(mockState.setPublicSiteUrls).not.toHaveBeenCalled();

		initFrontendConfigRuntime();

		expect(mockState.applyBranding).toHaveBeenCalledTimes(1);
		expect(mockState.setPublicSiteUrls).toHaveBeenCalledWith(
			branding.site_urls,
		);

		await useFrontendConfigStore.getState().load();

		expect(useFrontendConfigStore.getState().config).toEqual(frontendConfig);
		expect(
			JSON.parse(localStorage.getItem(FRONTEND_CONFIG_CACHE_KEY) ?? "null"),
		).toMatchObject({ config: frontendConfig });
	});

	it("drops cached configs with unsupported preview preference", async () => {
		localStorage.setItem(
			"aster-cached-frontend-config:v1",
			JSON.stringify({
				config: {
					...frontendConfig,
					media: { image_preview_preference: "sideways" },
				},
			}),
		);

		const { FRONTEND_CONFIG_CACHE_KEY, useFrontendConfigStore } =
			await loadStore();

		expect(useFrontendConfigStore.getState().config).toBeNull();
		expect(useFrontendConfigStore.getState().isLoaded).toBe(false);
		expect(localStorage.getItem(FRONTEND_CONFIG_CACHE_KEY)).toBeNull();
	});

	it("drops cached configs with invalid branding shape", async () => {
		localStorage.setItem(
			"aster-cached-frontend-config:v1",
			JSON.stringify({
				config: {
					...frontendConfig,
					branding: {
						...branding,
						site_urls: ["https://drive.example", 42],
					},
				},
				cachedAt: Date.now(),
			}),
		);

		const { FRONTEND_CONFIG_CACHE_KEY, useFrontendConfigStore } =
			await loadStore();

		expect(useFrontendConfigStore.getState().config).toBeNull();
		expect(useFrontendConfigStore.getState().imagePreviewPreference).toBe(
			"original_first",
		);
		expect(localStorage.getItem(FRONTEND_CONFIG_CACHE_KEY)).toBeNull();
	});

	it("treats cached payloads without a finite timestamp as stale but usable", async () => {
		const cachedConfig = {
			...frontendConfig,
			media: { image_preview_preference: "preview_first" },
		};
		localStorage.setItem(
			"aster-cached-frontend-config:v1",
			JSON.stringify({ config: cachedConfig, cachedAt: "yesterday" }),
		);

		const { useFrontendConfigStore } = await loadStore();

		expect(useFrontendConfigStore.getState().config).toEqual(cachedConfig);
		expect(useFrontendConfigStore.getState().imagePreviewPreference).toBe(
			"preview_first",
		);
	});

	it("recovers when removing a corrupt cached config also fails", async () => {
		localStorage.setItem("aster-cached-frontend-config:v1", "{bad json");
		const removeItemSpy = vi
			.spyOn(Storage.prototype, "removeItem")
			.mockImplementation(() => {
				throw new Error("storage locked");
			});

		const { useFrontendConfigStore } = await loadStore();

		expect(useFrontendConfigStore.getState().config).toBeNull();
		expect(useFrontendConfigStore.getState().isLoaded).toBe(false);
		removeItemSpy.mockRestore();
	});

	it("uses safe defaults when bootstrap fails without cached config", async () => {
		mockState.get.mockRejectedValueOnce(new Error("offline"));

		const { useFrontendConfigStore } = await loadStore();

		await useFrontendConfigStore.getState().load();

		expect(mockState.warn).toHaveBeenCalledTimes(1);
		expect(mockState.setPublicSiteUrls).toHaveBeenCalledWith(null);
		expect(useFrontendConfigStore.getState().config).toBeNull();
		expect(useFrontendConfigStore.getState().branding).toEqual(
			DEFAULT_BRANDING,
		);
		expect(useFrontendConfigStore.getState().imagePreviewPreference).toBe(
			"original_first",
		);
		expect(useFrontendConfigStore.getState().isLoaded).toBe(true);
	});

	it("keeps a loaded cached config when revalidation fails", async () => {
		localStorage.setItem(
			"aster-cached-frontend-config:v1",
			JSON.stringify({ config: frontendConfig, cachedAt: Date.now() }),
		);
		mockState.get.mockRejectedValueOnce(new Error("offline"));

		const { useFrontendConfigStore } = await loadStore();

		await useFrontendConfigStore.getState().load({ force: true });

		expect(mockState.warn).toHaveBeenCalledTimes(1);
		expect(useFrontendConfigStore.getState().config).toEqual(frontendConfig);
		expect(useFrontendConfigStore.getState().isLoaded).toBe(true);
	});

	it("clears cached config and resets public frontend state on invalidate", async () => {
		mockState.get.mockResolvedValue(frontendConfig);

		const { FRONTEND_CONFIG_CACHE_KEY, useFrontendConfigStore } =
			await loadStore();

		await useFrontendConfigStore.getState().load();
		expect(localStorage.getItem(FRONTEND_CONFIG_CACHE_KEY)).not.toBeNull();

		useFrontendConfigStore.getState().invalidate();

		expect(localStorage.getItem(FRONTEND_CONFIG_CACHE_KEY)).toBeNull();
		expect(useFrontendConfigStore.getState().config).toBeNull();
		expect(useFrontendConfigStore.getState().isLoaded).toBe(false);
		expect(useFrontendConfigStore.getState().siteUrl).toBeNull();
		expect(useFrontendConfigStore.getState().imagePreviewPreference).toBe(
			"original_first",
		);
	});

	it("reuses an in-flight frontend config load", async () => {
		let resolveConfig: (config: typeof frontendConfig) => void = () => {};
		mockState.get.mockImplementation(
			() =>
				new Promise((resolve) => {
					resolveConfig = resolve;
				}),
		);

		const { useFrontendConfigStore } = await loadStore();
		const firstLoad = useFrontendConfigStore.getState().load();
		const secondLoad = useFrontendConfigStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);

		resolveConfig(frontendConfig);
		await Promise.all([firstLoad, secondLoad]);

		expect(useFrontendConfigStore.getState().config).toEqual(frontendConfig);
	});

	it("starts a forced refresh instead of reusing the freshness window", async () => {
		const forcedConfig = {
			...frontendConfig,
			media: { image_preview_preference: "preview_first" },
		};
		mockState.get
			.mockResolvedValueOnce(frontendConfig)
			.mockResolvedValueOnce(forcedConfig);

		const { useFrontendConfigStore } = await loadStore();

		await useFrontendConfigStore.getState().load();
		await useFrontendConfigStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);

		await useFrontendConfigStore.getState().load({ force: true });

		expect(mockState.get).toHaveBeenCalledTimes(2);
		expect(useFrontendConfigStore.getState().imagePreviewPreference).toBe(
			"preview_first",
		);
	});
});
