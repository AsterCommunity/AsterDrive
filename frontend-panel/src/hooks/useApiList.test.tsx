import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: mockState.handleApiError,
}));

function createDeferred<T>() {
	let reject!: (reason?: unknown) => void;
	let resolve!: (value: T | PromiseLike<T>) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});

	return { promise, reject, resolve };
}

describe("useApiList", () => {
	beforeEach(() => {
		mockState.handleApiError.mockReset();
	});

	it("loads list data on mount and exposes reload", async () => {
		const fetcher = vi
			.fn()
			.mockResolvedValueOnce({ items: ["a"], total: 1 })
			.mockResolvedValueOnce({ items: ["b"], total: 2 });
		const { useApiList } = await import("@/hooks/useApiList");
		const { result } = renderHook(() => useApiList(fetcher));

		await waitFor(() => {
			expect(result.current.loading).toBe(false);
		});
		expect(result.current.items).toEqual(["a"]);
		expect(result.current.total).toBe(1);

		await result.current.reload();

		await waitFor(() => {
			expect(result.current.items).toEqual(["b"]);
		});
		expect(result.current.total).toBe(2);
		expect(fetcher).toHaveBeenCalledTimes(2);
	});

	it("reports fetch failures through handleApiError", async () => {
		const failure = new Error("load failed");
		const fetcher = vi.fn().mockRejectedValue(failure);
		const { useApiList } = await import("@/hooks/useApiList");
		const { result } = renderHook(() => useApiList(fetcher));

		await waitFor(() => {
			expect(result.current.loading).toBe(false);
		});
		expect(result.current.items).toEqual([]);
		expect(result.current.total).toBe(0);
		expect(mockState.handleApiError).toHaveBeenCalledWith(failure);
	});

	it("ignores stale responses when a newer load finishes first", async () => {
		const first = createDeferred<{ items: string[]; total: number }>();
		const second = createDeferred<{ items: string[]; total: number }>();
		const fetcher = vi
			.fn()
			.mockImplementationOnce(() => first.promise)
			.mockImplementationOnce(() => second.promise);
		const { useApiList } = await import("@/hooks/useApiList");
		const { result } = renderHook(() => useApiList(fetcher));

		const reloadPromise = result.current.reload();

		second.resolve({ items: ["new"], total: 1 });
		await waitFor(() => {
			expect(result.current.loading).toBe(false);
		});
		expect(result.current.items).toEqual(["new"]);
		expect(result.current.total).toBe(1);

		first.resolve({ items: ["old"], total: 99 });
		await Promise.resolve();
		await reloadPromise;

		expect(result.current.items).toEqual(["new"]);
		expect(result.current.total).toBe(1);
		expect(mockState.handleApiError).not.toHaveBeenCalled();
	});
	it("ignores a rejected request after unmount", async () => {
		const deferred = createDeferred<{ items: string[]; total: number }>();
		const { useApiList } = await import("@/hooks/useApiList");
		const { unmount } = renderHook(() => useApiList(() => deferred.promise));
		unmount();
		deferred.reject(new Error("late failure"));
		await Promise.resolve();
		expect(mockState.handleApiError).not.toHaveBeenCalled();
	});
});
