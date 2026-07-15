import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FolderContents, FolderListItem } from "@/types/api";
import { useShareFolderTree } from "./useShareFolderTree";

const mockState = vi.hoisted(() => ({
	listContent: vi.fn(),
	listSubfolderContent: vi.fn(),
}));

vi.mock("@/services/shareService", () => ({
	shareService: {
		listContent: (...args: unknown[]) => mockState.listContent(...args),
		listSubfolderContent: (...args: unknown[]) =>
			mockState.listSubfolderContent(...args),
	},
}));

function folder(id: number, name: string): FolderListItem {
	return {
		id,
		is_locked: false,
		is_shared: false,
		name,
		tags: [],
		updated_at: "2026-07-15T00:00:00Z",
	};
}

function contents(folders: FolderListItem[]): FolderContents {
	return {
		files: [],
		folders,
		folders_total: folders.length,
		files_total: 0,
		next_file_cursor: null,
	} as FolderContents;
}

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (reason?: unknown) => void;
	const promise = new Promise<T>((next, fail) => {
		resolve = next;
		reject = fail;
	});
	return { promise, reject, resolve };
}

describe("useShareFolderTree", () => {
	beforeEach(() => {
		mockState.listContent.mockReset();
		mockState.listSubfolderContent.mockReset();
		mockState.listContent.mockResolvedValue(contents([]));
		mockState.listSubfolderContent.mockResolvedValue(contents([]));
	});

	it("reuses the visible root contents instead of requesting them again", async () => {
		const docs = folder(1, "Docs");
		const { result } = renderHook(() =>
			useShareFolderTree({
				breadcrumb: [{ id: null, name: "Shared Root" }],
				folderContents: contents([docs]),
				token: "share-a",
			}),
		);

		await waitFor(() => expect(result.current.rootIds).toEqual([1]));
		expect(result.current.loadedKeys.has("root")).toBe(true);
		expect(result.current.nodeMap.get(1)?.folder.name).toBe("Docs");
		expect(mockState.listContent).not.toHaveBeenCalled();
	});

	it("keeps equivalent breadcrumb and folder content inputs stable across rerenders", async () => {
		const docs = folder(1, "Docs");
		const { result, rerender } = renderHook(
			({ breadcrumb, folderContents }) =>
				useShareFolderTree({
					breadcrumb,
					folderContents,
					token: "share-a",
				}),
			{
				initialProps: {
					breadcrumb: [{ id: null, name: "Shared Root" }],
					folderContents: contents([docs]),
				},
			},
		);
		await waitFor(() => expect(result.current.rootIds).toEqual([1]));

		rerender({
			breadcrumb: [{ id: null, name: "Shared Root" }],
			folderContents: contents([folder(1, "Docs")]),
		});

		expect(result.current.rootIds).toEqual([1]);
		expect(mockState.listContent).not.toHaveBeenCalled();
	});

	it("updates cached breadcrumb and folder content values when their data changes", async () => {
		const docs = folder(1, "Docs");
		const deep = folder(2, "Deep");
		const { result, rerender } = renderHook(
			({ breadcrumb, folderContents }) =>
				useShareFolderTree({
					breadcrumb,
					folderContents,
					token: "share-a",
				}),
			{
				initialProps: {
					breadcrumb: [{ id: null, name: "Shared Root" }],
					folderContents: contents([docs]),
				},
			},
		);
		await waitFor(() => expect(result.current.rootIds).toEqual([1]));

		rerender({
			breadcrumb: [
				{ id: null, name: "Shared Root" },
				{ id: 1, name: "Docs" },
			],
			folderContents: contents([deep]),
		});

		await waitFor(() => {
			expect(result.current.currentFolderId).toBe(1);
			expect(result.current.nodeMap.get(1)?.childIds).toEqual([2]);
		});
		expect(result.current.nodeMap.get(2)?.folder.name).toBe("Deep");
	});

	it("does not initialize or fetch a tree without a share token", async () => {
		const { result } = renderHook(() =>
			useShareFolderTree({
				breadcrumb: [],
				folderContents: null,
				token: "",
			}),
		);

		expect(result.current.rootIds).toEqual([]);
		expect(result.current.loadedKeys.size).toBe(0);
		expect(mockState.listContent).not.toHaveBeenCalled();
	});

	it("hydrates and expands every parent in a deep canonical route", async () => {
		mockState.listContent.mockResolvedValue(
			contents([folder(1, "Docs"), folder(2, "Images")]),
		);
		mockState.listSubfolderContent.mockImplementation(
			(_token: string, folderId: number) =>
				folderId === 1
					? Promise.resolve(contents([folder(3, "Deep"), folder(4, "Notes")]))
					: Promise.resolve(contents([])),
		);

		const { result } = renderHook(() =>
			useShareFolderTree({
				breadcrumb: [
					{ id: null, name: "Shared Root" },
					{ id: 1, name: "Docs" },
					{ id: 3, name: "Deep" },
				],
				folderContents: contents([folder(5, "Current child")]),
				token: "share-a",
			}),
		);

		await waitFor(() => {
			expect(result.current.rootIds).toEqual([1, 2]);
			expect(result.current.nodeMap.get(1)?.childIds).toEqual([3, 4]);
		});
		expect(result.current.nodeMap.get(3)?.childIds).toEqual([5]);
		expect(result.current.expandedKeys).toEqual(new Set(["root", "1", "3"]));
		expect(mockState.listContent).toHaveBeenCalledWith(
			"share-a",
			expect.objectContaining({ file_limit: 0, folder_limit: 1000 }),
		);
		expect(mockState.listSubfolderContent).toHaveBeenCalledWith(
			"share-a",
			1,
			expect.objectContaining({ file_limit: 0, folder_limit: 1000 }),
		);
	});

	it("tolerates sparse breadcrumb entries and reuses an existing ancestor edge", async () => {
		const docs = folder(1, "Docs");
		const deep = folder(2, "Deep");
		const { result, rerender } = renderHook(
			({ breadcrumb }) =>
				useShareFolderTree({
					breadcrumb,
					folderContents: contents([deep]),
					token: "share-a",
				}),
			{
				initialProps: {
					breadcrumb: [
						{ id: null, name: "Shared Root" },
						{ id: null, name: "Missing" },
						{ id: 1, name: "Docs" },
					] as Array<{ id: number | null; name: string }>,
				},
			},
		);
		await waitFor(() => expect(result.current.currentFolderId).toBe(1));

		rerender({
			breadcrumb: [
				{ id: null, name: "Shared Root" },
				{ id: 1, name: docs.name },
				{ id: 2, name: deep.name },
			],
		});
		await waitFor(() =>
			expect(result.current.nodeMap.get(1)?.childIds).toEqual([2]),
		);

		rerender({
			breadcrumb: [
				{ id: null, name: "Shared Root" },
				{ id: 1, name: docs.name },
				{ id: 2, name: deep.name },
				{ id: 3, name: "Leaf" },
			],
		});
		await waitFor(() => expect(result.current.currentFolderId).toBe(3));
		expect(result.current.nodeMap.get(1)?.childIds).toEqual([2]);
	});

	it("loads children for a branch that is not yet represented in the node map", async () => {
		mockState.listSubfolderContent.mockResolvedValueOnce(
			contents([folder(3, "Child")]),
		);
		const { result } = renderHook(() =>
			useShareFolderTree({
				breadcrumb: [{ id: null, name: "Shared Root" }],
				folderContents: contents([]),
				token: "share-a",
			}),
		);
		await waitFor(() =>
			expect(result.current.loadedKeys.has("root")).toBe(true),
		);

		act(() => result.current.toggle(99));

		await waitFor(() => expect(result.current.loadedKeys.has("99")).toBe(true));
		expect(result.current.nodeMap.has(99)).toBe(false);
		expect(mockState.listSubfolderContent).toHaveBeenCalledWith(
			"share-a",
			99,
			expect.objectContaining({ file_limit: 0 }),
		);
	});

	it("deduplicates lazy child loads and reuses loaded children", async () => {
		const pending = deferred<FolderContents>();
		mockState.listSubfolderContent.mockReturnValue(pending.promise);
		const { result } = renderHook(() =>
			useShareFolderTree({
				breadcrumb: [{ id: null, name: "Shared Root" }],
				folderContents: contents([folder(1, "Docs")]),
				token: "share-a",
			}),
		);
		await waitFor(() => expect(result.current.rootIds).toEqual([1]));

		act(() => {
			result.current.toggle(1);
			result.current.toggle(1);
			result.current.toggle(1);
		});
		expect(mockState.listSubfolderContent).toHaveBeenCalledTimes(1);

		await act(async () => {
			pending.resolve(contents([folder(2, "Child")]));
			await pending.promise;
		});
		await waitFor(() =>
			expect(result.current.nodeMap.get(1)?.childIds).toEqual([2]),
		);

		act(() => {
			result.current.toggle(1);
			result.current.toggle(1);
		});
		expect(mockState.listSubfolderContent).toHaveBeenCalledTimes(1);
	});

	it("discards a stale tree response after the share token changes", async () => {
		const stale = deferred<FolderContents>();
		mockState.listContent.mockImplementation((token: string) =>
			token === "share-a"
				? stale.promise
				: Promise.resolve(contents([folder(2, "Current")])),
		);
		const { result, rerender } = renderHook(
			({ token }: { token: string }) =>
				useShareFolderTree({
					breadcrumb: [{ id: null, name: "Shared Root" }],
					folderContents: null,
					token,
				}),
			{ initialProps: { token: "share-a" } },
		);

		rerender({ token: "share-b" });
		await waitFor(() => expect(result.current.rootIds).toEqual([2]));

		await act(async () => {
			stale.resolve(contents([folder(1, "Stale")]));
			await stale.promise;
		});
		expect(result.current.rootIds).toEqual([2]);
		expect(result.current.nodeMap.has(1)).toBe(false);
	});

	it("discards a stale tree failure after the share token changes", async () => {
		const stale = deferred<FolderContents>();
		mockState.listContent.mockImplementation((token: string) =>
			token === "share-a"
				? stale.promise
				: Promise.resolve(contents([folder(2, "Current")])),
		);
		const { result, rerender } = renderHook(
			({ token }: { token: string }) =>
				useShareFolderTree({
					breadcrumb: [{ id: null, name: "Shared Root" }],
					folderContents: null,
					token,
				}),
			{ initialProps: { token: "share-a" } },
		);

		rerender({ token: "share-b" });
		await waitFor(() => expect(result.current.rootIds).toEqual([2]));
		await act(async () => {
			stale.reject(new Error("stale failure"));
			await stale.promise.catch(() => undefined);
		});

		expect(result.current.failedKeys.size).toBe(0);
		expect(result.current.rootIds).toEqual([2]);
	});

	it("keeps a failed branch retryable without falling back to the root", async () => {
		mockState.listSubfolderContent
			.mockRejectedValueOnce(new Error("share scope denied"))
			.mockResolvedValueOnce(contents([folder(2, "Child")]));
		const { result } = renderHook(() =>
			useShareFolderTree({
				breadcrumb: [{ id: null, name: "Shared Root" }],
				folderContents: contents([folder(1, "Docs")]),
				token: "share-a",
			}),
		);
		await waitFor(() => expect(result.current.rootIds).toEqual([1]));

		act(() => result.current.toggle(1));
		await waitFor(() => expect(result.current.failedKeys.has("1")).toBe(true));
		act(() => {
			result.current.toggle(1);
			result.current.toggle(1);
		});
		await waitFor(() =>
			expect(result.current.nodeMap.get(1)?.childIds).toEqual([2]),
		);
		expect(mockState.listSubfolderContent).toHaveBeenCalledTimes(2);
	});

	it("clears a rejected in-flight request before retrying the same branch", async () => {
		const pending = deferred<FolderContents>();
		mockState.listSubfolderContent
			.mockReturnValueOnce(pending.promise)
			.mockResolvedValueOnce(contents([folder(2, "Child")]));
		const { result } = renderHook(() =>
			useShareFolderTree({
				breadcrumb: [{ id: null, name: "Shared Root" }],
				folderContents: contents([folder(1, "Docs")]),
				token: "share-a",
			}),
		);
		await waitFor(() => expect(result.current.rootIds).toEqual([1]));

		act(() => result.current.toggle(1));
		await act(async () => {
			pending.reject(new Error("temporary failure"));
			await pending.promise.catch(() => undefined);
		});
		await waitFor(() => expect(result.current.failedKeys.has("1")).toBe(true));

		act(() => {
			result.current.toggle(1);
			result.current.toggle(1);
		});
		await waitFor(() =>
			expect(result.current.nodeMap.get(1)?.childIds).toEqual([2]),
		);
		expect(mockState.listSubfolderContent).toHaveBeenCalledTimes(2);
	});
});
