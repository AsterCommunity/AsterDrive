import { beginLocalStorageDeleteMutation } from "@/lib/storageMutationCoordinator";
import { batchService } from "@/services/batchService";
import { fileService } from "@/services/fileService";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import {
	applyWorkspaceRequestState,
	beginWorkspaceRequest,
	fetchFolder,
	getInitialPageParams,
	isRequestCanceled,
	resolveBreadcrumb,
} from "./request";
import type { CrudSlice, FileStoreSlice } from "./types";

export const createCrudSlice: FileStoreSlice<CrudSlice> = (set, get) => ({
	createFile: async (name) => {
		const { currentFolderId } = get();
		await fileService.createEmptyFile(name, currentFolderId);
		await get().refresh();
	},

	createFolder: async (name) => {
		const { currentFolderId } = get();
		await fileService.createFolder(name, currentFolderId);
		await get().refresh();
	},

	deleteFile: async (id) => {
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
		await get().refresh();
	},

	deleteFolder: async (id) => {
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
		await get().refresh();
	},

	moveToFolder: async (fileIds, folderIds, targetFolderId) => {
		const revision = get().workspaceRequestRevision;
		const result = await batchService.batchMove(
			fileIds,
			folderIds,
			targetFolderId,
		);
		get().clearSelection();

		if (get().workspaceRequestRevision !== revision) {
			return result;
		}

		const { currentFolderId } = get();
		const request = beginWorkspaceRequest(set, get);

		try {
			const [contents, breadcrumb] = await Promise.all([
				fetchFolder(
					currentFolderId,
					getInitialPageParams(get().sortBy, get().sortOrder),
					request.signal,
				),
				resolveBreadcrumb(currentFolderId, undefined, request.signal),
			]);

			applyWorkspaceRequestState(set, get, request, {
				folders: contents.folders,
				files: contents.files,
				foldersTotalCount: contents.folders_total,
				filesTotalCount: contents.files_total,
				nextFileCursor: contents.next_file_cursor ?? null,
				breadcrumb,
				loading: false,
				loadingMore: false,
				error: null,
			});
		} catch (error) {
			if (!isRequestCanceled(error)) {
				applyWorkspaceRequestState(set, get, request, {
					loading: false,
					loadingMore: false,
				});
				throw error;
			}
		}

		return result;
	},
});
