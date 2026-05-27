import { FILE_PAGE_SIZE } from "@/lib/constants";
import { logger } from "@/lib/logger";
import {
	applyWorkspaceRequestState,
	beginWorkspaceRequest,
	fetchFolder,
	finishWorkspaceRequest,
	getInitialPageParams,
	isRequestCanceled,
	resolveBreadcrumb,
} from "./request";
import type { FileStoreSlice, NavigationSlice } from "./types";
import {
	createRootBreadcrumb,
	createSearchReset,
	createSelectionReset,
	createWorkspaceContentReset,
} from "./types";

export const createNavigationSlice: FileStoreSlice<NavigationSlice> = (
	set,
	get,
) => ({
	currentFolderId: null,
	breadcrumb: createRootBreadcrumb(),
	folders: [],
	files: [],
	loading: false,
	error: null,
	filesTotalCount: 0,
	foldersTotalCount: 0,
	loadingMore: false,
	nextFileCursor: null,

	navigateTo: async (folderId, folderName, breadcrumbPath) => {
		const request = beginWorkspaceRequest(set, get);
		set({
			loading: true,
			error: null,
			...createSearchReset(),
			...createSelectionReset(),
			...createWorkspaceContentReset(),
		});

		try {
			const [contents, newBreadcrumb] = await Promise.all([
				fetchFolder(
					folderId,
					getInitialPageParams(get().sortBy, get().sortOrder),
					request.signal,
				),
				resolveBreadcrumb(folderId, breadcrumbPath, request.signal),
			]);

			applyWorkspaceRequestState(set, get, request, {
				currentFolderId: folderId,
				folders: contents.folders,
				files: contents.files,
				foldersTotalCount: contents.folders_total,
				filesTotalCount: contents.files_total,
				nextFileCursor: contents.next_file_cursor ?? null,
				breadcrumb: newBreadcrumb,
				lastFolderContents: {
					folderId,
					folders: contents.folders,
					sortBy: get().sortBy,
					sortOrder: get().sortOrder,
					workspaceRevision: get().workspaceRequestRevision,
				},
				loading: false,
				error: null,
			});
		} catch (error) {
			if (isRequestCanceled(error)) {
				finishWorkspaceRequest(set, get, request);
				return;
			}

			const message =
				error && typeof error === "object" && "message" in error
					? (error as { message: string }).message
					: folderName || "Failed to load folder";

			applyWorkspaceRequestState(set, get, request, {
				loading: false,
				error: message,
			});
			throw error;
		}
	},

	refresh: async () => {
		const { currentFolderId } = get();
		const request = beginWorkspaceRequest(set, get);
		set({
			loading: true,
			...createWorkspaceContentReset(),
		});

		try {
			const contents = await fetchFolder(
				currentFolderId,
				getInitialPageParams(get().sortBy, get().sortOrder),
				request.signal,
			);

			applyWorkspaceRequestState(set, get, request, {
				folders: contents.folders,
				files: contents.files,
				foldersTotalCount: contents.folders_total,
				filesTotalCount: contents.files_total,
				nextFileCursor: contents.next_file_cursor ?? null,
				lastFolderContents: {
					folderId: currentFolderId,
					folders: contents.folders,
					sortBy: get().sortBy,
					sortOrder: get().sortOrder,
					workspaceRevision: get().workspaceRequestRevision,
				},
				loading: false,
			});
		} catch (error) {
			if (isRequestCanceled(error)) {
				finishWorkspaceRequest(set, get, request);
				return;
			}

			applyWorkspaceRequestState(set, get, request, { loading: false });
			throw error;
		}
	},

	loadMoreFiles: async () => {
		const { currentFolderId, nextFileCursor, loadingMore, sortBy, sortOrder } =
			get();
		if (loadingMore || !nextFileCursor) return;

		const request = beginWorkspaceRequest(set, get);
		set({ loadingMore: true });

		try {
			const contents = await fetchFolder(
				currentFolderId,
				{
					folder_limit: 0,
					file_limit: FILE_PAGE_SIZE,
					file_after_value: nextFileCursor.value,
					file_after_id: nextFileCursor.id,
					sort_by: sortBy,
					sort_order: sortOrder,
				},
				request.signal,
			);

			applyWorkspaceRequestState(set, get, request, (state) => ({
				files: [...state.files, ...contents.files],
				nextFileCursor: contents.next_file_cursor ?? null,
				loadingMore: false,
			}));
		} catch (error) {
			if (isRequestCanceled(error)) {
				applyWorkspaceRequestState(set, get, request, {
					loadingMore: false,
				});
				return;
			}

			applyWorkspaceRequestState(set, get, request, {
				loadingMore: false,
			});
			logger.warn("loadMoreFiles failed", error);
		}
	},

	hasMoreFiles: () => get().nextFileCursor !== null,
});
