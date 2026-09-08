import { STORAGE_KEYS } from "@/config/app";
import { logger } from "@/lib/logger";
import { queuePreferenceSync } from "@/lib/preferenceSync";
import {
	applyWorkspaceRequestState,
	beginWorkspaceRequest,
	fetchFolder,
	getInitialPageParams,
	getStored,
	isRequestCanceled,
	setStored,
} from "./request";
import type {
	FileStoreGet,
	FileStoreSet,
	FileStoreSlice,
	PreferencesSlice,
	SortBy,
	SortOrder,
} from "./types";
import {
	createWorkspaceContentReset,
	normalizeBrowserOpenMode,
	normalizeSortBy,
	normalizeSortOrder,
	normalizeViewMode,
} from "./types";

function reloadSortedFolder(
	set: FileStoreSet,
	get: FileStoreGet,
	sortBy: SortBy,
	sortOrder: SortOrder,
) {
	const currentFolderId = get().currentFolderId;
	const request = beginWorkspaceRequest(set, get, "refresh", currentFolderId);
	if (!request) return;

	void fetchFolder(
		currentFolderId,
		getInitialPageParams(sortBy, sortOrder),
		request.signal,
	)
		.then((contents) => {
			applyWorkspaceRequestState(set, get, request, {
				files: contents.files,
				folders: contents.folders,
				filesTotalCount: contents.files_total,
				foldersTotalCount: contents.folders_total,
				nextFileCursor: contents.next_file_cursor ?? null,
			});
		})
		.catch((error) => {
			if (isRequestCanceled(error)) {
				return;
			}

			applyWorkspaceRequestState(set, get, request, {});
			logger.warn("sort refresh failed", error);
		});
}

export const createPreferencesSlice: FileStoreSlice<PreferencesSlice> = (
	set,
	get,
) => ({
	viewMode: getStored(STORAGE_KEYS.viewMode, "list", normalizeViewMode),
	browserOpenMode: getStored(
		STORAGE_KEYS.browserOpenMode,
		"single_click",
		normalizeBrowserOpenMode,
	),
	sortBy: getStored(STORAGE_KEYS.sortBy, "name", normalizeSortBy),
	sortOrder: getStored(STORAGE_KEYS.sortOrder, "asc", normalizeSortOrder),

	setViewMode: (mode) => {
		const viewMode = normalizeViewMode(mode, get().viewMode);
		if (viewMode !== mode) return;
		setStored(STORAGE_KEYS.viewMode, viewMode);
		set({ viewMode: mode });
		queuePreferenceSync({ view_mode: viewMode });
	},

	setBrowserOpenMode: (mode) => {
		const browserOpenMode = normalizeBrowserOpenMode(
			mode,
			get().browserOpenMode,
		);
		if (browserOpenMode !== mode) return;
		setStored(STORAGE_KEYS.browserOpenMode, browserOpenMode);
		set({ browserOpenMode });
		queuePreferenceSync({ browser_open_mode: browserOpenMode });
	},

	setSortBy: (sortBy) => {
		const nextSortBy = normalizeSortBy(sortBy, get().sortBy);
		if (nextSortBy !== sortBy) return;
		setStored(STORAGE_KEYS.sortBy, nextSortBy);
		queuePreferenceSync({ sort_by: nextSortBy });
		set({
			sortBy: nextSortBy,
			...createWorkspaceContentReset(),
		});
		reloadSortedFolder(set, get, nextSortBy, get().sortOrder);
	},

	setSortOrder: (sortOrder) => {
		const nextSortOrder = normalizeSortOrder(sortOrder, get().sortOrder);
		if (nextSortOrder !== sortOrder) return;
		setStored(STORAGE_KEYS.sortOrder, nextSortOrder);
		queuePreferenceSync({ sort_order: nextSortOrder });
		set({
			sortOrder: nextSortOrder,
			...createWorkspaceContentReset(),
		});
		reloadSortedFolder(set, get, get().sortBy, nextSortOrder);
	},

	_applyFromServer: ({ viewMode, browserOpenMode, sortBy, sortOrder }) => {
		const nextViewMode = normalizeViewMode(viewMode, get().viewMode);
		const nextBrowserOpenMode = normalizeBrowserOpenMode(
			browserOpenMode,
			get().browserOpenMode,
		);
		const nextSortBy = normalizeSortBy(sortBy, get().sortBy);
		const nextSortOrder = normalizeSortOrder(sortOrder, get().sortOrder);

		setStored(STORAGE_KEYS.viewMode, nextViewMode);
		setStored(STORAGE_KEYS.browserOpenMode, nextBrowserOpenMode);
		setStored(STORAGE_KEYS.sortBy, nextSortBy);
		setStored(STORAGE_KEYS.sortOrder, nextSortOrder);
		set({
			viewMode: nextViewMode,
			browserOpenMode: nextBrowserOpenMode,
			sortBy: nextSortBy,
			sortOrder: nextSortOrder,
		});
	},
});
