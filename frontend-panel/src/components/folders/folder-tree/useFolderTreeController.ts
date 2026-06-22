import type { DragEvent } from "react";
import {
	useCallback,
	useEffect,
	useMemo,
	useReducer,
	useRef,
	useState,
} from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { handleApiError } from "@/hooks/useApiError";
import { FOLDER_TREE_DRAG_EXPAND_DELAY_MS } from "@/lib/constants";
import {
	getInvalidInternalDropReason,
	hasInternalDragData,
	readInternalDragData,
} from "@/lib/dragDrop";
import { subscribeStorageChange } from "@/lib/storageChangeBus";
import { decideFolderTreeStorageRefresh } from "@/lib/storageMutationCoordinator";
import {
	workspaceFolderPath,
	workspaceKey,
	workspaceRootPath,
} from "@/lib/workspace";
import { fileService } from "@/services/fileService";
import { useAuthStore } from "@/stores/authStore";
import { useFileStore } from "@/stores/fileStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { FolderListItem } from "@/types/api";
import {
	createInitialFolderTreeState,
	folderTreeReducer,
} from "./folderTreeReducer";
import { cloneNodeEntries, getFolderTreeListParams } from "./folderTreeState";
import type { FolderTreeProps, FolderTreeSnapshot } from "./types";

let folderTreeSnapshot: FolderTreeSnapshot | null = null;

export function useFolderTreeController({
	onMoveToFolder,
}: FolderTreeProps = {}) {
	const userId = useAuthStore((s) => s.user?.id ?? null);
	const workspace = useWorkspaceStore((s) => s.workspace);
	const currentWorkspaceKey = workspaceKey(workspace);
	const location = useLocation();
	const navigate = useNavigate();
	const breadcrumb = useFileStore((s) => s.breadcrumb);
	const currentFolderId = useFileStore((s) => s.currentFolderId);
	const moveToFolder = useFileStore((s) => s.moveToFolder);
	const storeFolders = useFileStore((s) => s.folders);
	const storeCurrentFolderId = useFileStore((s) => s.currentFolderId);
	const storeLastFolderContents = useFileStore((s) => s.lastFolderContents);
	const storeLoading = useFileStore((s) => s.loading);
	const sortBy = useFileStore((s) => s.sortBy);
	const sortOrder = useFileStore((s) => s.sortOrder);
	const isRootRoute = location.pathname === workspaceRootPath(workspace);
	const cachedSnapshot =
		folderTreeSnapshot?.userId === userId &&
		folderTreeSnapshot.workspaceKey === currentWorkspaceKey &&
		folderTreeSnapshot.sortBy === sortBy &&
		folderTreeSnapshot.sortOrder === sortOrder
			? folderTreeSnapshot
			: null;
	const [treeState, dispatchTree] = useReducer(
		folderTreeReducer,
		cachedSnapshot,
		createInitialFolderTreeState,
	);
	const [rootDragOver, setRootDragOver] = useState(false);
	const {
		expandedIds,
		loadedIds,
		loadingIds,
		nodeMap,
		rootExpanded,
		rootIds,
		rootLoaded,
	} = treeState;

	const childrenCacheRef = useRef<Map<number | null, FolderListItem[]>>(
		new Map(),
	);
	const inflightLoadsRef = useRef<Map<number | null, Promise<void>>>(new Map());
	const expandingPathRef = useRef<string>("");
	const hoverExpandTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	const hoverExpandTargetIdRef = useRef<number | null>(null);

	const clearHoverExpandTimer = useCallback(() => {
		if (hoverExpandTimerRef.current) {
			clearTimeout(hoverExpandTimerRef.current);
			hoverExpandTimerRef.current = null;
		}
		hoverExpandTargetIdRef.current = null;
	}, []);

	useEffect(() => {
		if (
			folderTreeSnapshot?.userId === userId &&
			folderTreeSnapshot.workspaceKey === currentWorkspaceKey &&
			folderTreeSnapshot.sortBy === sortBy &&
			folderTreeSnapshot.sortOrder === sortOrder
		)
			return;
		clearHoverExpandTimer();
		folderTreeSnapshot = null;
		childrenCacheRef.current = new Map();
		inflightLoadsRef.current = new Map();
		expandingPathRef.current = "";
		dispatchTree({ type: "reset" });
	}, [clearHoverExpandTimer, currentWorkspaceKey, sortBy, sortOrder, userId]);

	useEffect(() => {
		folderTreeSnapshot = {
			expandedIds: Array.from(expandedIds),
			loadedIds: Array.from(loadedIds),
			nodeEntries: cloneNodeEntries(nodeMap),
			rootExpanded,
			rootIds,
			sortBy,
			sortOrder,
			userId,
			workspaceKey: currentWorkspaceKey,
		};
	}, [
		currentWorkspaceKey,
		expandedIds,
		loadedIds,
		nodeMap,
		rootExpanded,
		rootIds,
		sortBy,
		sortOrder,
		userId,
	]);

	const syncFolderChildren = useCallback(
		(parentId: number | null, folders: FolderListItem[]) => {
			childrenCacheRef.current.set(parentId, folders);
			const cachedChildIds = new Map<number, number[]>();
			for (const [id, cachedChildren] of childrenCacheRef.current) {
				if (id !== null) {
					cachedChildIds.set(
						id,
						cachedChildren.map((folder) => folder.id),
					);
				}
			}
			dispatchTree({
				type: "syncChildren",
				cachedChildIds,
				folders,
				parentId,
			});
		},
		[],
	);

	const hasFreshStoreFolderContents = useCallback(
		(parentId: number | null) =>
			storeLastFolderContents?.folderId === parentId &&
			storeLastFolderContents.sortBy === sortBy &&
			storeLastFolderContents.sortOrder === sortOrder &&
			storeLastFolderContents.workspaceRevision ===
				useFileStore.getState().workspaceRequestRevision,
		[sortBy, sortOrder, storeLastFolderContents],
	);

	const ensureChildrenLoaded = useCallback(
		async (parentId: number | null) => {
			if (parentId === null) {
				if (rootLoaded) return;
			} else if (loadedIds.has(parentId)) {
				return;
			}

			const lastFolderContents = useFileStore.getState().lastFolderContents;
			if (
				lastFolderContents?.folderId === parentId &&
				lastFolderContents.sortBy === sortBy &&
				lastFolderContents.sortOrder === sortOrder &&
				lastFolderContents.workspaceRevision ===
					useFileStore.getState().workspaceRequestRevision
			) {
				syncFolderChildren(parentId, lastFolderContents.folders);
				return;
			}

			const inflight = inflightLoadsRef.current.get(parentId);
			if (inflight) {
				await inflight;
				return;
			}

			const cached = childrenCacheRef.current.get(parentId);
			if (cached) {
				syncFolderChildren(parentId, cached);
				return;
			}

			const loadPromise = (async () => {
				if (parentId !== null) {
					dispatchTree({
						type: "setFolderLoading",
						folderId: parentId,
						loading: true,
					});
				}
				try {
					const contents =
						parentId === null
							? await fileService.listRoot(
									getFolderTreeListParams(sortBy, sortOrder),
								)
							: await fileService.listFolder(
									parentId,
									getFolderTreeListParams(sortBy, sortOrder),
								);
					syncFolderChildren(parentId, contents.folders);
				} finally {
					if (parentId !== null) {
						dispatchTree({
							type: "setFolderLoading",
							folderId: parentId,
							loading: false,
						});
					}
					inflightLoadsRef.current.delete(parentId);
				}
			})();

			inflightLoadsRef.current.set(parentId, loadPromise);
			await loadPromise;
		},
		[loadedIds, rootLoaded, sortBy, sortOrder, syncFolderChildren],
	);

	const refreshFolderChildren = useCallback(
		async (parentId: number | null) => {
			childrenCacheRef.current.delete(parentId);
			inflightLoadsRef.current.delete(parentId);
			if (parentId === null) {
				dispatchTree({ type: "setRootLoaded", loaded: false });
				const contents = await fileService.listRoot(
					getFolderTreeListParams(sortBy, sortOrder),
				);
				syncFolderChildren(null, contents.folders);
				return;
			}
			dispatchTree({ type: "markFolderUnloaded", folderId: parentId });
			const contents = await fileService.listFolder(
				parentId,
				getFolderTreeListParams(sortBy, sortOrder),
			);
			syncFolderChildren(parentId, contents.folders);
		},
		[sortBy, sortOrder, syncFolderChildren],
	);

	useEffect(() => {
		if (rootLoaded) return;

		if (isRootRoute && storeCurrentFolderId === null) {
			if (storeLoading) return;
			if (hasFreshStoreFolderContents(null)) {
				syncFolderChildren(null, storeFolders);
				return;
			}
		}

		let cancelled = false;
		void ensureChildrenLoaded(null).catch(() => {
			if (!cancelled) {
				dispatchTree({ type: "setRootLoaded", loaded: false });
			}
		});
		return () => {
			cancelled = true;
		};
	}, [
		ensureChildrenLoaded,
		hasFreshStoreFolderContents,
		isRootRoute,
		rootLoaded,
		storeCurrentFolderId,
		storeFolders,
		storeLoading,
		syncFolderChildren,
	]);

	useEffect(() => {
		if (storeLoading) return;
		if (rootLoaded && storeCurrentFolderId === null && isRootRoute) {
			syncFolderChildren(null, storeFolders);
		}
	}, [
		isRootRoute,
		rootLoaded,
		storeCurrentFolderId,
		storeFolders,
		storeLoading,
		syncFolderChildren,
	]);

	useEffect(() => {
		if (!storeLastFolderContents) return;
		if (storeLastFolderContents.sortBy !== sortBy) return;
		if (storeLastFolderContents.sortOrder !== sortOrder) return;
		if (
			storeLastFolderContents.workspaceRevision !==
			useFileStore.getState().workspaceRequestRevision
		) {
			return;
		}
		syncFolderChildren(
			storeLastFolderContents.folderId,
			storeLastFolderContents.folders,
		);
	}, [sortBy, sortOrder, storeLastFolderContents, syncFolderChildren]);

	useEffect(() => {
		if (storeLoading) return;
		if (!rootLoaded || storeCurrentFolderId === null) return;
		syncFolderChildren(storeCurrentFolderId, storeFolders);
	}, [
		rootLoaded,
		storeCurrentFolderId,
		storeFolders,
		storeLoading,
		syncFolderChildren,
	]);

	useEffect(() => {
		if (!rootLoaded || currentFolderId === null) return;

		const pathIds = breadcrumb
			.map((item) => item.id)
			.filter((id): id is number => id !== null);
		if (pathIds.length === 0) return;

		const pathKey = pathIds.join("/");
		if (expandingPathRef.current === pathKey) return;

		let cancelled = false;

		async function expandPath() {
			dispatchTree({ type: "expandRoot" });
			for (const folderId of pathIds) {
				if (cancelled) return;
				await ensureChildrenLoaded(folderId);
				if (cancelled) return;
				dispatchTree({ type: "expandFolder", folderId });
			}
			expandingPathRef.current = pathKey;
		}

		void expandPath();
		return () => {
			cancelled = true;
		};
	}, [breadcrumb, currentFolderId, ensureChildrenLoaded, rootLoaded]);

	// biome-ignore lint/correctness/useExhaustiveDependencies: reset marker whenever folder target changes
	useEffect(() => {
		expandingPathRef.current = "";
		clearHoverExpandTimer();
	}, [clearHoverExpandTimer, currentFolderId]);

	useEffect(() => () => clearHoverExpandTimer(), [clearHoverExpandTimer]);

	useEffect(() => {
		return subscribeStorageChange((event) => {
			const parentsToRefresh = decideFolderTreeStorageRefresh(event, {
				currentWorkspace: workspace,
				expandedFolderIds: expandedIds,
				folderParentIdsById: new Map(
					Array.from(nodeMap.entries(), ([folderId, node]) => [
						folderId,
						node.parentId,
					]),
				),
			});
			if (parentsToRefresh.length === 0) return;

			void Promise.all(
				parentsToRefresh.map((parentId) => refreshFolderChildren(parentId)),
			).catch(handleApiError);
		});
	}, [expandedIds, nodeMap, refreshFolderChildren, workspace]);

	const ensureFolderExpanded = useCallback(
		async (folderId: number) => {
			if (expandedIds.has(folderId)) return;
			await ensureChildrenLoaded(folderId);
			dispatchTree({ type: "expandFolder", folderId });
		},
		[ensureChildrenLoaded, expandedIds],
	);

	const handleToggle = useCallback(
		async (folderId: number) => {
			clearHoverExpandTimer();
			if (expandedIds.has(folderId)) {
				dispatchTree({ type: "collapseFolder", folderId });
				return;
			}

			await ensureFolderExpanded(folderId);
		},
		[clearHoverExpandTimer, ensureFolderExpanded, expandedIds],
	);

	const handleNavigate = useCallback(
		async (id: number, name: string) => {
			clearHoverExpandTimer();
			await ensureFolderExpanded(id);
			navigate(workspaceFolderPath(workspace, id, name));
		},
		[clearHoverExpandTimer, ensureFolderExpanded, navigate, workspace],
	);

	const handleDrop = useCallback(
		(
			fileIds: number[],
			folderIds: number[],
			targetFolderId: number,
			_targetPathIds: number[],
		) => {
			clearHoverExpandTimer();
			if (onMoveToFolder) {
				void Promise.resolve(
					onMoveToFolder(fileIds, folderIds, targetFolderId),
				).catch(handleApiError);
				return;
			}

			void moveToFolder(fileIds, folderIds, targetFolderId).catch(
				handleApiError,
			);
		},
		[clearHoverExpandTimer, moveToFolder, onMoveToFolder],
	);

	const scheduleHoverExpand = useCallback(
		(folderId: number) => {
			const node = nodeMap.get(folderId);
			if (!node) return;
			if (expandedIds.has(folderId)) return;
			if (loadingIds.has(folderId)) return;
			if (loadedIds.has(folderId) && node.childIds.length === 0) return;
			if (hoverExpandTargetIdRef.current === folderId) return;

			clearHoverExpandTimer();
			hoverExpandTargetIdRef.current = folderId;
			hoverExpandTimerRef.current = setTimeout(() => {
				hoverExpandTimerRef.current = null;
				const targetId = hoverExpandTargetIdRef.current;
				hoverExpandTargetIdRef.current = null;
				if (targetId == null) return;
				void ensureFolderExpanded(targetId);
			}, FOLDER_TREE_DRAG_EXPAND_DELAY_MS);
		},
		[
			clearHoverExpandTimer,
			ensureFolderExpanded,
			expandedIds,
			loadedIds,
			loadingIds,
			nodeMap,
		],
	);

	const handleRootDragOver = (event: DragEvent<HTMLDivElement>) => {
		if (!hasInternalDragData(event.dataTransfer)) return;
		event.preventDefault();
		event.dataTransfer.dropEffect = "move";
		clearHoverExpandTimer();
		setRootDragOver(true);
	};

	const handleRootDrop = (event: DragEvent<HTMLDivElement>) => {
		clearHoverExpandTimer();
		setRootDragOver(false);
		event.preventDefault();
		const data = readInternalDragData(event.dataTransfer);
		if (!data) return;
		if (getInvalidInternalDropReason(data, null, []) !== null) return;
		if (onMoveToFolder) {
			void Promise.resolve(
				onMoveToFolder(data.fileIds, data.folderIds, null),
			).catch(handleApiError);
			return;
		}
		void moveToFolder(data.fileIds, data.folderIds, null).catch(handleApiError);
	};

	const handleRootToggle = () => {
		clearHoverExpandTimer();
		dispatchTree({ type: "toggleRoot" });
	};

	const handleDragHoverStart = useCallback(
		(folderId: number) => {
			scheduleHoverExpand(folderId);
		},
		[scheduleHoverExpand],
	);

	const handleDragHoverEnd = useCallback(
		(folderId: number) => {
			if (hoverExpandTargetIdRef.current !== folderId) return;
			clearHoverExpandTimer();
		},
		[clearHoverExpandTimer],
	);

	const visibleRootIds = useMemo(
		() => rootIds.filter((id) => nodeMap.has(id)),
		[nodeMap, rootIds],
	);

	return {
		branchProps: {
			currentFolderId,
			expandedIds,
			loadedIds,
			loadingIds,
			nodeMap,
			onDragHoverEnd: handleDragHoverEnd,
			onDragHoverStart: handleDragHoverStart,
			onDrop: handleDrop,
			onNavigate: handleNavigate,
			onToggle: handleToggle,
		},
		rootProps: {
			active:
				currentFolderId === null &&
				location.pathname === workspaceRootPath(workspace),
			dragOver: rootDragOver,
			expanded: rootExpanded,
			onClick: () => navigate(workspaceRootPath(workspace)),
			onDragLeave: () => setRootDragOver(false),
			onDragOver: handleRootDragOver,
			onDrop: handleRootDrop,
			onToggle: handleRootToggle,
		},
		rootExpanded,
		rootLoaded,
		visibleRootIds,
	};
}
