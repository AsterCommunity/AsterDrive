import type { TFunction } from "i18next";
import { useCallback, useState } from "react";
import { toast } from "sonner";
import { handleApiError } from "@/hooks/useApiError";
import { ArchiveTaskNameDialog } from "@/pages/file-browser/fileBrowserLazy";
import type { FileBrowserArchiveTaskTarget } from "@/pages/file-browser/types";
import { batchService } from "@/services/batchService";
import { fileService } from "@/services/fileService";
import { requestDownloadSelection } from "@/stores/downloadStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type {
	ArchiveFilenameEncoding,
	FileListItem,
	FolderListItem,
} from "@/types/api";

function buildArchiveTimestamp() {
	const now = new Date();
	const pad = (value: number) => value.toString().padStart(2, "0");
	return (
		[
			now.getUTCFullYear().toString(),
			pad(now.getUTCMonth() + 1),
			pad(now.getUTCDate()),
		].join("") +
		"-" +
		[
			pad(now.getUTCHours()),
			pad(now.getUTCMinutes()),
			pad(now.getUTCSeconds()),
		].join("")
	);
}

function ensureZipSuffix(name: string) {
	return name.toLowerCase().endsWith(".zip") ? name : `${name}.zip`;
}

function stripZipSuffix(name: string) {
	return name.toLowerCase().endsWith(".zip") && name.length > 4
		? name.slice(0, -4)
		: "";
}

function defaultArchiveCompressName(
	fileIds: number[],
	folderIds: number[],
	files: FileListItem[],
	folders: FolderListItem[],
) {
	if (folderIds.length === 1 && fileIds.length === 0) {
		const folder = folders.find((entry) => entry.id === folderIds[0]);
		if (folder) {
			return ensureZipSuffix(folder.name);
		}
	}

	if (fileIds.length === 1 && folderIds.length === 0) {
		const file = files.find((entry) => entry.id === fileIds[0]);
		if (file) {
			return ensureZipSuffix(file.name);
		}
	}

	return `archive-${buildArchiveTimestamp()}.zip`;
}

function defaultArchiveExtractFolderName(sourceFileName: string) {
	const stripped = stripZipSuffix(sourceFileName);
	if (stripped) {
		return stripped;
	}
	return `extracted-${buildArchiveTimestamp()}`;
}

interface UseFileBrowserArchiveActionsOptions {
	clearSelection: () => void;
	displayFiles: FileListItem[];
	displayFolders: FolderListItem[];
	t: TFunction;
}

export function useFileBrowserArchiveActions({
	clearSelection,
	displayFiles,
	displayFolders,
	t,
}: UseFileBrowserArchiveActionsOptions) {
	const [archiveTaskTarget, setArchiveTaskTarget] =
		useState<FileBrowserArchiveTaskTarget | null>(null);

	const notifyTaskQueued = useCallback(
		(displayName: string) => {
			toast.success(t("tasks:task_created_success"), {
				description: displayName,
			});
		},
		[t],
	);

	const closeArchiveTask = useCallback(() => {
		setArchiveTaskTarget(null);
	}, []);

	const startArchiveDownload = useCallback(
		async (fileIds: number[], folderIds: number[]) => {
			if (fileIds.length === 0 && folderIds.length === 0) {
				return;
			}

			const files = displayFiles
				.filter((file) => fileIds.includes(file.id))
				.map((file) => ({ id: file.id, name: file.name, size: file.size }));
			const folders = displayFolders
				.filter((folder) => folderIds.includes(folder.id))
				.map((folder) => ({ id: folder.id, name: folder.name }));
			if (files.length + folders.length !== fileIds.length + folderIds.length) {
				await batchService.streamArchiveDownload(fileIds, folderIds);
				return;
			}
			requestDownloadSelection({
				workspace: useWorkspaceStore.getState().workspace,
				files,
				folders,
			});
		},
		[displayFiles, displayFolders],
	);

	const requestArchiveCompress = useCallback(
		(
			fileIds: number[],
			folderIds: number[],
			options?: { clearSelectionOnSuccess?: boolean },
		) => {
			if (fileIds.length === 0 && folderIds.length === 0) {
				return;
			}

			void ArchiveTaskNameDialog.preload();
			setArchiveTaskTarget({
				mode: "compress",
				fileIds,
				folderIds,
				initialName: defaultArchiveCompressName(
					fileIds,
					folderIds,
					displayFiles,
					displayFolders,
				),
				clearSelectionOnSuccess: options?.clearSelectionOnSuccess ?? false,
			});
		},
		[displayFiles, displayFolders],
	);

	const requestArchiveExtract = useCallback(
		(fileId: number) => {
			const sourceFile = displayFiles.find((entry) => entry.id === fileId);
			void ArchiveTaskNameDialog.preload();
			setArchiveTaskTarget({
				mode: "extract",
				fileId,
				initialName: defaultArchiveExtractFolderName(sourceFile?.name ?? ""),
			});
		},
		[displayFiles],
	);

	const submitArchiveTask = useCallback(
		async (
			name: string | undefined,
			filenameEncoding?: ArchiveFilenameEncoding,
		) => {
			if (!archiveTaskTarget) {
				return;
			}

			if (archiveTaskTarget.mode === "compress") {
				const task = await batchService.createArchiveCompressTask(
					archiveTaskTarget.fileIds,
					archiveTaskTarget.folderIds,
					name,
				);
				notifyTaskQueued(task.display_name);
				if (archiveTaskTarget.clearSelectionOnSuccess) {
					clearSelection();
				}
				return;
			}

			const task = await fileService.createArchiveExtractTask(
				archiveTaskTarget.fileId,
				undefined,
				name,
				filenameEncoding,
			);
			notifyTaskQueued(task.display_name);
		},
		[archiveTaskTarget, clearSelection, notifyTaskQueued],
	);

	const handleArchiveDownload = useCallback(
		(targetFolderId: number) => {
			void startArchiveDownload([], [targetFolderId]).catch(handleApiError);
		},
		[startArchiveDownload],
	);

	const handleBatchArchiveCompress = useCallback(
		async (fileIds: number[], folderIds: number[]) => {
			requestArchiveCompress(fileIds, folderIds, {
				clearSelectionOnSuccess: true,
			});
		},
		[requestArchiveCompress],
	);

	const handleArchiveCompress = useCallback(
		(type: "file" | "folder", id: number) => {
			const fileIds = type === "file" ? [id] : [];
			const folderIds = type === "folder" ? [id] : [];
			requestArchiveCompress(fileIds, folderIds);
		},
		[requestArchiveCompress],
	);

	const handleArchiveExtract = useCallback(
		(fileId: number) => {
			requestArchiveExtract(fileId);
		},
		[requestArchiveExtract],
	);

	return {
		archiveTaskTarget,
		closeArchiveTask,
		handleArchiveCompress,
		handleArchiveDownload,
		handleArchiveExtract,
		handleBatchArchiveCompress,
		startArchiveDownload,
		submitArchiveTask,
	};
}
