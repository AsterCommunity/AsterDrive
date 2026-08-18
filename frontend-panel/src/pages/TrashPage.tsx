import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonFileGrid } from "@/components/common/SkeletonFileGrid";
import { SkeletonFileTable } from "@/components/common/SkeletonFileTable";
import { ViewToggle } from "@/components/common/ViewToggle";
import {
	type FileBrowserContextValue,
	FileBrowserProvider,
} from "@/components/files/FileBrowserContext";
import { FileGrid } from "@/components/files/FileGrid";
import { FileTable } from "@/components/files/FileTable";
import { AppLayout } from "@/components/layout/AppLayout";
import { TrashBatchActionBar } from "@/components/trash/TrashBatchActionBar";
import {
	buildTrashMetaMap,
	toBrowserFiles,
	toBrowserFolders,
} from "@/components/trash/trashBrowserItems";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ItemCheckbox } from "@/components/ui/item-checkbox";
import { ScrollArea } from "@/components/ui/scroll-area";
import { STORAGE_KEYS } from "@/config/app";
import { handleApiError } from "@/hooks/useApiError";
import { useBottomOverlayOffset } from "@/hooks/useBottomOverlayOffset";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { usePendingAction } from "@/hooks/usePendingAction";
import { useSelectionShortcuts } from "@/hooks/useSelectionShortcuts";
import { FOLDER_LIMIT, getBottomOverlayPaddingClass } from "@/lib/constants";
import { formatBatchToast } from "@/lib/formatBatchToast";
import { subscribeStorageChange } from "@/lib/storageChangeBus";
import { cn } from "@/lib/utils";
import { trashService } from "@/services/trashService";
import { useAuthStore } from "@/stores/authStore";
import { useFileStore } from "@/stores/fileStore";
import type { TrashContents } from "@/types/api";
import type { TrashItem } from "@/types/api-helpers";

type ViewMode = "grid" | "list";
type TrashOperation = "restore" | "purge";
type TrashPendingOperation = TrashOperation | "purge-all";

interface PendingTrashState {
	keys: Set<string>;
	operation: TrashPendingOperation;
}

function getStoredViewMode(): ViewMode {
	if (typeof window === "undefined") return "list";
	const stored = localStorage.getItem(STORAGE_KEYS.trashViewMode);
	return stored === "grid" ? "grid" : "list";
}

function getItemKey(item: TrashItem) {
	return `${item.entity_type}:${item.id}`;
}

export default function TrashPage() {
	const { t } = useTranslation(["core", "files", "admin", "tasks"]);
	usePageTitle(t("core:trash"));
	const refreshUser = useAuthStore((s) => s.refreshUser);
	const [contents, setContents] = useState<TrashContents>({
		files: [],
		folders: [],
		files_total: 0,
		folders_total: 0,
	});
	const [loading, setLoading] = useState(true);
	const [viewMode, setViewMode] = useState<ViewMode>(getStoredViewMode);
	const [loadingMore, setLoadingMore] = useState(false);
	const [pendingState, setPendingState] = useState<PendingTrashState | null>(
		null,
	);
	const { pending: purgeAllPending, runWithPending: runPurgeAllWithPending } =
		usePendingAction();
	const pendingRef = useRef(false);
	const syncInFlightRef = useRef(false);
	const sentinelRef = useRef<HTMLDivElement | null>(null);

	// D9 Finder 化：回收站复用文件浏览器（trashMode），选择状态走 fileStore
	const selectedFileIds = useFileStore((s) => s.selectedFileIds);
	const selectedFolderIds = useFileStore((s) => s.selectedFolderIds);
	const toggleFileSelection = useFileStore((s) => s.toggleFileSelection);
	const toggleFolderSelection = useFileStore((s) => s.toggleFolderSelection);
	const selectItems = useFileStore((s) => s.selectItems);
	const clearBrowserSelection = useFileStore((s) => s.clearSelection);

	const folders = useMemo(
		() => toBrowserFolders(contents.folders),
		[contents.folders],
	);
	const files = useMemo(() => toBrowserFiles(contents.files), [contents.files]);
	const trashMetaMap = useMemo(() => buildTrashMetaMap(contents), [contents]);

	const itemCount = folders.length + files.length;
	const totalItems = contents.files_total + contents.folders_total;
	const hasMoreFiles = contents.next_file_cursor != null;
	const hasMoreFolders = contents.folders.length < contents.folders_total;
	const hasMore = hasMoreFiles || hasMoreFolders;
	const selectionCount = selectedFileIds.size + selectedFolderIds.size;
	const allSelected = itemCount > 0 && selectionCount === itemCount;
	const isEmpty = !loading && totalItems === 0;
	const pendingKeys = pendingState?.keys ?? new Set<string>();
	const pendingOperation = pendingState?.operation ?? null;
	const isBusy = pendingState !== null || purgeAllPending;
	const bottomOverlayOffset = useBottomOverlayOffset(selectionCount > 0);
	const bottomOverlayPadding =
		getBottomOverlayPaddingClass(bottomOverlayOffset);

	const fadingIds = useMemo(() => {
		const fileIds = new Set<number>();
		const folderIds = new Set<number>();
		for (const key of pendingKeys) {
			const [type, rawId] = key.split(":");
			const id = Number(rawId);
			if (type === "file") fileIds.add(id);
			else if (type === "folder") folderIds.add(id);
		}
		return { fileIds, folderIds };
	}, [pendingKeys]);

	const selectedItems = useMemo<TrashItem[]>(
		() => [
			...contents.folders
				.filter((folder) => selectedFolderIds.has(folder.id))
				.map((folder) => ({ ...folder, entity_type: "folder" as const })),
			...contents.files
				.filter((file) => selectedFileIds.has(file.id))
				.map((file) => ({ ...file, entity_type: "file" as const })),
		],
		[contents, selectedFileIds, selectedFolderIds],
	);

	const TRASH_PAGE_SIZE = 100;

	const load = useCallback(async () => {
		setLoading(true);
		try {
			const data = await trashService.list({
				folder_limit: FOLDER_LIMIT,
				file_limit: TRASH_PAGE_SIZE,
			});
			setContents(data);
			useFileStore.getState().clearSelection();
		} catch (err) {
			handleApiError(err);
		} finally {
			setLoading(false);
		}
	}, []);

	const loadMore = useCallback(async () => {
		if (loadingMore || (!hasMoreFiles && !hasMoreFolders)) return;
		setLoadingMore(true);
		try {
			const data = await trashService.list({
				folder_limit: hasMoreFolders ? FOLDER_LIMIT : 0,
				folder_offset: hasMoreFolders ? contents.folders.length : 0,
				file_limit: hasMoreFiles ? TRASH_PAGE_SIZE : 0,
				file_after_expires_at: contents.next_file_cursor?.expires_at,
				file_after_id: contents.next_file_cursor?.id,
			});
			setContents((prev) => ({
				...prev,
				folders: [...prev.folders, ...data.folders],
				files: [...prev.files, ...data.files],
				folders_total: data.folders_total,
				files_total: data.files_total,
				next_file_cursor: data.next_file_cursor,
			}));
		} catch (err) {
			handleApiError(err);
		} finally {
			setLoadingMore(false);
		}
	}, [
		contents.folders.length,
		contents.next_file_cursor,
		hasMoreFiles,
		hasMoreFolders,
		loadingMore,
	]);

	useEffect(() => {
		void load();
	}, [load]);

	// 离开回收站时清掉浏览器选择，避免串回文件页
	useEffect(() => {
		return () => useFileStore.getState().clearSelection();
	}, []);

	useEffect(() => {
		return subscribeStorageChange((event) => {
			if (event.kind !== "sync.required") {
				return;
			}
			if (syncInFlightRef.current) {
				return;
			}
			syncInFlightRef.current = true;
			void Promise.all([load(), refreshUser({ fields: ["quota"] })]).finally(
				() => {
					syncInFlightRef.current = false;
				},
			);
		});
	}, [load, refreshUser]);

	// Infinite scroll
	useEffect(() => {
		if (!hasMore || loadingMore) return;
		const el = sentinelRef.current;
		if (!el) return;
		const observer = new IntersectionObserver(
			(entries) => {
				if (entries[0].isIntersecting) void loadMore();
			},
			{ rootMargin: "200px" },
		);
		observer.observe(el);
		return () => observer.disconnect();
	}, [hasMore, loadingMore, loadMore]);

	const handleViewModeChange = (mode: ViewMode) => {
		localStorage.setItem(STORAGE_KEYS.trashViewMode, mode);
		setViewMode(mode);
	};

	const clearSelection = useCallback(() => {
		if (pendingRef.current || purgeAllPending) return;
		clearBrowserSelection();
	}, [clearBrowserSelection, purgeAllPending]);

	const selectAllItems = useCallback(() => {
		if (pendingRef.current || purgeAllPending) return;
		selectItems(
			files.map((file) => file.id),
			folders.map((folder) => folder.id),
		);
	}, [files, folders, selectItems, purgeAllPending]);

	const toggleSelectAll = useCallback(() => {
		if (allSelected) {
			clearSelection();
			return;
		}
		selectAllItems();
	}, [allSelected, clearSelection, selectAllItems]);

	const runOperation = useCallback(
		async (targets: TrashItem[], operation: TrashOperation) => {
			if (targets.length === 0 || pendingRef.current) return;

			pendingRef.current = true;
			setPendingState({
				keys: new Set(targets.map(getItemKey)),
				operation,
			});
			try {
				const results = await Promise.allSettled(
					targets.map(async (item) => {
						if (operation === "restore") {
							if (item.entity_type === "file") {
								await trashService.restoreFile(item.id);
							} else {
								await trashService.restoreFolder(item.id);
							}
							return;
						}

						if (item.entity_type === "file") {
							await trashService.purgeFile(item.id);
						} else {
							await trashService.purgeFolder(item.id);
						}
					}),
				);

				const succeeded = results.filter(
					(result) => result.status === "fulfilled",
				).length;
				const failed = results.length - succeeded;

				const toastContent = formatBatchToast(t, operation, {
					succeeded,
					failed,
					errors: [],
				});
				if (toastContent.variant === "success") {
					toast.success(toastContent.title);
				} else {
					toast.error(toastContent.title);
				}

				if (succeeded > 0) {
					if (operation === "purge") {
						await Promise.all([load(), refreshUser({ fields: ["quota"] })]);
					} else {
						await load();
					}
				}
			} finally {
				pendingRef.current = false;
				setPendingState(null);
			}
		},
		[load, refreshUser, t],
	);

	const handleRestore = useCallback(
		async (targets: TrashItem[]) => {
			try {
				await runOperation(targets, "restore");
			} catch (err) {
				handleApiError(err);
			}
		},
		[runOperation],
	);

	const handlePurge = useCallback(
		async (targets: TrashItem[]) => {
			try {
				await runOperation(targets, "purge");
			} catch (err) {
				handleApiError(err);
			}
		},
		[runOperation],
	);

	const handlePurgeAll = async () => {
		if (pendingRef.current) return;

		const result = await runPurgeAllWithPending(async () => {
			setPendingState({
				keys: new Set([
					...contents.files.map((file) => `file:${file.id}`),
					...contents.folders.map((folder) => `folder:${folder.id}`),
				]),
				operation: "purge-all",
			});
			try {
				const task = await trashService.purgeAll();
				toast.success(t("tasks:task_created_success"), {
					description: task.display_name,
				});
			} catch (err) {
				handleApiError(err);
			} finally {
				setPendingState(null);
			}
		});

		if (!result.entered) return;
	};
	const {
		confirmId: purgeTargets,
		requestConfirm: requestPurgeConfirm,
		dialogProps: purgeDialogProps,
	} = useConfirmDialog<TrashItem[]>(handlePurge);
	const {
		requestConfirm: requestPurgeAllConfirm,
		dialogProps: purgeAllDialogProps,
	} = useConfirmDialog<true>(handlePurgeAll);

	useSelectionShortcuts({
		selectAll: selectAllItems,
		clearSelection,
		enabled: purgeTargets === null && !purgeAllDialogProps.open && !isBusy,
	});

	const findTrashItem = useCallback(
		(type: "file" | "folder", id: number): TrashItem | undefined => {
			if (type === "folder") {
				const folder = contents.folders.find((item) => item.id === id);
				return folder ? { ...folder, entity_type: "folder" } : undefined;
			}
			const file = contents.files.find((item) => item.id === id);
			return file ? { ...file, entity_type: "file" } : undefined;
		},
		[contents],
	);

	const fileBrowserContextValue = useMemo<FileBrowserContextValue>(
		() => ({
			folders,
			files,
			browserOpenMode: "single_click",
			readOnly: true,
			selectionEnabled: true,
			trashMode: true,
			breadcrumbPathIds: [],
			fadingFileIds: fadingIds.fileIds,
			fadingFolderIds: fadingIds.folderIds,
			getTrashMeta: (type, id) => trashMetaMap.get(`${type}:${id}`),
			onTrashRestore: (type, id) => {
				const item = findTrashItem(type, id);
				if (item) void handleRestore([item]);
			},
			onTrashPurge: (type, id) => {
				const item = findTrashItem(type, id);
				if (item) requestPurgeConfirm([item]);
			},
			batchSelectionActions:
				selectionCount > 0
					? {
							count: selectionCount,
							onRestore: () => {
								void handleRestore(selectedItems);
							},
							onPurge: () => requestPurgeConfirm(selectedItems),
						}
					: null,
			// 回收站条目不可打开：单击条目即切换选择（沿用旧 TrashGrid 的交互）
			onFolderOpen: (id) => {
				if (!pendingRef.current) toggleFolderSelection(id);
			},
			onFileClick: (file) => {
				if (!pendingRef.current) toggleFileSelection(file.id);
			},
			onShare: () => {},
			onDownload: () => {},
			onToggleLock: () => false,
			onDelete: () => undefined,
		}),
		[
			folders,
			files,
			fadingIds,
			trashMetaMap,
			findTrashItem,
			handleRestore,
			requestPurgeConfirm,
			selectionCount,
			selectedItems,
			toggleFileSelection,
			toggleFolderSelection,
		],
	);

	return (
		<AppLayout>
			<div className="flex flex-1 flex-col gap-4 overflow-hidden p-4">
				<div className="px-1 py-2">
					<div className="flex flex-col gap-4 md:flex-row md:items-start md:justify-between">
						<div className="flex items-center gap-3">
							<div className="flex size-11 items-center justify-center rounded-xl bg-destructive/10 text-destructive">
								<Icon name="Trash" className="size-5" />
							</div>
							<div className="min-w-0">
								<h1 className="text-lg font-semibold">{t("trash")}</h1>
								<p className="text-sm text-muted-foreground">
									{t("files:trash_page_desc")}
								</p>
							</div>
						</div>
						<div className="flex shrink-0 items-center gap-2 self-start">
							<ViewToggle value={viewMode} onChange={handleViewModeChange} />
							{!isEmpty && !loading ? (
								<Button
									variant="destructive"
									size="sm"
									disabled={isBusy}
									onClick={() => requestPurgeAllConfirm(true)}
								>
									<Icon
										name={
											pendingOperation === "purge-all" ? "Spinner" : "Trash"
										}
										className={`mr-1 size-4 ${pendingOperation === "purge-all" ? "animate-spin" : ""}`}
									/>
									{pendingOperation === "purge-all"
										? t("files:trash_purging")
										: t("admin:empty_trash")}
								</Button>
							) : null}
						</div>
					</div>
				</div>

				{!loading && !isEmpty ? (
					<div className="flex items-center justify-between px-1 py-1">
						<div className="flex items-center gap-3">
							{viewMode === "grid" ? (
								<ItemCheckbox
									checked={allSelected}
									onChange={toggleSelectAll}
								/>
							) : null}
							<span className="text-sm font-medium">
								{selectionCount > 0
									? t("selected_count", { count: selectionCount })
									: t("items_count", { count: totalItems })}
							</span>
						</div>
					</div>
				) : null}

				<div className="min-h-0 flex flex-1 flex-col overflow-hidden">
					{loading ? (
						viewMode === "grid" ? (
							<SkeletonFileGrid />
						) : (
							<SkeletonFileTable />
						)
					) : isEmpty ? (
						<EmptyState
							icon={<Icon name="Trash" className="size-10" />}
							title={t("files:trash_empty_title")}
							description={t("files:trash_empty_desc")}
						/>
					) : (
						<ScrollArea
							className="min-h-0 flex-1"
							viewportProps={{
								className: cn(bottomOverlayPadding),
							}}
						>
							<FileBrowserProvider value={fileBrowserContextValue}>
								{viewMode === "grid" ? <FileGrid /> : <FileTable />}
							</FileBrowserProvider>
							{hasMore && (
								<div ref={sentinelRef} className="flex justify-center py-4">
									{loadingMore && (
										<div className="size-5 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-muted-foreground" />
									)}
								</div>
							)}
						</ScrollArea>
					)}
				</div>
			</div>

			<TrashBatchActionBar
				count={selectionCount}
				pendingOperation={pendingOperation}
				onRestore={() => {
					void handleRestore(selectedItems);
				}}
				onPurge={() => requestPurgeConfirm(selectedItems)}
				onClearSelection={clearSelection}
			/>

			<ConfirmDialog
				{...purgeDialogProps}
				title={t("files:trash_purge_confirm_title", {
					count: purgeTargets?.length ?? 0,
				})}
				description={t("files:trash_purge_confirm_desc")}
				confirmLabel={t("files:trash_delete_permanently")}
				variant="destructive"
			/>

			<ConfirmDialog
				{...purgeAllDialogProps}
				title={t("are_you_sure")}
				description={t("admin:confirm_empty_trash")}
				confirmLabel={t("admin:empty_trash")}
				variant="destructive"
			/>
		</AppLayout>
	);
}
