import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	buildOrderedSelectionItems,
	findSelectionItemIndex,
	sliceSelectionRange,
} from "@/stores/fileStore/selectionRange";
import type { FileListItem, FolderListItem } from "@/types/api";

vi.mock("@/services/fileService", () => ({
	fileService: {
		listRoot: vi.fn(),
		listFolder: vi.fn(),
		getFolderAncestors: vi.fn(),
	},
}));

vi.mock("@/lib/preferenceSync", () => ({
	cancelPreferenceSync: vi.fn(),
	queuePreferenceSync: vi.fn(),
}));

vi.mock("@/services/batchService", () => ({
	batchService: {
		batchCopy: vi.fn(),
		batchDelete: vi.fn(),
		batchMove: vi.fn(),
	},
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: {
		getState: () => ({
			refreshUser: vi.fn().mockResolvedValue(undefined),
		}),
	},
}));

async function loadStore() {
	vi.resetModules();
	const { useFileStore } = await import("@/stores/fileStore");
	return useFileStore;
}

function folder(id: number) {
	return { id } as FolderListItem;
}

function file(id: number) {
	return { id } as FileListItem;
}

/** 注入 3 个文件夹（id 1-3）+ 4 个文件（id 11-14），有序序列为 [1,2,3,11,12,13,14] */
async function loadPopulatedStore() {
	const useFileStore = await loadStore();
	useFileStore.setState({
		folders: [folder(1), folder(2), folder(3)],
		files: [file(11), file(12), file(13), file(14)],
	});
	return useFileStore;
}

type Store = Awaited<ReturnType<typeof loadPopulatedStore>>;

function selectionState(s: Store) {
	const state = s.getState();
	return {
		files: [...state.selectedFileIds].sort((a, b) => a - b),
		folders: [...state.selectedFolderIds].sort((a, b) => a - b),
		anchor: state.selectionAnchor,
		focus: state.selectionFocus,
	};
}

describe("selectionRange helpers", () => {
	it("builds the display-ordered sequence (folders first, then files)", () => {
		const items = buildOrderedSelectionItems(
			[folder(1), folder(2)],
			[file(11), file(12)],
		);
		expect(items).toEqual([
			{ type: "folder", id: 1 },
			{ type: "folder", id: 2 },
			{ type: "file", id: 11 },
			{ type: "file", id: 12 },
		]);
	});

	it("finds item indexes by type and id", () => {
		const items = buildOrderedSelectionItems([folder(1)], [file(11)]);
		expect(findSelectionItemIndex(items, { type: "folder", id: 1 })).toBe(0);
		expect(findSelectionItemIndex(items, { type: "file", id: 11 })).toBe(1);
		// 同 id 不同类型不能混淆
		expect(findSelectionItemIndex(items, { type: "folder", id: 11 })).toBe(-1);
		expect(findSelectionItemIndex(items, { type: "file", id: 99 })).toBe(-1);
	});

	it("slices ranges in both directions across the folder/file boundary", () => {
		const items = buildOrderedSelectionItems(
			[folder(1), folder(2)],
			[file(11), file(12)],
		);
		expect(sliceSelectionRange(items, 0, 3)).toEqual({
			folderIds: [1, 2],
			fileIds: [11, 12],
		});
		expect(sliceSelectionRange(items, 3, 1)).toEqual({
			folderIds: [2],
			fileIds: [11, 12],
		});
	});
});

describe("selectionSlice anchor behavior", () => {
	let store: Store;

	beforeEach(async () => {
		store = await loadPopulatedStore();
	});

	it("sets anchor and focus on toggle", () => {
		store.getState().toggleFileSelection(12);
		expect(selectionState(store)).toEqual({
			files: [12],
			folders: [],
			anchor: { type: "file", id: 12 },
			focus: { type: "file", id: 12 },
		});
	});

	it("sets anchor and focus on selectOnly", () => {
		store.getState().selectOnlyFolder(2);
		expect(selectionState(store)).toEqual({
			files: [],
			folders: [2],
			anchor: { type: "folder", id: 2 },
			focus: { type: "folder", id: 2 },
		});
	});

	it("clears anchor on selectItems, selectAll and clearSelection", () => {
		store.getState().selectOnlyFile(11);
		store.getState().selectItems([11, 12], [1]);
		expect(store.getState().selectionAnchor).toBeNull();

		store.getState().selectOnlyFile(11);
		store.getState().selectAll();
		expect(store.getState().selectionAnchor).toBeNull();
		expect(store.getState().selectionFocus).toBeNull();

		store.getState().selectOnlyFile(11);
		store.getState().clearSelection();
		expect(store.getState().selectionAnchor).toBeNull();
	});
});

describe("selectRangeTo (shift+click)", () => {
	let store: Store;

	beforeEach(async () => {
		store = await loadPopulatedStore();
	});

	it("selects only the target when there is no anchor", () => {
		store.getState().selectRangeTo("file", 12);
		expect(selectionState(store)).toEqual({
			files: [12],
			folders: [],
			anchor: { type: "file", id: 12 },
			focus: { type: "file", id: 12 },
		});
	});

	it("selects the range from the nearest selected item to the target", () => {
		store.getState().selectOnlyFile(11);
		store.getState().selectRangeTo("file", 13);
		expect(selectionState(store)).toEqual({
			files: [11, 12, 13],
			folders: [],
			anchor: { type: "file", id: 11 },
			focus: { type: "file", id: 13 },
		});
	});

	it("keeps earlier cmd-clicked items when shift-clicking (union semantics)", () => {
		// Cmd 点选 folder 1 和 folder 3，再 Shift+点击 file 12：
		// 从锚点（最后 Cmd 点击的 folder 3）扩到 file 12，folder 1 保留
		store.getState().toggleFolderSelection(1);
		store.getState().toggleFolderSelection(3);
		store.getState().selectRangeTo("file", 12);
		expect(selectionState(store)).toEqual({
			files: [11, 12],
			folders: [1, 3],
			anchor: { type: "folder", id: 3 },
			focus: { type: "file", id: 12 },
		});
	});

	it("re-selects the range end from the anchor on repeated shift+clicks", () => {
		// 点击 folder 1 → Shift+file 12 → Shift+folder 2：
		// 每次都从锚点 folder 1 重新选终点，暂存的上次范围被撤出
		store.getState().selectOnlyFolder(1);
		store.getState().selectRangeTo("file", 12);
		expect(selectionState(store).files).toEqual([11, 12]);
		expect(selectionState(store).folders).toEqual([1, 2, 3]);

		store.getState().selectRangeTo("folder", 2);
		expect(selectionState(store)).toEqual({
			files: [],
			folders: [1, 2],
			anchor: { type: "folder", id: 1 },
			focus: { type: "folder", id: 2 },
		});
	});

	it("withdraws the staged shift range but keeps cmd-clicked items", () => {
		// Cmd 点选 folder 1、folder 3 → Shift+file 12 → Shift+file 11：
		// 暂存范围（folder3..file12）撤出重选为（folder3..file11），
		// file 12 移出；Cmd 选的 folder 1 不受影响
		store.getState().toggleFolderSelection(1);
		store.getState().toggleFolderSelection(3);
		store.getState().selectRangeTo("file", 12);
		expect(selectionState(store).files).toEqual([11, 12]);

		store.getState().selectRangeTo("file", 11);
		expect(selectionState(store)).toEqual({
			files: [11],
			folders: [1, 3],
			anchor: { type: "folder", id: 3 },
			focus: { type: "file", id: 11 },
		});
	});

	it("supports reverse ranges and crosses the folder/file boundary", () => {
		store.getState().selectOnlyFile(13);
		store.getState().selectRangeTo("folder", 2);
		expect(selectionState(store)).toEqual({
			files: [11, 12, 13],
			folders: [2, 3],
			anchor: { type: "file", id: 13 },
			focus: { type: "folder", id: 2 },
		});
	});

	it("treats selections whose items left the list as having no base item", () => {
		store.getState().selectOnlyFile(11);
		// 选中项 11 已不在列表中（切换目录/被删除）
		store.setState({ files: [file(13), file(14)] });
		store.getState().selectRangeTo("file", 14);
		expect(selectionState(store)).toEqual({
			files: [14],
			folders: [],
			anchor: { type: "file", id: 14 },
			focus: { type: "file", id: 14 },
		});
	});
});

describe("moveSelectionBy (arrow keys)", () => {
	let store: Store;

	beforeEach(async () => {
		store = await loadPopulatedStore();
	});

	it("selects the first item on positive delta and the last on negative when nothing is focused", () => {
		store.getState().moveSelectionBy(1);
		expect(selectionState(store).folders).toEqual([1]);

		store.getState().clearSelection();
		store.getState().moveSelectionBy(-1);
		expect(selectionState(store).files).toEqual([14]);
	});

	it("moves the single selection and follows with the anchor", () => {
		store.getState().selectOnlyFolder(2);
		store.getState().moveSelectionBy(2);
		expect(selectionState(store)).toEqual({
			files: [11],
			folders: [],
			anchor: { type: "file", id: 11 },
			focus: { type: "file", id: 11 },
		});
	});

	it("clamps at the sequence ends", () => {
		store.getState().selectOnlyFile(14);
		store.getState().moveSelectionBy(5);
		expect(selectionState(store).files).toEqual([14]);

		store.getState().moveSelectionBy(-99);
		expect(selectionState(store).folders).toEqual([1]);
	});

	it("is a no-op on an empty list", async () => {
		const empty = await loadStore();
		empty.getState().moveSelectionBy(1);
		expect(empty.getState().selectionCount()).toBe(0);
	});
});

describe("extendSelectionBy (shift+arrow keys)", () => {
	let store: Store;

	beforeEach(async () => {
		store = await loadPopulatedStore();
	});

	it("selects the first item when there is no anchor", () => {
		store.getState().extendSelectionBy(1);
		expect(selectionState(store)).toEqual({
			files: [],
			folders: [1],
			anchor: { type: "folder", id: 1 },
			focus: { type: "folder", id: 1 },
		});
	});

	it("extends the range from the anchor without moving it", () => {
		store.getState().selectOnlyFolder(2);
		store.getState().extendSelectionBy(2);
		expect(selectionState(store)).toEqual({
			files: [11],
			folders: [2, 3],
			anchor: { type: "folder", id: 2 },
			focus: { type: "file", id: 11 },
		});

		store.getState().extendSelectionBy(1);
		expect(selectionState(store)).toEqual({
			files: [11, 12],
			folders: [2, 3],
			anchor: { type: "folder", id: 2 },
			focus: { type: "file", id: 12 },
		});
	});

	it("shrinks the range when moving back towards the anchor", () => {
		// 序列 [1,2,3,11,12,13,14]，锚点 folder 2（index 1）
		store.getState().selectOnlyFolder(2);
		store.getState().extendSelectionBy(3);
		// 焦点到 file 12（index 4），范围 folder 2,3 + file 11,12
		expect(selectionState(store)).toEqual({
			files: [11, 12],
			folders: [2, 3],
			anchor: { type: "folder", id: 2 },
			focus: { type: "file", id: 12 },
		});

		// 回退 1 步：范围收缩到 folder 2,3 + file 11，锚点不动
		store.getState().extendSelectionBy(-1);
		expect(selectionState(store)).toEqual({
			files: [11],
			folders: [2, 3],
			anchor: { type: "folder", id: 2 },
			focus: { type: "file", id: 11 },
		});
	});
});
