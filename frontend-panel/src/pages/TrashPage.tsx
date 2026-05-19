import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonFileGrid } from "@/components/common/SkeletonFileGrid";
import { SkeletonFileTable } from "@/components/common/SkeletonFileTable";
import { ViewToggle } from "@/components/common/ViewToggle";
import { AppLayout } from "@/components/layout/AppLayout";
import { TrashBatchActionBar } from "@/components/trash/TrashBatchActionBar";
import { TrashGrid } from "@/components/trash/TrashGrid";
import { TrashTable } from "@/components/trash/TrashTable";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ItemCheckbox } from "@/components/ui/item-checkbox";
import { ScrollArea } from "@/components/ui/scroll-area";
import { STORAGE_KEYS } from "@/config/app";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useSelectionShortcuts } from "@/hooks/useSelectionShortcuts";
import { FOLDER_LIMIT } from "@/lib/constants";
import { formatBatchToast } from "@/lib/formatBatchToast";
import { subscribeStorageChange } from "@/lib/storageChangeBus";
import { trashService } from "@/services/trashService";
import { useAuthStore } from "@/stores/authStore";
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

function toTrashItems(contents: TrashContents): TrashItem[] {
	return [
		...contents.folders.map(
			(folder) =>
				({
					...folder,
					entity_type: "folder",
				}) as const,
		),
		...contents.files.map(
			(file) =>
				({
					...file,
					entity_type: "file",
				}) as const,
		),
	].sort(
		(a, b) =>
			new Date(b.expires_at).getTime() - new Date(a.expires_at).getTime(),
	);
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
	const [selectedKeys, setSelectedKeys] = useState<Set<string>>(new Set());
	const [loadingMore, setLoadingMore] = useState(false);
	const [pendingState, setPendingState] = useState<PendingTrashState | null>(
		null,
	);
	const pendingRef = useRef(false);
	const sentinelRef = useRef<HTMLDivElement | null>(null);

	const items = toTrashItems(contents);
	const totalItems = contents.files_total + contents.folders_total;
	const hasMoreFiles = contents.next_file_cursor != null;
	const hasMoreFolders = contents.folders.length < contents.folders_total;
	const hasMore = hasMoreFiles || hasMoreFolders;
	const selectedItems = items.filter((item) =>
		selectedKeys.has(getItemKey(item)),
	);
	const selectionCount = selectedItems.length;
	const allSelected = items.length > 0 && selectionCount === items.length;
	const isEmpty = !loading && totalItems === 0;
	const pendingKeys = pendingState?.keys ?? new Set<string>();
	const pendingOperation = pendingState?.operation ?? null;
	const isBusy = pendingState !== null;

	const TRASH_PAGE_SIZE = 100;

	const load = useCallback(async () => {
		setLoading(true);
		try {
			const data = await trashService.list({
				folder_limit: FOLDER_LIMIT,
				file_limit: TRASH_PAGE_SIZE,
			});
			setContents(data);
			setSelectedKeys(new Set());
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

	useEffect(() => {
		return subscribeStorageChange((event) => {
			if (event.kind !== "sync.required") {
				return;
			}
			void Promise.all([load(), refreshUser({ fields: ["quota"] })]);
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

	const toggleSelect = (item: TrashItem) => {
		if (pendingRef.current) return;

		const key = getItemKey(item);
		setSelectedKeys((prev) => {
			const next = new Set(prev);
			if (next.has(key)) next.delete(key);
			else next.add(key);
			return next;
		});
	};

	const clearSelection = useCallback(() => {
		if (pendingRef.current) return;

		setSelectedKeys(new Set());
	}, []);

	const selectAllItems = useCallback(() => {
		if (pendingRef.current) return;

		setSelectedKeys(new Set(items.map(getItemKey)));
	}, [items]);

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

		pendingRef.current = true;
		setPendingState({
			keys: new Set(items.map(getItemKey)),
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
			pendingRef.current = false;
			setPendingState(null);
		}
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

	return (
		<AppLayout
			actions={<ViewToggle value={viewMode} onChange={handleViewModeChange} />}
		>
			<div className="flex flex-1 flex-col gap-4 overflow-hidden p-4">
				<div className="rounded-xl border bg-muted/20 px-4 py-4">
					<div className="flex flex-col gap-4 md:flex-row md:items-start md:justify-between">
						<div className="flex items-center gap-3">
							<div className="flex h-11 w-11 items-center justify-center rounded-xl bg-destructive/10 text-destructive">
								<Icon name="Trash" className="h-5 w-5" />
							</div>
							<div className="min-w-0">
								<h1 className="text-lg font-semibold">{t("trash")}</h1>
								<p className="text-sm text-muted-foreground">
									{t("files:trash_page_desc")}
								</p>
							</div>
						</div>
						{!isEmpty && !loading ? (
							<Button
								variant="destructive"
								size="sm"
								className="self-start"
								disabled={isBusy}
								onClick={() => requestPurgeAllConfirm(true)}
							>
								<Icon
									name={pendingOperation === "purge-all" ? "Spinner" : "Trash"}
									className={`mr-1 h-4 w-4 ${pendingOperation === "purge-all" ? "animate-spin" : ""}`}
								/>
								{pendingOperation === "purge-all"
									? t("files:trash_purging")
									: t("admin:empty_trash")}
							</Button>
						) : null}
					</div>
				</div>

				{!loading && !isEmpty ? (
					<div className="flex items-center justify-between rounded-xl border bg-background px-4 py-3">
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
						<span className="hidden text-sm text-muted-foreground md:inline">
							{t("files:trash_page_desc")}
						</span>
					</div>
				) : null}

				<div className="min-h-0 flex flex-1 flex-col overflow-hidden rounded-xl border border-border/70 bg-card shadow-sm dark:shadow-none">
					{loading ? (
						viewMode === "grid" ? (
							<SkeletonFileGrid />
						) : (
							<SkeletonFileTable />
						)
					) : isEmpty ? (
						<EmptyState
							icon={<Icon name="Trash" className="h-10 w-10" />}
							title={t("files:trash_empty_title")}
							description={t("files:trash_empty_desc")}
						/>
					) : (
						<ScrollArea className="min-h-0 flex-1">
							{viewMode === "grid" ? (
								<TrashGrid
									items={items}
									actionsDisabled={isBusy}
									pendingKeys={pendingKeys}
									pendingOperation={pendingOperation}
									selectedKeys={selectedKeys}
									onToggleSelect={toggleSelect}
									onRestore={(item) => {
										void handleRestore([item]);
									}}
									onPurge={(item) => requestPurgeConfirm([item])}
								/>
							) : (
								<TrashTable
									items={items}
									allSelected={allSelected}
									actionsDisabled={isBusy}
									pendingKeys={pendingKeys}
									pendingOperation={pendingOperation}
									selectedKeys={selectedKeys}
									onToggleSelectAll={toggleSelectAll}
									onToggleSelect={toggleSelect}
									onRestore={(item) => {
										void handleRestore([item]);
									}}
									onPurge={(item) => requestPurgeConfirm([item])}
								/>
							)}
							{hasMore && (
								<div ref={sentinelRef} className="flex justify-center py-4">
									{loadingMore && (
										<div className="h-5 w-5 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-muted-foreground" />
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
