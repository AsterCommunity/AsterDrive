import { beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	get: vi.fn(),
	warn: vi.fn(),
}));

vi.mock("@/services/mediaDataSupportService", () => ({
	mediaDataSupportService: {
		get: (...args: unknown[]) => mockState.get(...args),
	},
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: (...args: unknown[]) => mockState.warn(...args),
	},
}));

const supportConfig = {
	enabled: true,
	kinds: {
		audio: {
			enabled: true,
			extensions: ["mp3", "flac"],
			match: "extensions",
		},
		image: {
			enabled: true,
			extensions: ["jpg"],
			match: "extensions",
		},
		video: {
			enabled: false,
			extensions: [],
			match: "extensions",
		},
	},
	max_source_bytes: 1024,
	version: 1,
};

async function loadStore() {
	vi.resetModules();
	return await import("@/stores/mediaDataSupportStore");
}

describe("mediaDataSupportStore", () => {
	beforeEach(() => {
		localStorage.clear();
		mockState.get.mockReset();
		mockState.warn.mockReset();
		vi.useRealTimers();
	});

	it("loads public media data support once and reuses the loaded state", async () => {
		mockState.get.mockResolvedValue(supportConfig);

		const { useMediaDataSupportStore } = await loadStore();

		expect(useMediaDataSupportStore.getState().config).toBeNull();
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(false);

		await useMediaDataSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);
		expect(useMediaDataSupportStore.getState().config).toEqual(supportConfig);
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(true);

		await useMediaDataSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);
	});

	it("hydrates cached support immediately and revalidates it", async () => {
		const cachedSupport = {
			...supportConfig,
			max_source_bytes: 2048,
		};
		localStorage.setItem(
			"aster-cached-media-data-support:v1",
			JSON.stringify({ config: cachedSupport, cachedAt: Date.now() }),
		);
		mockState.get.mockResolvedValue(supportConfig);

		const { MEDIA_DATA_SUPPORT_CACHE_KEY, useMediaDataSupportStore } =
			await loadStore();

		expect(useMediaDataSupportStore.getState().config).toEqual(cachedSupport);
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(true);

		await useMediaDataSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);
		expect(useMediaDataSupportStore.getState().config).toEqual(supportConfig);
		expect(
			JSON.parse(localStorage.getItem(MEDIA_DATA_SUPPORT_CACHE_KEY) ?? "null"),
		).toMatchObject({
			config: supportConfig,
		});
	});

	it("ignores stale unversioned cached support", async () => {
		const staleCachedSupport = {
			...supportConfig,
			max_source_bytes: 4096,
		};
		localStorage.setItem(
			"aster-cached-media-data-support",
			JSON.stringify({ config: staleCachedSupport, cachedAt: Date.now() }),
		);
		mockState.get.mockResolvedValue(supportConfig);

		const { MEDIA_DATA_SUPPORT_CACHE_KEY, useMediaDataSupportStore } =
			await loadStore();

		expect(MEDIA_DATA_SUPPORT_CACHE_KEY).toBe(
			"aster-cached-media-data-support:v1",
		);
		expect(useMediaDataSupportStore.getState().config).toBeNull();
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(false);

		await useMediaDataSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);
		expect(useMediaDataSupportStore.getState().config).toEqual(supportConfig);
		expect(
			localStorage.getItem("aster-cached-media-data-support"),
		).not.toBeNull();
		expect(
			JSON.parse(localStorage.getItem(MEDIA_DATA_SUPPORT_CACHE_KEY) ?? "null"),
		).toMatchObject({
			config: supportConfig,
		});
	});

	it("drops malformed cached support configs", async () => {
		localStorage.setItem(
			"aster-cached-media-data-support:v1",
			JSON.stringify({
				config: {
					enabled: true,
					kinds: {
						audio: { enabled: true, match: "mime", extensions: ["mp3"] },
					},
					max_source_bytes: 1024,
					version: 1,
				},
			}),
		);

		const { MEDIA_DATA_SUPPORT_CACHE_KEY, useMediaDataSupportStore } =
			await loadStore();

		expect(useMediaDataSupportStore.getState().config).toBeNull();
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(false);
		expect(localStorage.getItem(MEDIA_DATA_SUPPORT_CACHE_KEY)).toBeNull();
	});

	it("drops cached support payloads without a config object", async () => {
		localStorage.setItem(
			"aster-cached-media-data-support:v1",
			JSON.stringify({ config: null, cachedAt: Date.now() }),
		);

		const { MEDIA_DATA_SUPPORT_CACHE_KEY, useMediaDataSupportStore } =
			await loadStore();

		expect(useMediaDataSupportStore.getState().config).toBeNull();
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(false);
		expect(localStorage.getItem(MEDIA_DATA_SUPPORT_CACHE_KEY)).toBeNull();
	});

	it("drops unparseable cached support payloads", async () => {
		localStorage.setItem("aster-cached-media-data-support:v1", "{bad json");

		const { MEDIA_DATA_SUPPORT_CACHE_KEY, useMediaDataSupportStore } =
			await loadStore();

		expect(useMediaDataSupportStore.getState().config).toBeNull();
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(false);
		expect(localStorage.getItem(MEDIA_DATA_SUPPORT_CACHE_KEY)).toBeNull();
	});

	it("invalidates loaded support and clears the versioned cache", async () => {
		mockState.get.mockResolvedValue(supportConfig);

		const { MEDIA_DATA_SUPPORT_CACHE_KEY, useMediaDataSupportStore } =
			await loadStore();

		await useMediaDataSupportStore.getState().load();
		expect(localStorage.getItem(MEDIA_DATA_SUPPORT_CACHE_KEY)).not.toBeNull();

		useMediaDataSupportStore.getState().invalidate();

		expect(localStorage.getItem(MEDIA_DATA_SUPPORT_CACHE_KEY)).toBeNull();
		expect(useMediaDataSupportStore.getState().config).toBeNull();
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(false);
	});

	it("ignores an in-flight load after invalidation", async () => {
		let resolveLoad!: (value: typeof supportConfig) => void;
		const pendingLoad = new Promise<typeof supportConfig>((resolve) => {
			resolveLoad = resolve;
		});
		mockState.get.mockReturnValueOnce(pendingLoad);

		const { useMediaDataSupportStore } = await loadStore();

		const load = useMediaDataSupportStore.getState().load();
		useMediaDataSupportStore.getState().invalidate();
		resolveLoad(supportConfig);
		await load;

		expect(useMediaDataSupportStore.getState().config).toBeNull();
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(false);
	});

	it("keeps failed bootstraps retryable for the next ordinary load", async () => {
		mockState.get
			.mockRejectedValueOnce(new Error("offline"))
			.mockResolvedValueOnce(supportConfig);

		const { useMediaDataSupportStore } = await loadStore();

		await useMediaDataSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);
		expect(mockState.warn).toHaveBeenCalledTimes(1);
		expect(useMediaDataSupportStore.getState().config).toBeNull();
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(false);

		await useMediaDataSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(2);
		expect(useMediaDataSupportStore.getState().config).toEqual(supportConfig);
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(true);
	});

	it("deduplicates concurrent non-forced loads", async () => {
		let resolveLoad!: (value: typeof supportConfig) => void;
		const pendingLoad = new Promise<typeof supportConfig>((resolve) => {
			resolveLoad = resolve;
		});
		mockState.get.mockReturnValueOnce(pendingLoad);

		const { useMediaDataSupportStore } = await loadStore();

		const firstLoad = useMediaDataSupportStore.getState().load();
		const secondLoad = useMediaDataSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);

		resolveLoad(supportConfig);
		await Promise.all([firstLoad, secondLoad]);

		expect(useMediaDataSupportStore.getState().config).toEqual(supportConfig);
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(true);
	});

	it("starts a new forced refresh instead of reusing an existing load", async () => {
		let resolveInitialLoad!: (value: typeof supportConfig) => void;
		const initialLoad = new Promise<typeof supportConfig>((resolve) => {
			resolveInitialLoad = resolve;
		});
		const forcedConfig = {
			...supportConfig,
			max_source_bytes: 2048,
		};
		mockState.get
			.mockReturnValueOnce(initialLoad)
			.mockResolvedValueOnce(forcedConfig);

		const { useMediaDataSupportStore } = await loadStore();

		const firstLoad = useMediaDataSupportStore.getState().load();
		const forcedLoad = useMediaDataSupportStore
			.getState()
			.load({ force: true });

		expect(mockState.get).toHaveBeenCalledTimes(2);

		resolveInitialLoad(supportConfig);
		await Promise.all([firstLoad, forcedLoad]);

		expect(useMediaDataSupportStore.getState().config).toEqual(forcedConfig);
		expect(useMediaDataSupportStore.getState().isLoaded).toBe(true);
	});

	it("revalidates cached support again after the freshness window", async () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-05-07T00:00:00Z"));
		localStorage.setItem(
			"aster-cached-media-data-support:v1",
			JSON.stringify({ config: supportConfig, cachedAt: Date.now() }),
		);
		const refreshedConfig = {
			...supportConfig,
			max_source_bytes: 2048,
		};
		mockState.get
			.mockResolvedValueOnce(refreshedConfig)
			.mockResolvedValueOnce(supportConfig);

		const { useMediaDataSupportStore } = await loadStore();

		await useMediaDataSupportStore.getState().load();
		await useMediaDataSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(1);

		vi.advanceTimersByTime(30_001);
		await useMediaDataSupportStore.getState().load();

		expect(mockState.get).toHaveBeenCalledTimes(2);
		expect(useMediaDataSupportStore.getState().config).toEqual(supportConfig);
	});
});
