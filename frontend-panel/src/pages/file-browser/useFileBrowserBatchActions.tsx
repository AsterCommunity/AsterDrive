import {
	type ReactNode,
	useCallback,
	useEffect,
	useMemo,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { BatchTargetFolderDialog } from "@/components/files/BatchTargetFolderDialog";
import {
	TagManagerDialog,
	type TagManagerTarget,
} from "@/components/files/TagManagerDialog";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { formatBatchToast } from "@/lib/formatBatchToast";
import { beginLocalStorageDeleteMutation } from "@/lib/storageMutationCoordinator";
import type { Workspace } from "@/lib/workspace";
import { batchService, resolveCopyDispatch } from "@/services/batchService";
import { useFileStore } from "@/stores/fileStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { BatchResult, FileListItem, FolderListItem } from "@/types/api";
import type { FileBrowserSelectionToolbarState } from "./types";

interface UseFileBrowserBatchActionsOptions {
	allowCopyMove?: boolean;
	allowDelete?: boolean;
	allowTagManagement?: boolean;
	displayFiles: FileListItem[];
	displayFolders: FolderListItem[];
	onChanged?: () => Promise<void> | void;
	onMoveToFolder?: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<BatchResult> | BatchResult;
	onArchiveCompress?: (
		fileIds: number[],
		folderIds: number[],
	) => Promise<void> | void;
	onArchiveDownload?: (
		fileIds: number[],
		folderIds: number[],
	) => Promise<void> | void;
	onDownload?: (fileId: number, fileName: string) => void;
}

interface UseFileBrowserBatchActionsResult {
	dialogs: ReactNode;
	selectionToolbar: FileBrowserSelectionToolbarState | null;
}

export function useFileBrowserBatchActions({
	allowCopyMove = true,
	allowDelete = true,
	allowTagManagement = true,
	displayFiles,
	displayFolders,
	onChanged,
	onMoveToFolder,
	onArchiveCompress,
	onArchiveDownload,
	onDownload,
}: UseFileBrowserBatchActionsOptions): UseFileBrowserBatchActionsResult {
	const { t } = useTranslation(["files", "tasks"]);
	const selectedFileIds = useFileStore((s) => s.selectedFileIds);
	const selectedFolderIds = useFileStore((s) => s.selectedFolderIds);
	const clearSelection = useFileStore((s) => s.clearSelection);
	const selectItems = useFileStore((s) => s.selectItems);
	const refresh = useFileStore((s) => s.refresh);
	const moveToFolder = useFileStore((s) => s.moveToFolder);
	const currentFolderId = useFileStore((s) => s.currentFolderId);
	const breadcrumb = useFileStore((s) => s.breadcrumb);
	const [targetDialogMode, setTargetDialogMode] = useState<
		"move" | "copy" | null
	>(null);
	const [tagManagerOpen, setTagManagerOpen] = useState(false);

	const fileIds = useMemo(() => Array.from(selectedFileIds), [selectedFileIds]);
	const folderIds = useMemo(
		() => Array.from(selectedFolderIds),
		[selectedFolderIds],
	);
	const displayedFileIds = useMemo(
		() => displayFiles.map((file) => file.id),
		[displayFiles],
	);
	const displayedFolderIds = useMemo(
		() => displayFolders.map((folder) => folder.id),
		[displayFolders],
	);
	const count = fileIds.length + folderIds.length;
	const displayedCount = displayedFileIds.length + displayedFolderIds.length;
	const hasDisplayedItems = displayedCount > 0;
	const allDisplayedSelected =
		hasDisplayedItems &&
		count === displayedCount &&
		displayedFileIds.every((id) => selectedFileIds.has(id)) &&
		displayedFolderIds.every((id) => selectedFolderIds.has(id));
	const refreshVisibleItems = useCallback(async () => {
		if (onChanged) {
			await onChanged();
			return;
		}
		await refresh();
	}, [onChanged, refresh]);

	useEffect(() => {
		if (count === 0) {
			setTargetDialogMode(null);
			setTagManagerOpen(false);
		}
	}, [count]);

	const handleDelete = useCallback(async () => {
		const mutation = beginLocalStorageDeleteMutation({
			workspace: useWorkspaceStore.getState().workspace,
			fileIds,
			folderIds,
		});
		try {
			const result = await batchService.batchDelete(fileIds, folderIds);
			const batchToast = formatBatchToast(t, "delete", result);
			if (batchToast.variant === "error") {
				toast.error(batchToast.title, { description: batchToast.description });
			} else {
				toast.success(batchToast.title, {
					description: batchToast.description,
				});
			}
			clearSelection();
			await refreshVisibleItems();
		} catch (err) {
			mutation.rollback();
			handleApiError(err);
		}
	}, [clearSelection, fileIds, folderIds, refreshVisibleItems, t]);

	const {
		requestConfirm: requestDeleteConfirm,
		dialogProps: deleteDialogProps,
	} = useConfirmDialog<true>(handleDelete);

	const handleMove = useCallback(() => {
		setTargetDialogMode("move");
	}, []);

	const handleCopy = useCallback(() => {
		setTargetDialogMode("copy");
	}, []);

	const handleManageTags = useCallback(() => {
		setTagManagerOpen(true);
	}, []);

	const isSingleSelectedFile = fileIds.length === 1 && folderIds.length === 0;
	const selectedSingleFile = useMemo(
		() =>
			isSingleSelectedFile
				? displayFiles.find((file) => file.id === fileIds[0])
				: undefined,
		[displayFiles, fileIds, isSingleSelectedFile],
	);

	const handleSelectionDownload = useCallback(async () => {
		if (isSingleSelectedFile && onDownload) {
			onDownload(fileIds[0], selectedSingleFile?.name ?? "");
			clearSelection();
			return;
		}

		if (!onArchiveDownload) return;
		try {
			await onArchiveDownload(fileIds, folderIds);
			clearSelection();
		} catch (err) {
			handleApiError(err);
		}
	}, [
		clearSelection,
		fileIds,
		folderIds,
		isSingleSelectedFile,
		onArchiveDownload,
		onDownload,
		selectedSingleFile,
	]);

	const handleArchiveCompress = useCallback(async () => {
		if (!onArchiveCompress) return;
		try {
			await onArchiveCompress(fileIds, folderIds);
		} catch (err) {
			handleApiError(err);
		}
	}, [fileIds, folderIds, onArchiveCompress]);

	const handleToggleDisplayedSelection = useCallback(() => {
		if (allDisplayedSelected) {
			clearSelection();
			return;
		}

		selectItems(displayedFileIds, displayedFolderIds);
	}, [
		allDisplayedSelected,
		clearSelection,
		displayedFileIds,
		displayedFolderIds,
		selectItems,
	]);

	const handleTargetConfirm = useCallback(
		async ({
			workspace: targetWorkspace,
			folderId: targetFolderId,
		}: {
			workspace: Workspace;
			folderId: number | null;
		}) => {
			if (!targetDialogMode) return;

			try {
				const currentWorkspace = useWorkspaceStore.getState().workspace;
				const customMoveHandler =
					targetDialogMode === "move" ? onMoveToFolder : undefined;
				const result =
					targetDialogMode === "move"
						? await (customMoveHandler ?? moveToFolder)(
								fileIds,
								folderIds,
								targetFolderId,
							)
						: await resolveCopyDispatch({
								currentWorkspace,
								targetWorkspace,
								fileIds,
								folderIds,
								targetFolderId,
							});
				const batchToast = formatBatchToast(t, targetDialogMode, result);
				if (batchToast.variant === "error") {
					toast.error(batchToast.title, {
						description: batchToast.description,
					});
				} else {
					toast.success(batchToast.title, {
						description: batchToast.description,
					});
				}
				if (targetDialogMode === "copy" || customMoveHandler) {
					clearSelection();
					await refreshVisibleItems();
				}
				setTargetDialogMode(null);
			} catch (err) {
				handleApiError(err);
			}
		},
		[
			clearSelection,
			fileIds,
			folderIds,
			moveToFolder,
			onMoveToFolder,
			refreshVisibleItems,
			t,
			targetDialogMode,
		],
	);

	const selectionToolbar =
		count > 0
			? {
					count,
					allDisplayedSelected,
					downloadAction:
						isSingleSelectedFile && onDownload
							? {
									kind: "file" as const,
									onClick: handleSelectionDownload,
								}
							: onArchiveDownload
								? {
										kind: "archive" as const,
										onClick: handleSelectionDownload,
									}
								: undefined,
					hasDisplayedItems,
					onArchiveCompress: onArchiveCompress
						? handleArchiveCompress
						: undefined,
					onClearSelection: clearSelection,
					onCopy: allowCopyMove ? handleCopy : undefined,
					onDelete: allowDelete ? () => requestDeleteConfirm(true) : undefined,
					onManageTags: allowTagManagement ? handleManageTags : undefined,
					onMove: allowCopyMove ? handleMove : undefined,
					onToggleDisplayedSelection: handleToggleDisplayedSelection,
				}
			: null;
	const tagManagerTarget = useMemo<TagManagerTarget | null>(
		() =>
			allowTagManagement && count > 0
				? {
						mode: "batch",
						count,
						fileIds,
						folderIds,
						onChanged: refreshVisibleItems,
					}
				: null,
		[allowTagManagement, count, fileIds, folderIds, refreshVisibleItems],
	);

	return {
		selectionToolbar,
		dialogs: (
			<>
				<ConfirmDialog
					{...deleteDialogProps}
					title={t("batch_delete_confirm_title", { count })}
					description={t("batch_delete_confirm_desc")}
					confirmLabel={t("core:delete")}
					variant="destructive"
				/>

				<BatchTargetFolderDialog
					open={targetDialogMode !== null}
					onOpenChange={(open) => {
						if (!open) setTargetDialogMode(null);
					}}
					mode={targetDialogMode ?? "move"}
					onConfirm={handleTargetConfirm}
					currentFolderId={currentFolderId}
					initialBreadcrumb={breadcrumb}
					selectedFolderIds={folderIds}
				/>

				<TagManagerDialog
					open={tagManagerOpen}
					onOpenChange={setTagManagerOpen}
					target={tagManagerTarget}
				/>
			</>
		),
	};
}
