import { waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { STORAGE_KEYS } from "@/config/app";
import { FILE_PAGE_SIZE, FOLDER_LIMIT } from "@/lib/constants";
import { createFolderContents } from "@/test/fixtures";
import type { FolderContents } from "@/types/api";

const mockState = vi.hoisted(() => ({
	batchCopy: vi.fn(),
	batchDelete: vi.fn(),
	batchMove: vi.fn(),
	createEmptyFile: vi.fn(),
	createFolder: vi.fn(),
	deleteFile: vi.fn(),
	deleteFolder: vi.fn(),
	getFolderAncestors: vi.fn(),
	listFolder: vi.fn(),
	listRoot: vi.fn(),
	queuePreferenceSync: vi.fn(),
	refreshUser: vi.fn(async () => undefined),
	warn: vi.fn(),
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		createEmptyFile: mockState.createEmptyFile,
		createFolder: mockState.createFolder,
		deleteFile: mockState.deleteFile,
		deleteFolder: mockState.deleteFolder,
		getFolderAncestors: mockState.getFolderAncestors,
		listFolder: mockState.listFolder,
		listRoot: mockState.listRoot,
		moveFile: vi.fn(),
		moveFolder: vi.fn(),
	},
}));

vi.mock("@/lib/preferenceSync", () => ({
	cancelPreferenceSync: vi.fn(),
	queuePreferenceSync: mockState.queuePreferenceSync,
}));

vi.mock("@/services/batchService", () => ({
	batchService: {
		batchCopy: mockState.batchCopy,
		batchDelete: mockState.batchDelete,
		batchMove: mockState.batchMove,
	},
	singleOperationResult: (promise: Promise<unknown>) =>
		promise.then(() => ({ succeeded: 1, failed: 0, errors: [] })),
	resolveMoveDispatch: ({
		fileIds,
		folderIds,
		targetFolderId,
		dispatcher,
	}: {
		fileIds: number[];
		folderIds: number[];
		targetFolderId: number | null;
		dispatcher: {
			batchMove: (...args: unknown[]) => unknown;
		};
	}) => dispatcher.batchMove(fileIds, folderIds, targetFolderId),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: {
		getState: () => ({
			refreshUser: mockState.refreshUser,
		}),
	},
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: mockState.warn,
		error: vi.fn(),
		debug: vi.fn(),
	},
}));

async function loadStore() {
	vi.resetModules();
	return await import("@/stores/fileStore");
}

function expectAbortOptions(options: unknown) {
	expect(options).toEqual(
		expect.objectContaining({
			signal: expect.any(AbortSignal),
		}),
	);
}

function expectFolderRefresh(
	callIndex: number,
	folderId: number | null,
	sortBy = "name",
	sortOrder = "asc",
) {
	const expectedParams = {
		folder_limit: FOLDER_LIMIT,
		file_limit: FILE_PAGE_SIZE,
		sort_by: sortBy,
		sort_order: sortOrder,
	};

	if (folderId === null) {
		expect(mockState.listRoot).toHaveBeenNthCalledWith(
			callIndex,
			expectedParams,
			expect.any(Object),
		);
		expectAbortOptions(mockState.listRoot.mock.calls[callIndex - 1]?.[1]);
		return;
	}

	expect(mockState.listFolder).toHaveBeenNthCalledWith(
		callIndex,
		folderId,
		expectedParams,
		expect.any(Object),
	);
	expectAbortOptions(mockState.listFolder.mock.calls[callIndex - 1]?.[2]);
}

describe("useFileStore actions", () => {
	beforeEach(() => {
		localStorage.clear();
		mockState.batchCopy.mockReset();
		mockState.batchDelete.mockReset();
		mockState.batchMove.mockReset();
		mockState.createEmptyFile.mockReset();
		mockState.createFolder.mockReset();
		mockState.deleteFile.mockReset();
		mockState.deleteFolder.mockReset();
		mockState.getFolderAncestors.mockReset();
		mockState.listFolder.mockReset();
		mockState.listRoot.mockReset();
		mockState.queuePreferenceSync.mockReset();
		mockState.refreshUser.mockReset();
		mockState.refreshUser.mockResolvedValue(undefined);
		mockState.warn.mockReset();
	});

	it("aborts active requests and resets workspace-scoped state", async () => {
		const { useFileStore } = await loadStore();
		const controller = new AbortController();
		const seeded = createFolderContents({
			files: [{ id: 20, name: "draft.txt" }],
			folders: [{ id: 10, name: "Docs" }],
			files_total: 1,
			folders_total: 1,
			next_file_cursor: {
				id: 20,
				value: "draft.txt",
			} as FolderContents["next_file_cursor"],
		});

		useFileStore.setState({
			currentFolderId: 5,
			breadcrumb: [
				{ id: null, name: "Root" },
				{ id: 5, name: "Docs" },
			],
			files: seeded.files,
			folders: seeded.folders,
			loading: true,
			error: "boom",
			filesTotalCount: 1,
			foldersTotalCount: 1,
			loadingMore: true,
			nextFileCursor: seeded.next_file_cursor,
			selectedFileIds: new Set([20]),
			selectedFolderIds: new Set([10]),
			clipboard: {
				fileIds: [20],
				folderIds: [10],
				mode: "copy",
			},
			workspaceRequestRevision: 4,
			_workspaceRequestId: 9,
			_workspaceRequestController: controller,
		});

		useFileStore.getState().resetWorkspaceState();

		const state = useFileStore.getState();
		expect(controller.signal.aborted).toBe(true);
		expect(state.workspaceRequestRevision).toBe(5);
		expect(state._workspaceRequestId).toBe(0);
		expect(state._workspaceRequestController).toBeNull();
		expect(state.currentFolderId).toBeNull();
		expect(state.breadcrumb).toEqual([{ id: null, name: "Root" }]);
		expect(state.loading).toBe(false);
		expect(state.error).toBeNull();
		expect(state.folders).toEqual([]);
		expect(state.files).toEqual([]);
		expect(state.foldersTotalCount).toBe(0);
		expect(state.filesTotalCount).toBe(0);
		expect(state.loadingMore).toBe(false);
		expect(state.nextFileCursor).toBeNull();
		expect(state.selectedFileIds.size).toBe(0);
		expect(state.selectedFolderIds.size).toBe(0);
		expect(state.clipboard).toBeNull();
	});

	it("manages selection helpers across files and folders", async () => {
		const { useFileStore } = await loadStore();
		const contents = createFolderContents({
			files: [
				{ id: 1, name: "alpha.txt" },
				{ id: 2, name: "beta.txt" },
			],
			folders: [
				{ id: 10, name: "Docs" },
				{ id: 11, name: "Media" },
			],
		});

		useFileStore.setState({
			files: contents.files,
			folders: contents.folders,
		});

		useFileStore.getState().toggleFileSelection(1);
		useFileStore.getState().toggleFolderSelection(10);

		expect(useFileStore.getState().selectionCount()).toBe(2);

		useFileStore.getState().toggleFileSelection(1);
		expect(useFileStore.getState().selectedFileIds).toEqual(new Set());

		useFileStore.getState().selectOnlyFile(2);
		expect(useFileStore.getState().selectedFileIds).toEqual(new Set([2]));
		expect(useFileStore.getState().selectedFolderIds).toEqual(new Set());

		useFileStore.getState().selectOnlyFolder(11);
		expect(useFileStore.getState().selectedFileIds).toEqual(new Set());
		expect(useFileStore.getState().selectedFolderIds).toEqual(new Set([11]));

		useFileStore.getState().selectAll();
		expect(useFileStore.getState().selectedFileIds).toEqual(new Set([1, 2]));
		expect(useFileStore.getState().selectedFolderIds).toEqual(
			new Set([10, 11]),
		);

		useFileStore.getState().selectItems([2], [11]);
		expect(useFileStore.getState().selectedFileIds).toEqual(new Set([2]));
		expect(useFileStore.getState().selectedFolderIds).toEqual(new Set([11]));

		useFileStore.getState().clearSelection();
		expect(useFileStore.getState().selectionCount()).toBe(0);
		expect(useFileStore.getState().selectedFileIds).toEqual(new Set());
		expect(useFileStore.getState().selectedFolderIds).toEqual(new Set());
	});

	it("persists sort order changes and reloads the current folder", async () => {
		mockState.listRoot.mockResolvedValue(
			createFolderContents({
				files: [{ id: 8, name: "zeta.txt" }],
				files_total: 1,
			}),
		);

		const { useFileStore } = await loadStore();

		useFileStore.getState().setSortOrder("desc");

		await waitFor(() => {
			expect(mockState.listRoot).toHaveBeenCalledTimes(1);
		});

		expectFolderRefresh(1, null, "name", "desc");
		expect(mockState.queuePreferenceSync).toHaveBeenCalledWith({
			sort_order: "desc",
		});
		expect(localStorage.getItem(STORAGE_KEYS.sortOrder)).toBe("desc");
		expect(useFileStore.getState().sortOrder).toBe("desc");
		expect(useFileStore.getState().files).toEqual([
			expect.objectContaining({ id: 8, name: "zeta.txt" }),
		]);
	});

	it("applies server preferences into state and local storage", async () => {
		const { useFileStore } = await loadStore();

		useFileStore.getState()._applyFromServer({
			viewMode: "grid",
			browserOpenMode: "double_click",
			sortBy: "updated_at",
			sortOrder: "desc",
		});

		expect(useFileStore.getState()).toMatchObject({
			viewMode: "grid",
			browserOpenMode: "double_click",
			sortBy: "updated_at",
			sortOrder: "desc",
		});
		expect(localStorage.getItem(STORAGE_KEYS.viewMode)).toBe("grid");
		expect(localStorage.getItem(STORAGE_KEYS.browserOpenMode)).toBe(
			"double_click",
		);
		expect(localStorage.getItem(STORAGE_KEYS.sortBy)).toBe("updated_at");
		expect(localStorage.getItem(STORAGE_KEYS.sortOrder)).toBe("desc");
		expect(mockState.queuePreferenceSync).not.toHaveBeenCalled();
	});

	it("normalizes invalid server preferences before updating state", async () => {
		const { useFileStore } = await loadStore();

		useFileStore.getState()._applyFromServer({
			viewMode: "tiles",
			browserOpenMode: "hover",
			sortBy: "owner",
			sortOrder: "random",
		});

		expect(useFileStore.getState()).toMatchObject({
			viewMode: "list",
			browserOpenMode: "single_click",
			sortBy: "name",
			sortOrder: "asc",
		});
		expect(localStorage.getItem(STORAGE_KEYS.viewMode)).toBe("list");
		expect(localStorage.getItem(STORAGE_KEYS.browserOpenMode)).toBe(
			"single_click",
		);
		expect(localStorage.getItem(STORAGE_KEYS.sortBy)).toBe("name");
		expect(localStorage.getItem(STORAGE_KEYS.sortOrder)).toBe("asc");
	});

	it("creates files in the current folder and refreshes auth state", async () => {
		mockState.createEmptyFile.mockResolvedValue(undefined);
		mockState.listFolder.mockResolvedValue(
			createFolderContents({
				files: [{ id: 12, name: "notes.txt" }],
				files_total: 1,
			}),
		);

		const { useFileStore } = await loadStore();
		useFileStore.setState({ currentFolderId: 9 });

		await useFileStore.getState().createFile("notes.txt");

		expect(mockState.createEmptyFile).toHaveBeenCalledWith("notes.txt", 9);
		expectFolderRefresh(1, 9);
		expect(mockState.refreshUser).not.toHaveBeenCalled();
		expect(useFileStore.getState().files).toEqual([
			expect.objectContaining({ id: 12, name: "notes.txt" }),
		]);
	});

	it("creates folders in the current folder without refreshing auth state", async () => {
		mockState.createFolder.mockResolvedValue(undefined);
		mockState.listFolder.mockResolvedValue(
			createFolderContents({
				folders: [{ id: 33, name: "Projects" }],
				folders_total: 1,
			}),
		);

		const { useFileStore } = await loadStore();
		useFileStore.setState({ currentFolderId: 9 });

		await useFileStore.getState().createFolder("Projects");

		expect(mockState.createFolder).toHaveBeenCalledWith("Projects", 9);
		expectFolderRefresh(1, 9);
		expect(mockState.refreshUser).not.toHaveBeenCalled();
		expect(useFileStore.getState().folders).toEqual([
			expect.objectContaining({ id: 33, name: "Projects" }),
		]);
	});

	it("deletes files and folders, clears their selection, and refreshes contents without auth", async () => {
		mockState.deleteFile.mockResolvedValue(undefined);
		mockState.deleteFolder.mockResolvedValue(undefined);
		mockState.listFolder
			.mockResolvedValueOnce(
				createFolderContents({
					folders: [{ id: 44, name: "Media" }],
					files: [{ id: 2, name: "keep.txt" }],
					files_total: 1,
					folders_total: 1,
				}),
			)
			.mockResolvedValueOnce(createFolderContents());

		const { useFileStore } = await loadStore();
		useFileStore.setState({
			currentFolderId: 9,
			selectedFileIds: new Set([1, 2]),
			selectedFolderIds: new Set([44, 55]),
		});

		await useFileStore.getState().deleteFile(1);

		expect(mockState.deleteFile).toHaveBeenCalledWith(1);
		expect(useFileStore.getState().selectedFileIds).toEqual(new Set([2]));
		expectFolderRefresh(1, 9);
		expect(mockState.refreshUser).not.toHaveBeenCalled();

		await useFileStore.getState().deleteFolder(44);

		expect(mockState.deleteFolder).toHaveBeenCalledWith(44);
		expect(useFileStore.getState().selectedFolderIds).toEqual(new Set([55]));
		expectFolderRefresh(2, 9);
		expect(mockState.refreshUser).not.toHaveBeenCalled();
	});

	it("moves selections, clears them, and refreshes workspace contents", async () => {
		const result = {
			succeeded: 2,
			failed: 0,
			errors: [],
		};
		mockState.batchMove.mockResolvedValue(result);
		mockState.listFolder.mockResolvedValue(
			createFolderContents({
				files: [{ id: 77, name: "moved.txt" }],
				folders: [{ id: 88, name: "Archived" }],
				files_total: 1,
				folders_total: 1,
			}),
		);
		mockState.getFolderAncestors.mockResolvedValue([{ id: 5, name: "Docs" }]);

		const { useFileStore } = await loadStore();
		useFileStore.setState({
			currentFolderId: 5,
			loading: true,
			loadingMore: true,
			error: "stale error",
			selectedFileIds: new Set([1]),
			selectedFolderIds: new Set([2]),
		});

		await expect(
			useFileStore.getState().moveToFolder([1], [2], 9),
		).resolves.toEqual(result);

		expect(mockState.batchMove).toHaveBeenCalledWith([1], [2], 9);
		expectFolderRefresh(1, 5);
		expect(mockState.getFolderAncestors).toHaveBeenCalledWith(
			5,
			expect.any(Object),
		);
		expectAbortOptions(mockState.getFolderAncestors.mock.calls[0]?.[1]);
		expect(useFileStore.getState().selectedFileIds.size).toBe(0);
		expect(useFileStore.getState().selectedFolderIds.size).toBe(0);
		expect(useFileStore.getState().breadcrumb).toEqual([
			{ id: null, name: "Root" },
			{ id: 5, name: "Docs" },
		]);
		expect(useFileStore.getState().files).toEqual([
			expect.objectContaining({ id: 77, name: "moved.txt" }),
		]);
		expect(useFileStore.getState().folders).toEqual([
			expect.objectContaining({ id: 88, name: "Archived" }),
		]);
		expect(useFileStore.getState().loading).toBe(false);
		expect(useFileStore.getState().loadingMore).toBe(false);
		expect(useFileStore.getState().error).toBeNull();
	});
});
