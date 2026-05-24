import type { ReactNode } from "react";
import type { SortBy, SortOrder } from "@/stores/fileStore";
import type { FolderListItem } from "@/types/api";

export interface FolderTreeNode {
	folder: FolderListItem;
	parentId: number | null;
	childIds: number[];
}

export interface FolderTreeSnapshot {
	expandedIds: number[];
	loadedIds: number[];
	nodeEntries: Array<[number, FolderTreeNode]>;
	rootExpanded: boolean;
	rootIds: number[];
	sortBy: SortBy;
	sortOrder: SortOrder;
	userId: number | null;
	workspaceKey: string;
}

export interface TreeNodeProps {
	currentFolderId: number | null;
	depth: number;
	expandedIds: Set<number>;
	loadedIds: Set<number>;
	loadingIds: Set<number>;
	nodeId: number;
	nodeMap: Map<number, FolderTreeNode>;
	onDragHoverEnd: (id: number) => void;
	onDragHoverStart: (id: number) => void;
	onDrop: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number,
		targetPathIds: number[],
	) => void;
	onNavigate: (id: number, name: string) => void;
	onToggle: (id: number) => void;
	renderChildren: (ids: number[], depth: number) => ReactNode;
}

export interface FolderTreeProps {
	onMoveToFolder?: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<void> | void;
}
