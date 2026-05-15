import { FOLDER_LIMIT } from "@/lib/constants";
import type { FolderListItem, FolderListParams } from "@/types/api";
import type { FolderTreeNode } from "./types";

export function getFolderTreeListParams(
	sortBy: FolderListParams["sort_by"],
	sortOrder: FolderListParams["sort_order"],
): FolderListParams {
	return {
		file_limit: 0,
		folder_limit: FOLDER_LIMIT,
		sort_by: sortBy,
		sort_order: sortOrder,
	};
}

export function cloneNodeEntries(
	nodeMap: Map<number, FolderTreeNode>,
): Array<[number, FolderTreeNode]> {
	return Array.from(nodeMap.entries()).map(([id, node]) => [
		id,
		{
			folder: node.folder,
			parentId: node.parentId,
			childIds: [...node.childIds],
		},
	]);
}

export function upsertChildren(
	nodeMap: Map<number, FolderTreeNode>,
	parentId: number | null,
	folders: FolderListItem[],
	getCachedChildIds?: (id: number) => number[] | undefined,
): { nodeMap: Map<number, FolderTreeNode>; rootIds: number[] } {
	const nextNodeMap = new Map(nodeMap);
	const childIds = folders.map((folder) => folder.id);

	for (const folder of folders) {
		const existing = nextNodeMap.get(folder.id);
		nextNodeMap.set(folder.id, {
			childIds: existing?.childIds ?? getCachedChildIds?.(folder.id) ?? [],
			folder,
			parentId,
		});
	}

	if (parentId === null) {
		return { nodeMap: nextNodeMap, rootIds: childIds };
	}

	const parentNode = nextNodeMap.get(parentId);
	if (parentNode) {
		nextNodeMap.set(parentId, {
			...parentNode,
			childIds,
		});
	}

	return { nodeMap: nextNodeMap, rootIds: [] };
}
