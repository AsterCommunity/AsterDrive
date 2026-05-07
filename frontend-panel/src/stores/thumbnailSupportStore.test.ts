import { beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	get: vi.fn(),
	warn: vi.fn(),
}));

vi.mock("@/services/thumbnailSupportService", () => ({
	thumbnailSupportService: {
		get: (...args: unknown[]) => mockState.get(...args),
	},
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: (...args: unknown[]) => mockState.warn(...args),
	},
}));

const supportConfig = {
	version: 1,
	extensions: ["png", "heic"],
};

async function loadStore() {
	vi.resetModules();
	return await import("@/stores/thumbnailSupportStore");
}

describe("thumbnailSupportStore", () => {
	beforeEach(() => {
		localStorage.clear();
		mockState.get.mockReset();
		mockState.warn.mockReset();
		vi.useRealTimers();
	});

	it("loads public thumbnail support once and reuses the loaded state", async () => {
		mockState.get.mockResolvedValue(supportConfig);

		const { useThumbnailSupportStore } = await loadStore();

		expect(useThumbnailSupportStore.getState().config).toBeNull();
		expect(useThumbnailSupportStore.getState().isLoaded).toBe(false);

		await useThumbnailSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);
		expect(useThumbnailSupportStore.getState().config).toEqual(supportConfig);
		expect(useThumbnailSupportStore.getState().isLoaded).toBe(true);

		await useThumbnailSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);
	});

	it("hydrates cached support immediately and revalidates it", async () => {
		const cachedSupport = {
			version: 1,
			extensions: ["png"],
		};
		localStorage.setItem(
			"aster-cached-thumbnail-support",
			JSON.stringify({ config: cachedSupport, cachedAt: Date.now() }),
		);
		mockState.get.mockResolvedValue(supportConfig);

		const { THUMBNAIL_SUPPORT_CACHE_KEY, useThumbnailSupportStore } =
			await loadStore();

		expect(useThumbnailSupportStore.getState().config).toEqual(cachedSupport);
		expect(useThumbnailSupportStore.getState().isLoaded).toBe(true);

		await useThumbnailSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);
		expect(useThumbnailSupportStore.getState().config).toEqual(supportConfig);
		expect(
			JSON.parse(localStorage.getItem(THUMBNAIL_SUPPORT_CACHE_KEY) ?? "null"),
		).toMatchObject({
			config: supportConfig,
		});
	});

	it("keeps failed bootstraps retryable for the next ordinary load", async () => {
		mockState.get
			.mockRejectedValueOnce(new Error("offline"))
			.mockResolvedValueOnce(supportConfig);

		const { useThumbnailSupportStore } = await loadStore();

		await useThumbnailSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);
		expect(mockState.warn).toHaveBeenCalledTimes(1);
		expect(useThumbnailSupportStore.getState().config).toBeNull();
		expect(useThumbnailSupportStore.getState().isLoaded).toBe(false);

		await useThumbnailSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(2);
		expect(useThumbnailSupportStore.getState().config).toEqual(supportConfig);
		expect(useThumbnailSupportStore.getState().isLoaded).toBe(true);
	});

	it("deduplicates concurrent non-forced loads", async () => {
		let resolveLoad!: (value: typeof supportConfig) => void;
		const pendingLoad = new Promise<typeof supportConfig>((resolve) => {
			resolveLoad = resolve;
		});
		mockState.get.mockReturnValueOnce(pendingLoad);

		const { useThumbnailSupportStore } = await loadStore();

		const firstLoad = useThumbnailSupportStore.getState().load();
		const secondLoad = useThumbnailSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);

		resolveLoad(supportConfig);
		await Promise.all([firstLoad, secondLoad]);

		expect(useThumbnailSupportStore.getState().config).toEqual(supportConfig);
		expect(useThumbnailSupportStore.getState().isLoaded).toBe(true);
	});

	it("starts a new forced refresh instead of reusing an existing load", async () => {
		let resolveInitialLoad!: (value: typeof supportConfig) => void;
		const initialLoad = new Promise<typeof supportConfig>((resolve) => {
			resolveInitialLoad = resolve;
		});
		const forcedConfig = {
			version: 1,
			extensions: ["png", "heic", "mp4"],
		};
		mockState.get
			.mockReturnValueOnce(initialLoad)
			.mockResolvedValueOnce(forcedConfig);

		const { useThumbnailSupportStore } = await loadStore();

		const firstLoad = useThumbnailSupportStore.getState().load();
		const forcedLoad = useThumbnailSupportStore
			.getState()
			.load({ force: true });

		expect(mockState.get).toHaveBeenCalledTimes(2);

		resolveInitialLoad(supportConfig);
		await Promise.all([firstLoad, forcedLoad]);

		expect(useThumbnailSupportStore.getState().config).toEqual(forcedConfig);
		expect(useThumbnailSupportStore.getState().isLoaded).toBe(true);
	});

	it("revalidates cached support again after the freshness window", async () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-05-07T00:00:00Z"));
		localStorage.setItem(
			"aster-cached-thumbnail-support",
			JSON.stringify({ config: supportConfig, cachedAt: Date.now() }),
		);
		const refreshedConfig = {
			version: 1,
			extensions: ["png", "heic", "mp4"],
		};
		mockState.get
			.mockResolvedValueOnce(refreshedConfig)
			.mockResolvedValueOnce(supportConfig);

		const { useThumbnailSupportStore } = await loadStore();

		await useThumbnailSupportStore.getState().load();
		await useThumbnailSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);

		vi.advanceTimersByTime(30_001);
		await useThumbnailSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(2);
		expect(useThumbnailSupportStore.getState().config).toEqual(supportConfig);
	});
});
