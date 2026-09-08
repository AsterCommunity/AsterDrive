import { batchService, singleOperationResult } from "@/services/batchService";
import { fileService } from "@/services/fileService";
import type { BatchResult } from "@/types/api";
import type { ClipboardSlice, FileStoreSlice } from "./types";

export const createClipboardSlice: FileStoreSlice<ClipboardSlice> = (
	set,
	get,
) => ({
	clipboard: null,

	clipboardCopy: () => {
		const { selectedFileIds, selectedFolderIds } = get();
		const count = selectedFileIds.size + selectedFolderIds.size;
		if (count === 0) return 0;

		set({
			clipboard: {
				fileIds: Array.from(selectedFileIds),
				folderIds: Array.from(selectedFolderIds),
				mode: "copy",
			},
		});
		return count;
	},

	clipboardCut: () => {
		const { selectedFileIds, selectedFolderIds } = get();
		const count = selectedFileIds.size + selectedFolderIds.size;
		if (count === 0) return 0;

		set({
			clipboard: {
				fileIds: Array.from(selectedFileIds),
				folderIds: Array.from(selectedFolderIds),
				mode: "cut",
			},
		});
		return count;
	},

	clipboardPaste: async () => {
		const { clipboard, currentFolderId, workspaceRequestRevision } = get();
		if (!clipboard) {
			throw new Error("No clipboard");
		}

		const selectedCount = clipboard.fileIds.length + clipboard.folderIds.length;
		let result: BatchResult;
		if (selectedCount === 1) {
			if (clipboard.mode === "copy") {
				result = await singleOperationResult(
					clipboard.fileIds.length === 1
						? fileService.copyFile(clipboard.fileIds[0], currentFolderId)
						: fileService.copyFolder(clipboard.folderIds[0], currentFolderId),
				);
			} else {
				result = await singleOperationResult(
					clipboard.fileIds.length === 1
						? fileService.moveFile(clipboard.fileIds[0], currentFolderId)
						: fileService.moveFolder(clipboard.folderIds[0], currentFolderId),
				);
			}
		} else if (clipboard.mode === "copy") {
			result = await batchService.batchCopy(
				clipboard.fileIds,
				clipboard.folderIds,
				currentFolderId,
			);
		} else {
			result = await batchService.batchMove(
				clipboard.fileIds,
				clipboard.folderIds,
				currentFolderId,
			);
		}

		const mode = clipboard.mode;
		if (mode === "cut") {
			set({ clipboard: null });
		}

		get().clearSelection();

		if (get().workspaceRequestRevision !== workspaceRequestRevision) {
			return { mode, result };
		}

		await get().refresh(currentFolderId);

		return { mode, result };
	},

	clearClipboard: () => {
		set({ clipboard: null });
	},
});
