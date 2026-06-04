import { beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	applyBranding: vi.fn(),
	getBranding: vi.fn(),
	loggerWarn: vi.fn(),
}));

vi.mock("@/services/brandingService", () => ({
	brandingService: {
		get: () => mockState.getBranding(),
	},
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: mockState.loggerWarn,
		error: vi.fn(),
		debug: vi.fn(),
	},
}));

vi.mock("@/lib/branding", async () => {
	const actual =
		await vi.importActual<typeof import("@/lib/branding")>("@/lib/branding");
	return {
		...actual,
		applyBranding: mockState.applyBranding,
	};
});

async function loadBrandingStoreModule() {
	vi.resetModules();
	return await import("@/stores/brandingStore");
}

describe("brandingStore", () => {
	beforeEach(async () => {
		localStorage.clear();
		mockState.applyBranding.mockReset();
		mockState.getBranding.mockReset();
		mockState.loggerWarn.mockReset();
		const { setPublicSiteUrls } = await import("@/lib/publicSiteUrl");
		setPublicSiteUrls(null);
	});

	it("loads public branding once and applies it", async () => {
		mockState.getBranding.mockResolvedValue({
			allow_user_registration: false,
			passkey_login_enabled: false,
			title: "Nebula Drive",
			description: "Team storage",
			favicon_url: "https://cdn.example.com/icon.png",
			wordmark_dark_url: "https://cdn.example.com/wordmark-dark.svg",
			wordmark_light_url: "https://cdn.example.com/wordmark-light.svg",
			site_urls: ["https://drive.example.com", "https://panel.example.com"],
		});

		const { useBrandingStore } = await loadBrandingStoreModule();
		const { getPublicSiteUrl, getPublicSiteUrls } = await import(
			"@/lib/publicSiteUrl"
		);

		await useBrandingStore.getState().load();
		await useBrandingStore.getState().load();

		expect(mockState.getBranding).toHaveBeenCalledTimes(1);
		expect(mockState.applyBranding).toHaveBeenCalledWith(
			expect.objectContaining({
				title: "Nebula Drive",
				description: "Team storage",
				faviconUrl: "https://cdn.example.com/icon.png",
				wordmarkDarkUrl: "https://cdn.example.com/wordmark-dark.svg",
				wordmarkLightUrl: "https://cdn.example.com/wordmark-light.svg",
			}),
		);
		expect(useBrandingStore.getState()).toMatchObject({
			allowUserRegistration: false,
			passkeyLoginEnabled: false,
			isLoaded: true,
			branding: expect.objectContaining({
				title: "Nebula Drive",
				description: "Team storage",
			}),
			siteUrl: "https://drive.example.com",
		});
		expect(getPublicSiteUrl()).toBe("https://drive.example.com");
		expect(getPublicSiteUrls()).toEqual([
			"https://drive.example.com",
			"https://panel.example.com",
		]);
	});

	it("hydrates cached branding immediately and revalidates it", async () => {
		const cachedBranding = {
			allow_user_registration: true,
			passkey_login_enabled: true,
			title: "Cached Drive",
			description: "Cached description",
			favicon_url: "/cached.svg",
			wordmark_dark_url: "/cached-dark.svg",
			wordmark_light_url: "/cached-light.svg",
			site_urls: ["https://cached.example.com"],
		};
		const freshBranding = {
			allow_user_registration: false,
			passkey_login_enabled: false,
			title: "Fresh Drive",
			description: "Fresh description",
			favicon_url: "/fresh.svg",
			wordmark_dark_url: "/fresh-dark.svg",
			wordmark_light_url: "/fresh-light.svg",
			site_urls: ["https://fresh.example.com"],
		};
		localStorage.setItem(
			"aster-cached-branding",
			JSON.stringify({ branding: cachedBranding, cachedAt: Date.now() }),
		);
		mockState.getBranding.mockResolvedValue(freshBranding);

		const { BRANDING_CACHE_KEY, useBrandingStore } =
			await loadBrandingStoreModule();
		const { getPublicSiteUrl } = await import("@/lib/publicSiteUrl");

		expect(useBrandingStore.getState()).toMatchObject({
			allowUserRegistration: true,
			passkeyLoginEnabled: true,
			branding: expect.objectContaining({
				title: "Cached Drive",
			}),
			isLoaded: true,
			siteUrl: "https://cached.example.com",
		});
		expect(getPublicSiteUrl()).toBe("https://cached.example.com");

		await useBrandingStore.getState().load();

		expect(mockState.getBranding).toHaveBeenCalledTimes(1);
		expect(useBrandingStore.getState()).toMatchObject({
			allowUserRegistration: false,
			passkeyLoginEnabled: false,
			branding: expect.objectContaining({
				title: "Fresh Drive",
			}),
			siteUrl: "https://fresh.example.com",
		});
		expect(
			JSON.parse(localStorage.getItem(BRANDING_CACHE_KEY) ?? "null"),
		).toMatchObject({
			branding: freshBranding,
		});
	});

	it("keeps cached branding when revalidation fails", async () => {
		localStorage.setItem(
			"aster-cached-branding",
			JSON.stringify({
				branding: {
					allow_user_registration: true,
					passkey_login_enabled: false,
					title: "Cached Drive",
					description: "Cached description",
					favicon_url: "/cached.svg",
					wordmark_dark_url: "/cached-dark.svg",
					wordmark_light_url: "/cached-light.svg",
					site_urls: ["https://cached.example.com"],
				},
				cachedAt: Date.now(),
			}),
		);
		mockState.getBranding.mockRejectedValueOnce(new Error("offline"));

		const { useBrandingStore } = await loadBrandingStoreModule();

		await useBrandingStore.getState().load();

		expect(mockState.loggerWarn).toHaveBeenCalledTimes(1);
		expect(useBrandingStore.getState()).toMatchObject({
			isLoaded: true,
			passkeyLoginEnabled: false,
			branding: expect.objectContaining({
				title: "Cached Drive",
			}),
		});
	});

	it("treats legacy cached branding without passkey policy as enabled", async () => {
		localStorage.setItem(
			"aster-cached-branding",
			JSON.stringify({
				branding: {
					allow_user_registration: true,
					title: "Legacy Cached Drive",
					description: "Cached description",
					favicon_url: "/cached.svg",
					wordmark_dark_url: "/cached-dark.svg",
					wordmark_light_url: "/cached-light.svg",
					site_urls: ["https://cached.example.com"],
				},
				cachedAt: Date.now(),
			}),
		);
		mockState.getBranding.mockRejectedValueOnce(new Error("offline"));

		const { useBrandingStore } = await loadBrandingStoreModule();

		await useBrandingStore.getState().load();

		expect(useBrandingStore.getState()).toMatchObject({
			allowUserRegistration: true,
			isLoaded: true,
			passkeyLoginEnabled: true,
			branding: expect.objectContaining({
				title: "Legacy Cached Drive",
			}),
		});
	});

	it("falls back to defaults when the public endpoint fails", async () => {
		mockState.getBranding.mockRejectedValue(new Error("network down"));

		const { useBrandingStore } = await loadBrandingStoreModule();
		const { getPublicSiteUrl } = await import("@/lib/publicSiteUrl");

		await useBrandingStore.getState().load();

		expect(mockState.loggerWarn).toHaveBeenCalledTimes(1);
		expect(mockState.applyBranding).toHaveBeenCalledWith(
			expect.objectContaining({
				title: "AsterDrive",
				description: "Self-hosted cloud storage",
				faviconUrl: expect.stringContaining("/favicon.svg"),
				wordmarkDarkUrl: expect.stringContaining(
					"/static/asterdrive/asterdrive-dark.svg",
				),
				wordmarkLightUrl: expect.stringContaining(
					"/static/asterdrive/asterdrive-light.svg",
				),
			}),
		);
		expect(useBrandingStore.getState()).toMatchObject({
			allowUserRegistration: true,
			passkeyLoginEnabled: true,
			isLoaded: true,
			branding: expect.objectContaining({
				title: "AsterDrive",
				description: "Self-hosted cloud storage",
			}),
			siteUrl: null,
		});
		expect(getPublicSiteUrl()).toBeNull();
	});
});
