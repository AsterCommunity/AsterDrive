import {
	buildOrderedSelectionItems,
	findSelectionItemIndex,
	type SelectionItemKey,
	sliceSelectionRange,
} from "./selectionRange";
import type { FileStoreSlice, SelectionSlice } from "./types";

function onlySelection(key: SelectionItemKey) {
	return {
		selectedFileIds: new Set(key.type === "file" ? [key.id] : []),
		selectedFolderIds: new Set(key.type === "folder" ? [key.id] : []),
	};
}

const EMPTY_SHIFT_RANGE = {
	shiftRangeFileIds: new Set<number>(),
	shiftRangeFolderIds: new Set<number>(),
};

export const createSelectionSlice: FileStoreSlice<SelectionSlice> = (
	set,
	get,
) => ({
	selectedFileIds: new Set(),
	selectedFolderIds: new Set(),
	// 锚点/焦点是范围选择的基准：锚点由普通点击或 Cmd+点击设定，
	// Shift+点击/Shift+方向键都从锚点出发。条目在列表中消失
	// （切换目录、删除）时不主动清理，各操作在计算时惰性校验。
	selectionAnchor: null,
	selectionFocus: null,
	// 上一次 Shift+点击并入选择的项（Finder 的"暂时加入"部分）。
	// 连续的 Shift+点击会先把这部分撤出、再从锚点并入新范围——
	// 反向点回去时暂时加入的项会被移出选择。任何非 Shift 的选择
	// 操作（普通点击、Cmd、全选、方向键移动）都会将其清空固化。
	shiftRangeFileIds: new Set(),
	shiftRangeFolderIds: new Set(),

	toggleFileSelection: (id) => {
		const next = new Set(get().selectedFileIds);
		if (next.has(id)) {
			next.delete(id);
		} else {
			next.add(id);
		}
		const key: SelectionItemKey = { type: "file", id };
		set({
			selectedFileIds: next,
			selectionAnchor: key,
			selectionFocus: key,
			...EMPTY_SHIFT_RANGE,
		});
	},

	toggleFolderSelection: (id) => {
		const next = new Set(get().selectedFolderIds);
		if (next.has(id)) {
			next.delete(id);
		} else {
			next.add(id);
		}
		const key: SelectionItemKey = { type: "folder", id };
		set({
			selectedFolderIds: next,
			selectionAnchor: key,
			selectionFocus: key,
			...EMPTY_SHIFT_RANGE,
		});
	},

	selectOnlyFile: (id) => {
		const key: SelectionItemKey = { type: "file", id };
		set({
			selectedFileIds: new Set([id]),
			selectedFolderIds: new Set(),
			selectionAnchor: key,
			selectionFocus: key,
			...EMPTY_SHIFT_RANGE,
		});
	},

	selectOnlyFolder: (id) => {
		const key: SelectionItemKey = { type: "folder", id };
		set({
			selectedFileIds: new Set(),
			selectedFolderIds: new Set([id]),
			selectionAnchor: key,
			selectionFocus: key,
			...EMPTY_SHIFT_RANGE,
		});
	},

	selectItems: (fileIds, folderIds) => {
		set({
			selectedFileIds: new Set(fileIds),
			selectedFolderIds: new Set(folderIds),
			selectionAnchor: null,
			selectionFocus: null,
			...EMPTY_SHIFT_RANGE,
		});
	},

	// Shift+点击（Finder 连续选择语义）：从锚点出发把到目标的范围并入
	// 选择；先撤出上一次 Shift+点击并入的部分，所以连续 Shift+点击
	// 等于"从锚点重新选终点"，反向点回时暂选区会收缩。Cmd 点选的项
	// 不在暂存范围内，始终保留。无有效锚点时等同普通点击。
	selectRangeTo: (type, id) => {
		const {
			files,
			folders,
			selectedFileIds,
			selectedFolderIds,
			selectionAnchor,
			shiftRangeFileIds,
			shiftRangeFolderIds,
		} = get();
		const items = buildOrderedSelectionItems(folders, files);
		const targetIndex = findSelectionItemIndex(items, { type, id });
		if (targetIndex < 0) return;
		const target = items[targetIndex];

		const anchorIndex = selectionAnchor
			? findSelectionItemIndex(items, selectionAnchor)
			: -1;
		if (anchorIndex < 0) {
			set({
				...onlySelection(target),
				selectionAnchor: target,
				selectionFocus: target,
				shiftRangeFileIds: new Set(target.type === "file" ? [target.id] : []),
				shiftRangeFolderIds: new Set(
					target.type === "folder" ? [target.id] : [],
				),
			});
			return;
		}

		const range = sliceSelectionRange(items, anchorIndex, targetIndex);
		// 先撤出上次 Shift 暂存，再并入新范围
		const nextFileIds = new Set(selectedFileIds);
		const nextFolderIds = new Set(selectedFolderIds);
		for (const fileId of shiftRangeFileIds) nextFileIds.delete(fileId);
		for (const folderId of shiftRangeFolderIds) nextFolderIds.delete(folderId);
		for (const fileId of range.fileIds) nextFileIds.add(fileId);
		for (const folderId of range.folderIds) nextFolderIds.add(folderId);
		set({
			selectedFileIds: nextFileIds,
			selectedFolderIds: nextFolderIds,
			selectionFocus: target,
			shiftRangeFileIds: new Set(range.fileIds),
			shiftRangeFolderIds: new Set(range.folderIds),
		});
	},

	// 方向键：移动焦点并单选目标项，锚点跟随。无焦点时从首/末项开始。
	moveSelectionBy: (delta) => {
		const { files, folders, selectionFocus } = get();
		const items = buildOrderedSelectionItems(folders, files);
		if (items.length === 0) return;

		const focusIndex = selectionFocus
			? findSelectionItemIndex(items, selectionFocus)
			: -1;
		let nextIndex: number;
		if (focusIndex < 0) {
			nextIndex = delta >= 0 ? 0 : items.length - 1;
		} else {
			nextIndex = Math.min(items.length - 1, Math.max(0, focusIndex + delta));
		}

		const target = items[nextIndex];
		set({
			...onlySelection(target),
			selectionAnchor: target,
			selectionFocus: target,
			...EMPTY_SHIFT_RANGE,
		});
	},

	// Shift+方向键：锚点固定，焦点移动，选择为锚点到焦点的范围。
	extendSelectionBy: (delta) => {
		const { files, folders, selectionAnchor, selectionFocus } = get();
		const items = buildOrderedSelectionItems(folders, files);
		if (items.length === 0) return;

		const anchorIndex = selectionAnchor
			? findSelectionItemIndex(items, selectionAnchor)
			: -1;
		if (anchorIndex < 0) {
			const target = delta >= 0 ? items[0] : items[items.length - 1];
			set({
				...onlySelection(target),
				selectionAnchor: target,
				selectionFocus: target,
				...EMPTY_SHIFT_RANGE,
			});
			return;
		}

		const focusIndex = selectionFocus
			? findSelectionItemIndex(items, selectionFocus)
			: anchorIndex;
		const nextFocusIndex = Math.min(
			items.length - 1,
			Math.max(0, (focusIndex < 0 ? anchorIndex : focusIndex) + delta),
		);
		const range = sliceSelectionRange(items, anchorIndex, nextFocusIndex);
		set({
			selectedFileIds: new Set(range.fileIds),
			selectedFolderIds: new Set(range.folderIds),
			selectionFocus: items[nextFocusIndex],
			...EMPTY_SHIFT_RANGE,
		});
	},

	selectAll: () => {
		const { files, folders } = get();
		set({
			selectedFileIds: new Set(files.map((file) => file.id)),
			selectedFolderIds: new Set(folders.map((folder) => folder.id)),
			selectionAnchor: null,
			selectionFocus: null,
			...EMPTY_SHIFT_RANGE,
		});
	},

	clearSelection: () => {
		set({
			selectedFileIds: new Set(),
			selectedFolderIds: new Set(),
			selectionAnchor: null,
			selectionFocus: null,
			...EMPTY_SHIFT_RANGE,
		});
	},

	selectionCount: () => {
		const { selectedFileIds, selectedFolderIds } = get();
		return selectedFileIds.size + selectedFolderIds.size;
	},
});
