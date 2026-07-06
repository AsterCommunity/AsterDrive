import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	useManagedAdminList,
	useManagedOffset,
} from "@/hooks/useManagedAdminList";

const mockState = vi.hoisted(() => ({
	fetcher: undefined as undefined | (() => Promise<unknown>),
	items: [] as string[],
	lastDeps: [] as unknown[],
	loading: false,
	reload: vi.fn(),
	setItems: vi.fn(),
	setTotal: vi.fn(),
	total: 0,
}));

vi.mock("@/hooks/useApiList", () => ({
	useApiList: (fetcher: () => Promise<unknown>, deps: unknown[]) => {
		mockState.fetcher = fetcher;
		mockState.lastDeps = deps;
		return mockState;
	},
}));

describe("useManagedAdminList", () => {
	beforeEach(() => {
		mockState.fetcher = undefined;
		mockState.items = [];
		mockState.lastDeps = [];
		mockState.loading = false;
		mockState.reload.mockReset();
		mockState.setItems.mockReset();
		mockState.setTotal.mockReset();
		mockState.total = 0;
	});

	it("clamps an empty page back to the last valid offset", async () => {
		mockState.total = 25;
		const setOffset = vi.fn();

		renderHook(() =>
			useManagedAdminList({
				loadPage: vi.fn(),
				query: { offset: 40, pageSize: 20 },
				setOffset,
			}),
		);

		await waitFor(() => {
			expect(setOffset).toHaveBeenCalledWith(20);
		});
	});

	it("keeps a valid offset unchanged", async () => {
		mockState.total = 25;
		const setOffset = vi.fn();

		renderHook(() =>
			useManagedAdminList({
				loadPage: vi.fn(),
				query: { offset: 20, pageSize: 20 },
				setOffset,
			}),
		);

		await waitFor(() => {
			expect(setOffset).not.toHaveBeenCalled();
		});
	});

	it("does not clamp while loading", async () => {
		mockState.loading = true;
		mockState.total = 25;
		const setOffset = vi.fn();

		renderHook(() =>
			useManagedAdminList({
				loadPage: vi.fn(),
				query: { offset: 40, pageSize: 20 },
				setOffset,
			}),
		);

		await waitFor(() => {
			expect(setOffset).not.toHaveBeenCalled();
		});
	});

	it("does not clamp when the current page has items", async () => {
		mockState.items = ["team"];
		mockState.total = 25;
		const setOffset = vi.fn();

		renderHook(() =>
			useManagedAdminList({
				loadPage: vi.fn(),
				query: { offset: 40, pageSize: 20 },
				setOffset,
			}),
		);

		await waitFor(() => {
			expect(setOffset).not.toHaveBeenCalled();
		});
	});

	it("derives reload dependencies from query values", () => {
		renderHook(() =>
			useManagedAdminList({
				loadPage: vi.fn(),
				query: { offset: 20, pageSize: 10, keyword: "report" },
				setOffset: vi.fn(),
			}),
		);

		expect(mockState.lastDeps).toEqual([20, 10, "report"]);
	});

	it("keeps explicit reload dependencies for non-query inputs", () => {
		renderHook(() =>
			useManagedAdminList({
				deps: ["files"],
				loadPage: vi.fn(),
				query: { offset: 0, pageSize: 20 },
				setOffset: vi.fn(),
			}),
		);

		expect(mockState.lastDeps).toEqual([0, 20, "files"]);
	});

	it("refreshes the fetcher when query-derived dependencies change", async () => {
		const loadPage = vi.fn().mockResolvedValue({ items: [], total: 0 });
		const { rerender } = renderHook(
			({ keyword }) =>
				useManagedAdminList({
					loadPage,
					query: { keyword, offset: 0, pageSize: 20 },
					setOffset: vi.fn(),
				}),
			{ initialProps: { keyword: "draft" } },
		);

		await mockState.fetcher?.();
		rerender({ keyword: "report" });
		await mockState.fetcher?.();

		expect(loadPage).toHaveBeenNthCalledWith(1, {
			keyword: "draft",
			offset: 0,
			pageSize: 20,
		});
		expect(loadPage).toHaveBeenNthCalledWith(2, {
			keyword: "report",
			offset: 0,
			pageSize: 20,
		});
	});
});

describe("useManagedOffset", () => {
	it("bridges SetStateAction updates to managed query offset patches", () => {
		const setQuery = vi.fn();
		const { result } = renderHook(() => useManagedOffset(setQuery));

		act(() => {
			result.current((current) => current + 10);
		});

		const update = setQuery.mock.calls[0]?.[0];
		expect(update({ offset: 5, pageSize: 20 })).toEqual({ offset: 15 });
	});

	it("normalizes direct offset updates", () => {
		const setQuery = vi.fn();
		const { result } = renderHook(() => useManagedOffset(setQuery));

		act(() => {
			result.current(-1.5);
		});

		const update = setQuery.mock.calls[0]?.[0];
		expect(update({ offset: 5, pageSize: 20 })).toEqual({ offset: 0 });
	});
});
