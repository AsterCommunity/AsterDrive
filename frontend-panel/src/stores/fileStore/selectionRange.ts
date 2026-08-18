import type { FileListItem, FolderListItem } from "@/types/api";

/**
 * 选择操作的条目键与有序序列。
 *
 * 序列顺序与网格/表格的展示顺序一致（文件夹在前、文件在后），
 * 范围选择（Shift+点击）与键盘导航（方向键 / Shift+方向键）都基于它计算。
 */
export interface SelectionItemKey {
	type: "file" | "folder";
	id: number;
}

export function buildOrderedSelectionItems(
	folders: FolderListItem[],
	files: FileListItem[],
): SelectionItemKey[] {
	return [
		...folders.map((folder) => ({
			type: "folder" as const,
			id: folder.id,
		})),
		...files.map((file) => ({ type: "file" as const, id: file.id })),
	];
}

/** 在有序序列中查找条目下标，未找到返回 -1。 */
export function findSelectionItemIndex(
	items: SelectionItemKey[],
	key: SelectionItemKey,
): number {
	return items.findIndex(
		(item) => item.type === key.type && item.id === key.id,
	);
}

/** 取出序列中 [a, b] 闭区间（自动处理反向）内的所有条目 id。 */
export function sliceSelectionRange(
	items: SelectionItemKey[],
	fromIndex: number,
	toIndex: number,
): { fileIds: number[]; folderIds: number[] } {
	const start = Math.min(fromIndex, toIndex);
	const end = Math.max(fromIndex, toIndex);
	const fileIds: number[] = [];
	const folderIds: number[] = [];
	for (let index = start; index <= end; index++) {
		const item = items[index];
		if (!item) continue;
		if (item.type === "folder") folderIds.push(item.id);
		else fileIds.push(item.id);
	}
	return { fileIds, folderIds };
}
