import type { FolderListItem } from "@/types/api";
import { upsertChildren } from "./folderTreeState";
import type { FolderTreeNode, FolderTreeSnapshot } from "./types";

export interface FolderTreeViewState {
	expandedIds: Set<number>;
	loadedIds: Set<number>;
	loadingIds: Set<number>;
	nodeMap: Map<number, FolderTreeNode>;
	rootExpanded: boolean;
	rootIds: number[];
	rootLoaded: boolean;
}

export type FolderTreeAction =
	| { type: "reset" }
	| {
			type: "syncChildren";
			cachedChildIds: Map<number, number[]>;
			folders: FolderListItem[];
			parentId: number | null;
	  }
	| { type: "setFolderLoading"; folderId: number; loading: boolean }
	| { type: "setRootLoaded"; loaded: boolean }
	| { type: "markFolderUnloaded"; folderId: number }
	| { type: "expandRoot" }
	| { type: "toggleRoot" }
	| { type: "expandFolder"; folderId: number }
	| { type: "collapseFolder"; folderId: number };

export function createInitialFolderTreeState(
	snapshot: FolderTreeSnapshot | null,
): FolderTreeViewState {
	const rootIds = snapshot?.rootIds ?? [];

	return {
		expandedIds: new Set(snapshot?.expandedIds ?? []),
		loadedIds: new Set(snapshot?.loadedIds ?? []),
		loadingIds: new Set(),
		nodeMap: new Map(snapshot?.nodeEntries ?? []),
		rootExpanded: snapshot?.rootExpanded ?? true,
		rootIds,
		rootLoaded: snapshot !== null || rootIds.length > 0,
	};
}

function setFolderLoading(
	loadingIds: Set<number>,
	folderId: number,
	loading: boolean,
) {
	const next = new Set(loadingIds);
	if (loading) {
		next.add(folderId);
	} else {
		next.delete(folderId);
	}
	return next;
}

export function folderTreeReducer(
	state: FolderTreeViewState,
	action: FolderTreeAction,
): FolderTreeViewState {
	switch (action.type) {
		case "reset":
			return createInitialFolderTreeState(null);
		case "syncChildren": {
			const result = upsertChildren(
				state.nodeMap,
				action.parentId,
				action.folders,
				(id) => action.cachedChildIds.get(id),
			);

			if (action.parentId === null) {
				return {
					...state,
					nodeMap: result.nodeMap,
					rootIds: result.rootIds,
					rootLoaded: true,
				};
			}

			return {
				...state,
				loadedIds: new Set(state.loadedIds).add(action.parentId),
				nodeMap: result.nodeMap,
			};
		}
		case "setFolderLoading":
			return {
				...state,
				loadingIds: setFolderLoading(
					state.loadingIds,
					action.folderId,
					action.loading,
				),
			};
		case "setRootLoaded":
			return {
				...state,
				rootLoaded: action.loaded,
			};
		case "markFolderUnloaded": {
			const loadedIds = new Set(state.loadedIds);
			loadedIds.delete(action.folderId);
			return {
				...state,
				loadedIds,
			};
		}
		case "expandRoot":
			return {
				...state,
				rootExpanded: true,
			};
		case "toggleRoot":
			return {
				...state,
				rootExpanded: !state.rootExpanded,
			};
		case "expandFolder":
			return {
				...state,
				expandedIds: new Set(state.expandedIds).add(action.folderId),
			};
		case "collapseFolder": {
			const expandedIds = new Set(state.expandedIds);
			expandedIds.delete(action.folderId);
			return {
				...state,
				expandedIds,
			};
		}
	}
}
