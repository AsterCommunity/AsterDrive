import type { DragEvent, ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";
import { SkeletonTree } from "@/components/common/SkeletonTree";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import {
	FOLDER_TREE_DRAG_EXPAND_DELAY_MS,
	SIDEBAR_SECTION_PADDING_CLASS,
} from "@/lib/constants";
import {
	getInvalidInternalDropReason,
	hasInternalDragData,
	readInternalDragData,
} from "@/lib/dragDrop";
import { folderTreeRowClass } from "@/lib/utils";
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
import { AnimatedTreeGroup } from "./folder-tree/AnimatedTreeGroup";
import { FolderTreeNodeRow } from "./folder-tree/FolderTreeNode";
import {
	cloneNodeEntries,
	getFolderTreeListParams,
	upsertChildren,
} from "./folder-tree/folderTreeState";
import type {
	FolderTreeNode,
	FolderTreeProps,
	FolderTreeSnapshot,
} from "./folder-tree/types";

let folderTreeSnapshot: FolderTreeSnapshot | null = null;

export function FolderTree({ onMoveToFolder }: FolderTreeProps = {}) {
	const { t } = useTranslation("files");
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

	const [nodeMap, setNodeMap] = useState<Map<number, FolderTreeNode>>(
		() => new Map(cachedSnapshot?.nodeEntries ?? []),
	);
	const [rootIds, setRootIds] = useState<number[]>(
		() => cachedSnapshot?.rootIds ?? [],
	);
	const [expandedIds, setExpandedIds] = useState<Set<number>>(
		() => new Set(cachedSnapshot?.expandedIds ?? []),
	);
	const [loadingIds, setLoadingIds] = useState<Set<number>>(() => new Set());
	const [loadedIds, setLoadedIds] = useState<Set<number>>(
		() => new Set(cachedSnapshot?.loadedIds ?? []),
	);
	const [rootExpanded, setRootExpanded] = useState(
		() => cachedSnapshot?.rootExpanded ?? true,
	);
	const [rootLoaded, setRootLoaded] = useState(
		() => cachedSnapshot !== null || rootIds.length > 0,
	);
	const [rootDragOver, setRootDragOver] = useState(false);

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
		setNodeMap(new Map());
		setRootIds([]);
		setExpandedIds(new Set());
		setLoadingIds(new Set());
		setLoadedIds(new Set());
		setRootExpanded(true);
		setRootLoaded(false);
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
			setNodeMap(
				(prev) =>
					upsertChildren(prev, parentId, folders, (id) => {
						const cachedChildren = childrenCacheRef.current.get(id);
						return cachedChildren?.map((folder) => folder.id);
					}).nodeMap,
			);
			if (parentId === null) {
				setRootIds(folders.map((folder) => folder.id));
				setRootLoaded(true);
			} else {
				setLoadedIds((prev) => new Set(prev).add(parentId));
			}
		},
		[],
	);

	const ensureChildrenLoaded = useCallback(
		async (parentId: number | null) => {
			if (parentId === null) {
				if (rootLoaded) return;
			} else if (loadedIds.has(parentId)) {
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
					setLoadingIds((prev) => new Set(prev).add(parentId));
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
						setLoadingIds((prev) => {
							const next = new Set(prev);
							next.delete(parentId);
							return next;
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
				setRootLoaded(false);
				const contents = await fileService.listRoot(
					getFolderTreeListParams(sortBy, sortOrder),
				);
				syncFolderChildren(null, contents.folders);
				return;
			}
			setLoadedIds((prev) => {
				const next = new Set(prev);
				next.delete(parentId);
				return next;
			});
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
		let cancelled = false;
		void ensureChildrenLoaded(null).catch(() => {
			if (!cancelled) {
				setRootLoaded(false);
			}
		});
		return () => {
			cancelled = true;
		};
	}, [ensureChildrenLoaded, rootLoaded]);

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
			setRootExpanded(true);
			for (const folderId of pathIds) {
				if (cancelled) return;
				await ensureChildrenLoaded(folderId);
				if (cancelled) return;
				setExpandedIds((prev) => new Set(prev).add(folderId));
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
		function onFolderTreeMove(event: Event) {
			const detail = (
				event as CustomEvent<{
					folderIds: number[];
					targetFolderId: number | null;
				}>
			).detail;
			if (!detail || detail.folderIds.length === 0) return;

			const sourceParentIds = detail.folderIds.map(
				(folderId) => nodeMap.get(folderId)?.parentId ?? null,
			);
			const parentsToRefresh = Array.from(
				new Set<number | null>([
					null,
					...expandedIds,
					...sourceParentIds,
					detail.targetFolderId,
				]),
			);

			void Promise.all(
				parentsToRefresh.map((parentId) => refreshFolderChildren(parentId)),
			).catch(handleApiError);
		}

		document.addEventListener("folder-tree-move", onFolderTreeMove);
		return () => {
			document.removeEventListener("folder-tree-move", onFolderTreeMove);
		};
	}, [expandedIds, nodeMap, refreshFolderChildren]);

	const ensureFolderExpanded = useCallback(
		async (folderId: number) => {
			if (expandedIds.has(folderId)) return;
			await ensureChildrenLoaded(folderId);
			setExpandedIds((prev) => new Set(prev).add(folderId));
		},
		[ensureChildrenLoaded, expandedIds],
	);

	const handleToggle = useCallback(
		async (folderId: number) => {
			clearHoverExpandTimer();
			if (expandedIds.has(folderId)) {
				setExpandedIds((prev) => {
					const next = new Set(prev);
					next.delete(folderId);
					return next;
				});
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

	const handleRootDragOver = (e: DragEvent) => {
		if (!hasInternalDragData(e.dataTransfer)) return;
		e.preventDefault();
		e.dataTransfer.dropEffect = "move";
		clearHoverExpandTimer();
		setRootDragOver(true);
	};

	const handleRootDrop = (e: DragEvent) => {
		clearHoverExpandTimer();
		setRootDragOver(false);
		e.preventDefault();
		const data = readInternalDragData(e.dataTransfer);
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
		setRootExpanded((prev) => !prev);
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

	function renderChildren(ids: number[], depth: number): ReactNode {
		return ids.map((id) => (
			<FolderTreeNodeRow
				key={id}
				currentFolderId={currentFolderId}
				depth={depth}
				expandedIds={expandedIds}
				loadedIds={loadedIds}
				loadingIds={loadingIds}
				nodeId={id}
				nodeMap={nodeMap}
				onDragHoverEnd={handleDragHoverEnd}
				onDragHoverStart={handleDragHoverStart}
				onDrop={handleDrop}
				onNavigate={handleNavigate}
				onToggle={handleToggle}
				renderChildren={renderChildren}
			/>
		));
	}

	const visibleRootIds = useMemo(
		() => rootIds.filter((id) => nodeMap.has(id)),
		[nodeMap, rootIds],
	);

	return (
		<div className={`${SIDEBAR_SECTION_PADDING_CLASS} py-2 space-y-0.5`}>
			{!rootLoaded ? (
				<SkeletonTree count={4} />
			) : (
				<>
					{/* biome-ignore lint/a11y/useSemanticElements: outer row needs drag-drop target and contains a nested toggle button */}
					<div
						role="button"
						tabIndex={0}
						aria-expanded={rootExpanded}
						className={folderTreeRowClass(
							currentFolderId === null &&
								location.pathname === workspaceRootPath(workspace),
							rootDragOver && "ring-2 ring-primary bg-accent/30",
						)}
						onClick={() => navigate(workspaceRootPath(workspace))}
						onKeyDown={(e) => {
							if (e.key === "Enter" || e.key === " ") {
								e.preventDefault();
								navigate(workspaceRootPath(workspace));
							}
						}}
						onDragOver={handleRootDragOver}
						onDragLeave={() => setRootDragOver(false)}
						onDrop={handleRootDrop}
					>
						<button
							type="button"
							aria-label={t(rootExpanded ? "collapse_tree" : "expand_tree")}
							className="shrink-0 rounded p-0.5 text-muted-foreground hover:bg-accent-foreground/10 hover:text-foreground"
							onClick={(e) => {
								e.stopPropagation();
								handleRootToggle();
							}}
						>
							<Icon
								name="CaretRight"
								className={`size-3 text-muted-foreground transition-transform duration-200 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none ${
									rootExpanded ? "rotate-90" : "rotate-0"
								}`}
							/>
						</button>
						<div className="flex min-w-0 flex-1 items-center gap-2 px-1">
							<Icon
								name={rootExpanded ? "FolderOpen" : "Folder"}
								className="size-4 shrink-0 text-muted-foreground"
							/>
							<span className="truncate">{t("root")}</span>
						</div>
					</div>
					<AnimatedTreeGroup open={rootExpanded && visibleRootIds.length > 0}>
						{renderChildren(visibleRootIds, 1)}
					</AnimatedTreeGroup>
				</>
			)}
		</div>
	);
}
