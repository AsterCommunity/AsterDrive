import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	get: vi.fn(),
	warn: vi.fn(),
}));

vi.mock("@/services/http", () => ({
	api: {
		client: {
			get: mockState.get,
		},
	},
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: mockState.warn,
	},
}));

async function loadHookModule() {
	vi.resetModules();
	return await import("@/hooks/useBlobUrl");
}

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (reason?: unknown) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

class MockCache {
	deleteCalls: string[] = [];
	store = new Map<string, Response>();

	async match(request: Request) {
		return this.store.get(request.url)?.clone();
	}

	async put(request: Request, response: Response) {
		this.store.set(request.url, response.clone());
	}

	async delete(request: Request) {
		this.deleteCalls.push(request.url);
		return this.store.delete(request.url);
	}

	clear() {
		this.store.clear();
	}
}

function installCacheStorage(cache = new MockCache()) {
	const cacheStorage = {
		delete: vi.fn(async () => {
			cache.clear();
			return true;
		}),
		open: vi.fn(async () => cache),
	};
	Object.defineProperty(globalThis, "caches", {
		configurable: true,
		value: cacheStorage,
	});
	return { cache, cacheStorage };
}

function installBlobStreamPolyfill() {
	if (typeof Blob.prototype.stream === "function") return;
	Object.defineProperty(Blob.prototype, "stream", {
		configurable: true,
		value(this: Blob) {
			return new ReadableStream<Uint8Array>({
				start: async (controller) => {
					controller.enqueue(new Uint8Array(await this.arrayBuffer()));
					controller.close();
				},
			});
		},
	});
}

describe("useBlobUrl", () => {
	beforeEach(() => {
		vi.useRealTimers();
		installBlobStreamPolyfill();
		localStorage.clear();
		mockState.get.mockReset();
		mockState.warn.mockReset();
		Object.defineProperty(globalThis, "caches", {
			configurable: true,
			value: undefined,
		});
		Object.defineProperty(URL, "createObjectURL", {
			configurable: true,
			value: vi
				.fn()
				.mockReturnValueOnce("blob:1")
				.mockReturnValueOnce("blob:2")
				.mockReturnValue("blob:3"),
		});
		Object.defineProperty(URL, "revokeObjectURL", {
			configurable: true,
			value: vi.fn(),
		});
	});

	it("loads blob URLs once and reuses the cache for concurrent consumers", async () => {
		const imageBlob = new Blob(["image"]);
		mockState.get.mockResolvedValue({
			status: 200,
			data: imageBlob,
			headers: { etag: '"etag-1"' },
		});
		const { clearBlobUrlCache, useBlobUrl } = await loadHookModule();

		const first = renderHook(() => useBlobUrl("/thumb"));
		await waitFor(() => {
			expect(first.result.current.blobUrl).toBe("blob:1");
			expect(first.result.current.blob).toBe(imageBlob);
		});

		const second = renderHook(() => useBlobUrl("/thumb"));
		await waitFor(() => {
			expect(second.result.current.blobUrl).toBe("blob:1");
			expect(second.result.current.blob).toBe(imageBlob);
		});

		expect(mockState.get).toHaveBeenCalledTimes(1);

		first.unmount();
		second.unmount();
		clearBlobUrlCache();

		expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:1");
	});

	it("retries thumbnail generation after 202 responses", async () => {
		mockState.get
			.mockResolvedValueOnce({
				status: 202,
				data: new Blob([]),
				headers: { "retry-after": "0.001" },
			})
			.mockResolvedValueOnce({
				status: 200,
				data: new Blob(["image"]),
				headers: { etag: '"etag-2"' },
			});
		const { clearBlobUrlCache, useBlobUrl } = await loadHookModule();

		const { result } = renderHook(() =>
			useBlobUrl("/thumb", { lane: "thumbnail" }),
		);

		await waitFor(() => {
			expect(mockState.get).toHaveBeenCalledTimes(2);
		});
		await waitFor(() => {
			expect(result.current.blobUrl).toBe("blob:1");
		});
		clearBlobUrlCache();
	});

	it("keeps polling thumbnails after the first 202 retry window is exhausted", async () => {
		for (let attempt = 0; attempt < 6; attempt += 1) {
			mockState.get.mockResolvedValueOnce({
				status: 202,
				data: new Blob([]),
				headers: { "retry-after": "0.001" },
			});
		}
		mockState.get.mockResolvedValueOnce({
			status: 200,
			data: new Blob(["image"]),
			headers: { etag: '"etag-202"' },
		});
		const { clearBlobUrlCache, useBlobUrl } = await loadHookModule();

		const { result } = renderHook(() =>
			useBlobUrl("/thumb", { lane: "thumbnail" }),
		);

		await waitFor(() => {
			expect(mockState.get).toHaveBeenCalledTimes(7);
		});
		await waitFor(() => {
			expect(result.current.blobUrl).toBe("blob:1");
		});
		expect(result.current.error).toBe(false);
		clearBlobUrlCache();
	});

	it("exposes errors and allows retries after failures", async () => {
		mockState.get
			.mockRejectedValueOnce(new Error("fetch failed"))
			.mockResolvedValueOnce({
				status: 200,
				data: new Blob(["image"]),
				headers: { etag: '"etag-3"' },
			});
		const { clearBlobUrlCache, useBlobUrl } = await loadHookModule();

		const { result } = renderHook(() =>
			useBlobUrl("/thumb", { lane: "thumbnail" }),
		);

		await waitFor(() => {
			expect(result.current.error).toBe(true);
		});
		expect(mockState.warn).toHaveBeenCalledWith(
			"blob fetch failed",
			"/thumb",
			expect.any(Error),
		);

		result.current.retry();

		await waitFor(() => {
			expect(result.current.blobUrl).toBe("blob:1");
		});
		expect(result.current.error).toBe(false);
		clearBlobUrlCache();
	});

	it("treats thumbnail 404 responses as a cacheable missing blob without warning", async () => {
		mockState.get.mockResolvedValue({
			status: 404,
			data: new Blob([]),
			headers: {},
		});
		const { clearBlobUrlCache, useBlobUrl } = await loadHookModule();

		const first = renderHook(() => useBlobUrl("/thumb", { lane: "thumbnail" }));

		await waitFor(() => {
			expect(first.result.current.loading).toBe(false);
		});
		expect(first.result.current.blobUrl).toBeNull();
		expect(first.result.current.error).toBe(false);
		expect(mockState.warn).not.toHaveBeenCalled();

		first.unmount();

		const second = renderHook(() =>
			useBlobUrl("/thumb", { lane: "thumbnail" }),
		);
		await waitFor(() => {
			expect(second.result.current.loading).toBe(false);
		});
		expect(second.result.current.blobUrl).toBeNull();
		expect(second.result.current.error).toBe(false);
		expect(mockState.get).toHaveBeenCalledTimes(1);

		second.unmount();
		clearBlobUrlCache();
	});

	it("revalidates cached blobs with etags and keeps the same object url on 304", async () => {
		const imageBlob = new Blob(["image"]);
		mockState.get
			.mockResolvedValueOnce({
				status: 200,
				data: imageBlob,
				headers: { etag: '"etag-4"' },
			})
			.mockResolvedValueOnce({
				status: 304,
				data: new Blob([]),
				headers: {},
			});
		const { clearBlobUrlCache, useBlobUrl } = await loadHookModule();

		const first = renderHook(() => useBlobUrl("/thumb"));
		await waitFor(() => {
			expect(first.result.current.blobUrl).toBe("blob:1");
		});
		first.unmount();

		const second = renderHook(() => useBlobUrl("/thumb"));
		await waitFor(() => {
			expect(second.result.current.blobUrl).toBe("blob:1");
			expect(second.result.current.blob).toBe(imageBlob);
		});

		expect(mockState.get).toHaveBeenNthCalledWith(2, "/thumb", {
			headers: { "If-None-Match": '"etag-4"' },
			responseType: "blob",
			validateStatus: expect.any(Function),
		});
		expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
		clearBlobUrlCache();
	});

	it("keeps thumbnail blob urls for the whole session after the first successful fetch", async () => {
		mockState.get.mockResolvedValue({
			status: 200,
			data: new Blob(["image"]),
			headers: { etag: '"etag-5"' },
		});
		const { clearBlobUrlCache, useBlobUrl } = await loadHookModule();

		const first = renderHook(() => useBlobUrl("/thumb", { lane: "thumbnail" }));
		await waitFor(() => {
			expect(first.result.current.blobUrl).toBe("blob:1");
		});
		first.unmount();

		const second = renderHook(() =>
			useBlobUrl("/thumb", { lane: "thumbnail" }),
		);
		await waitFor(() => {
			expect(second.result.current.blobUrl).toBe("blob:1");
		});

		expect(mockState.get).toHaveBeenCalledTimes(1);
		expect(URL.createObjectURL).toHaveBeenCalledTimes(1);

		second.unmount();
		clearBlobUrlCache();
	});

	it("restores persisted thumbnail blobs before background revalidation finishes", async () => {
		const { cacheStorage } = installCacheStorage();
		const revalidation = deferred<{
			status: number;
			data: Blob;
			headers: Record<string, string>;
		}>();
		mockState.get
			.mockResolvedValueOnce({
				status: 200,
				data: new Blob(["persisted-image"]),
				headers: { etag: '"etag-persisted"' },
			})
			.mockImplementationOnce(() => revalidation.promise);
		let module = await loadHookModule();

		const first = renderHook(() =>
			module.useBlobUrl("/thumb", { lane: "thumbnail" }),
		);
		await waitFor(() => {
			expect(first.result.current.blobUrl).toBe("blob:1");
		});
		expect(cacheStorage.open).toHaveBeenCalled();
		expect(mockState.warn).not.toHaveBeenCalled();
		first.unmount();
		module.clearBlobUrlCache();

		module = await loadHookModule();
		const second = renderHook(() =>
			module.useBlobUrl("/thumb", { lane: "thumbnail" }),
		);

		await waitFor(() => {
			expect(second.result.current.blobUrl).toBe("blob:2");
			expect(second.result.current.loading).toBe(false);
		});
		expect(mockState.get).toHaveBeenCalledTimes(2);
		expect(mockState.get).toHaveBeenNthCalledWith(2, "/thumb", {
			headers: { "If-None-Match": '"etag-persisted"' },
			responseType: "blob",
			validateStatus: expect.any(Function),
		});

		revalidation.resolve({
			status: 304,
			data: new Blob([]),
			headers: {},
		});

		await waitFor(() => {
			expect(URL.createObjectURL).toHaveBeenCalledTimes(2);
		});
		second.unmount();
		module.clearBlobUrlCache();
		await module.clearPersistedBlobUrlCache();
	});

	it("namespaces persisted thumbnail blobs by the current user", async () => {
		const { cache } = installCacheStorage();
		localStorage.setItem("aster-cached-user", JSON.stringify({ id: 1 }));
		mockState.get.mockResolvedValueOnce({
			status: 200,
			data: new Blob(["user-1-image"]),
			headers: { etag: '"etag-user-1"' },
		});
		let module = await loadHookModule();

		const first = renderHook(() =>
			module.useBlobUrl("/thumb", { lane: "thumbnail" }),
		);
		await waitFor(() => {
			expect(first.result.current.blobUrl).toBe("blob:1");
		});
		first.unmount();
		module.clearBlobUrlCache();

		localStorage.setItem("aster-cached-user", JSON.stringify({ id: 2 }));
		mockState.get.mockResolvedValueOnce({
			status: 200,
			data: new Blob(["user-2-image"]),
			headers: { etag: '"etag-user-2"' },
		});
		module = await loadHookModule();

		const second = renderHook(() =>
			module.useBlobUrl("/thumb", { lane: "thumbnail" }),
		);
		await waitFor(() => {
			expect(second.result.current.blobUrl).toBe("blob:2");
		});

		expect(mockState.get).toHaveBeenCalledTimes(2);
		expect(cache.store.size).toBe(2);
		expect(
			[...cache.store.keys()].some((key) => key.includes("user%3A1")),
		).toBe(true);
		expect(
			[...cache.store.keys()].some((key) => key.includes("user%3A2")),
		).toBe(true);

		second.unmount();
		module.clearBlobUrlCache();
		await module.clearPersistedBlobUrlCache();
	});

	it("does not create orphan object URLs after the cache entry is revoked", async () => {
		vi.useFakeTimers();
		const response = deferred<{
			status: number;
			data: Blob;
			headers: Record<string, string>;
		}>();
		mockState.get.mockReturnValueOnce(response.promise);
		const { clearBlobUrlCache, useBlobUrl } = await loadHookModule();

		const hook = renderHook(() => useBlobUrl("/thumb"));
		await act(async () => {
			await Promise.resolve();
		});
		expect(mockState.get).toHaveBeenCalledTimes(1);

		hook.unmount();
		await act(async () => {
			vi.advanceTimersByTime(30_000);
		});

		await act(async () => {
			response.resolve({
				status: 200,
				data: new Blob(["image"]),
				headers: { etag: '"etag-orphan"' },
			});
			await response.promise;
			await Promise.resolve();
		});

		expect(URL.createObjectURL).not.toHaveBeenCalled();
		clearBlobUrlCache();
		vi.useRealTimers();
	});

	it("stays idle when no path is provided", async () => {
		const { clearBlobUrlCache, useBlobUrl } = await loadHookModule();

		const { result } = renderHook(() => useBlobUrl(null));

		expect(result.current.blobUrl).toBeNull();
		expect(result.current.error).toBe(false);
		expect(result.current.loading).toBe(false);
		expect(mockState.get).not.toHaveBeenCalled();
		clearBlobUrlCache();
	});

	it("re-fetches active consumers after explicit invalidation", async () => {
		const { cache, cacheStorage } = installCacheStorage();
		mockState.get
			.mockResolvedValueOnce({
				status: 200,
				data: new Blob(["image-v1"]),
				headers: { etag: '"etag-1"' },
			})
			.mockResolvedValueOnce({
				status: 200,
				data: new Blob(["image-v2"]),
				headers: { etag: '"etag-2"' },
			});
		const { clearBlobUrlCache, invalidateBlobUrl, useBlobUrl } =
			await loadHookModule();

		const { result } = renderHook(() =>
			useBlobUrl("/thumb", { lane: "thumbnail" }),
		);

		await waitFor(() => {
			expect(result.current.blobUrl).toBe("blob:1");
		});
		expect(cacheStorage.open).toHaveBeenCalled();
		expect(mockState.warn).not.toHaveBeenCalled();
		await waitFor(() => {
			expect(cache.store.size).toBe(1);
		});

		act(() => {
			invalidateBlobUrl("/thumb");
		});
		await waitFor(() => {
			expect(cache.deleteCalls).toHaveLength(1);
		});

		await waitFor(() => {
			expect(mockState.get).toHaveBeenCalledTimes(2);
		});
		await waitFor(() => {
			expect(result.current.blobUrl).toBe("blob:2");
		});
		expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:1");
		clearBlobUrlCache();
	});

	it("serializes thumbnail fetches one by one", async () => {
		const firstResponse = deferred<{
			status: number;
			data: Blob;
			headers: Record<string, string>;
		}>();
		const secondResponse = deferred<{
			status: number;
			data: Blob;
			headers: Record<string, string>;
		}>();
		mockState.get
			.mockImplementationOnce(() => firstResponse.promise)
			.mockImplementationOnce(() => secondResponse.promise);
		const { clearBlobUrlCache, useBlobUrl } = await loadHookModule();

		const first = renderHook(() =>
			useBlobUrl("/thumb-1", { lane: "thumbnail" }),
		);
		const second = renderHook(() =>
			useBlobUrl("/thumb-2", { lane: "thumbnail" }),
		);

		await waitFor(() => {
			expect(mockState.get).toHaveBeenCalledTimes(1);
		});
		expect(mockState.get).toHaveBeenNthCalledWith(1, "/thumb-1", {
			headers: {},
			responseType: "blob",
			validateStatus: expect.any(Function),
		});

		firstResponse.resolve({
			status: 200,
			data: new Blob(["image-1"]),
			headers: { etag: '"etag-1"' },
		});

		await waitFor(() => {
			expect(first.result.current.blobUrl).toBe("blob:1");
		});
		await waitFor(() => {
			expect(mockState.get).toHaveBeenCalledTimes(2);
		});
		expect(mockState.get).toHaveBeenNthCalledWith(2, "/thumb-2", {
			headers: {},
			responseType: "blob",
			validateStatus: expect.any(Function),
		});

		secondResponse.resolve({
			status: 200,
			data: new Blob(["image-2"]),
			headers: { etag: '"etag-2"' },
		});

		await waitFor(() => {
			expect(second.result.current.blobUrl).toBe("blob:2");
		});

		first.unmount();
		second.unmount();
		clearBlobUrlCache();
	});
});
