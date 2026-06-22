import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { BatchTargetFolderDialog } from "@/components/files/BatchTargetFolderDialog";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { formatBatchToast } from "@/lib/formatBatchToast";
import { beginLocalStorageDeleteMutation } from "@/lib/storageMutationCoordinator";
import { batchService } from "@/services/batchService";
import type { BreadcrumbItem } from "@/stores/fileStore";
import { useFileStore } from "@/stores/fileStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";

interface BatchActionBarProps {
	onArchiveCompress?: (
		fileIds: number[],
		folderIds: number[],
	) => Promise<void> | void;
	onArchiveDownload?: (
		fileIds: number[],
		folderIds: number[],
	) => Promise<void> | void;
}

export function BatchActionBar({
	onArchiveCompress,
	onArchiveDownload,
}: BatchActionBarProps) {
	const { t } = useTranslation(["files", "tasks"]);
	const selectedFileIds = useFileStore((s) => s.selectedFileIds);
	const selectedFolderIds = useFileStore((s) => s.selectedFolderIds);
	const clearSelection = useFileStore((s) => s.clearSelection);
	const refresh = useFileStore((s) => s.refresh);
	const moveToFolder = useFileStore((s) => s.moveToFolder);
	const currentFolderId = useFileStore((s) => s.currentFolderId);
	const breadcrumb = useFileStore((s) => s.breadcrumb);
	const [targetDialogMode, setTargetDialogMode] = useState<
		"move" | "copy" | null
	>(null);

	const fileIds = Array.from(selectedFileIds);
	const folderIds = Array.from(selectedFolderIds);
	const count = fileIds.length + folderIds.length;

	const handleDelete = async () => {
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
			await refresh();
		} catch (err) {
			mutation.rollback();
			handleApiError(err);
		}
	};
	const {
		requestConfirm: requestDeleteConfirm,
		dialogProps: deleteDialogProps,
	} = useConfirmDialog<true>(handleDelete);

	if (count === 0) return null;

	const handleMove = () => {
		setTargetDialogMode("move");
	};

	const handleCopy = () => {
		setTargetDialogMode("copy");
	};

	const handleArchiveDownload = async () => {
		if (!onArchiveDownload) return;
		try {
			await onArchiveDownload(fileIds, folderIds);
			clearSelection();
		} catch (err) {
			handleApiError(err);
		}
	};

	const handleArchiveCompress = async () => {
		if (!onArchiveCompress) return;
		try {
			await onArchiveCompress(fileIds, folderIds);
		} catch (err) {
			handleApiError(err);
		}
	};

	const handleTargetConfirm = async (targetFolderId: number | null) => {
		if (!targetDialogMode) return;

		try {
			const result =
				targetDialogMode === "move"
					? await moveToFolder(fileIds, folderIds, targetFolderId)
					: await batchService.batchCopy(fileIds, folderIds, targetFolderId);
			const batchToast = formatBatchToast(t, targetDialogMode, result);
			if (batchToast.variant === "error") {
				toast.error(batchToast.title, { description: batchToast.description });
			} else {
				toast.success(batchToast.title, {
					description: batchToast.description,
				});
			}
			if (targetDialogMode === "copy") {
				clearSelection();
				await refresh();
			}
			setTargetDialogMode(null);
		} catch (err) {
			handleApiError(err);
		}
	};

	return (
		<>
			<div className="fixed bottom-4 left-1/2 z-(--z-fixed) flex -translate-x-1/2 items-center gap-2 rounded-xl border border-border/70 bg-card/95 px-4 py-2 shadow-lg shadow-black/8 backdrop-blur supports-[backdrop-filter]:bg-card/85 dark:shadow-none">
				<span className="text-sm font-medium">
					{t("core:selected_count", { count })}
				</span>
				<div className="flex items-center gap-1">
					<Button
						size="sm"
						variant="destructive"
						onClick={() => requestDeleteConfirm(true)}
					>
						<Icon name="Trash" className="size-3.5 mr-1" />
						{t("core:delete")}
					</Button>
					<Button size="sm" variant="outline" onClick={handleMove}>
						<Icon name="ArrowsOutCardinal" className="size-3.5 mr-1" />
						{t("move_to")}
					</Button>
					<Button size="sm" variant="outline" onClick={handleCopy}>
						<Icon name="Copy" className="size-3.5 mr-1" />
						{t("copy_to")}
					</Button>
					{onArchiveCompress ? (
						<Button size="sm" variant="outline" onClick={handleArchiveCompress}>
							<Icon name="FileZip" className="size-3.5 mr-1" />
							{t("tasks:archive_compress_action")}
						</Button>
					) : null}
					{onArchiveDownload ? (
						<Button size="sm" variant="outline" onClick={handleArchiveDownload}>
							<Icon name="Download" className="size-3.5 mr-1" />
							{t("tasks:archive_download_action")}
						</Button>
					) : null}
				</div>
				<Button size="sm" variant="ghost" onClick={clearSelection}>
					<Icon name="X" className="size-3.5" />
				</Button>
			</div>

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
				initialBreadcrumb={breadcrumb as BreadcrumbItem[]}
				selectedFolderIds={folderIds}
			/>
		</>
	);
}
