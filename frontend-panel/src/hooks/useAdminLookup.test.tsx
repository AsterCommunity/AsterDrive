import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAdminLookup } from "@/hooks/useAdminLookup";

const mocks = vi.hoisted(() => ({ get: vi.fn(), search: vi.fn() }));
vi.mock("@/services/adminLookupService", () => ({ adminLookupService: mocks }));
beforeEach(() => {
	mocks.get.mockReset();
	mocks.search.mockReset();
});
describe("useAdminLookup", () => {
	it("keeps a failed selected identity retryable after a successful search", async () => {
		const selected = { id: 99, label: "Archive", description: "#99" };
		mocks.get
			.mockRejectedValueOnce(new Error("offline"))
			.mockResolvedValue(selected);
		mocks.search.mockResolvedValue({ items: [], total: 0 });
		const { result, rerender } = renderHook(
			({ open }) => useAdminLookup("policies", 99, open),
			{ initialProps: { open: false } },
		);
		await waitFor(() => expect(result.current.failed).toBe(true));
		rerender({ open: true });
		await waitFor(() => expect(result.current.loading).toBe(false));
		expect(result.current.failed).toBe(true);
		act(() => result.current.retry());
		await waitFor(() => expect(result.current.selected).toEqual(selected));
		expect(result.current.failed).toBe(false);
	});
	it("restores a selected label outside the loaded page and preserves remote pagination", async () => {
		const selected = { id: 99, label: "Archive", description: "#99" };
		mocks.get.mockResolvedValue(selected);
		mocks.search
			.mockResolvedValueOnce({
				items: [{ id: 1, label: "First", description: "#1" }],
				total: 2,
			})
			.mockResolvedValueOnce({
				items: [{ id: 2, label: "Second", description: "#2" }],
				total: 2,
			});
		const { result } = renderHook(() => useAdminLookup("policies", 99, true));
		await waitFor(() => expect(result.current.loading).toBe(false));
		expect(result.current.selected).toEqual(selected);
		expect(result.current.options.map((item) => item.id)).toEqual([99, 1]);
		act(() => result.current.loadMore());
		await waitFor(() => expect(result.current.options).toHaveLength(3));
		expect(mocks.search).toHaveBeenLastCalledWith(
			"policies",
			"",
			1,
			expect.any(AbortSignal),
		);
		expect(result.current.hasMore).toBe(false);
	});
	it("aborts stale requests on typing, closing and unmount and ignores late results", async () => {
		let finish!: (value: unknown) => void;
		mocks.search
			.mockImplementationOnce(
				() =>
					new Promise((resolve) => {
						finish = resolve;
					}),
			)
			.mockResolvedValue({ items: [], total: 0 });
		const { result, rerender, unmount } = renderHook(
			({ open }) => useAdminLookup("users", undefined, open),
			{ initialProps: { open: true } },
		);
		const firstSignal = mocks.search.mock.calls[0]?.[3] as AbortSignal;
		act(() => result.current.setSearch("new"));
		expect(firstSignal.aborted).toBe(true);
		await act(async () =>
			finish({ items: [{ id: 1, label: "stale", description: "" }], total: 1 }),
		);
		expect(result.current.options).toEqual([]);
		await waitFor(() => expect(mocks.search).toHaveBeenCalledTimes(2));
		const signal = mocks.search.mock.calls[1]?.[3] as AbortSignal;
		rerender({ open: false });
		expect(signal.aborted).toBe(true);
		rerender({ open: true });
		const finalSignal = mocks.search.mock.lastCall?.[3] as AbortSignal;
		unmount();
		expect(finalSignal.aborted).toBe(true);
	});
});
