import { beginLocalStorageDeleteMutation } from "@/lib/storageMutationCoordinator";
import {
	batchService,
	resolveMoveDispatch,
	singleOperationResult,
} from "@/services/batchService";
import { fileService } from "@/services/fileService";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { CrudSlice, FileStoreSlice } from "./types";

export const createCrudSlice: FileStoreSlice<CrudSlice> = (set, get) => ({
	createFile: async (name) => {
		const { currentFolderId } = get();
		await fileService.createEmptyFile(name, currentFolderId);
		await get().refresh(currentFolderId);
	},

	createFolder: async (name) => {
		const { currentFolderId } = get();
		await fileService.createFolder(name, currentFolderId);
		await get().refresh(currentFolderId);
	},

	deleteFile: async (id) => {
		const { currentFolderId } = get();
		const mutation = beginLocalStorageDeleteMutation({
			workspace: useWorkspaceStore.getState().workspace,
			fileIds: [id],
		});
		try {
			await fileService.deleteFile(id);
		} catch (error) {
			mutation.rollback();
			throw error;
		}
		const next = new Set(get().selectedFileIds);
		next.delete(id);
		set({ selectedFileIds: next });
		await get().refresh(currentFolderId);
	},

	deleteFolder: async (id) => {
		const { currentFolderId } = get();
		const mutation = beginLocalStorageDeleteMutation({
			workspace: useWorkspaceStore.getState().workspace,
			folderIds: [id],
		});
		try {
			await fileService.deleteFolder(id);
		} catch (error) {
			mutation.rollback();
			throw error;
		}
		const next = new Set(get().selectedFolderIds);
		next.delete(id);
		set({ selectedFolderIds: next });
		await get().refresh(currentFolderId);
	},

	moveToFolder: async (fileIds, folderIds, targetFolderId) => {
		const revision = get().workspaceRequestRevision;
		const sourceFolderId = get().currentFolderId;
		const workspace = useWorkspaceStore.getState().workspace;
		const result = await resolveMoveDispatch({
			currentWorkspace: workspace,
			targetWorkspace: workspace,
			fileIds,
			folderIds,
			targetFolderId,
			dispatcher: {
				batchMove: batchService.batchMove,
				singleFileMove: (fileId, folderId) =>
					singleOperationResult(fileService.moveFile(fileId, folderId)),
				singleFolderMove: (folderId, parentId) =>
					singleOperationResult(fileService.moveFolder(folderId, parentId)),
				moveToWorkspace: batchService.moveToWorkspace,
			},
		});
		get().clearSelection();

		if (get().workspaceRequestRevision !== revision) {
			return result;
		}

		await get().refresh(sourceFolderId);

		return result;
	},
});
