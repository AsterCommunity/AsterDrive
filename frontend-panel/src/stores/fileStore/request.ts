import { FILE_PAGE_SIZE, FOLDER_LIMIT } from "@/lib/constants";
import { readLocalStorage, writeLocalStorage } from "@/lib/storage";
import { fileService } from "@/services/fileService";
import { isRequestCanceled } from "@/services/http";
import { searchService } from "@/services/searchService";
import type {
	FolderListParams,
	SearchParams,
	SearchResults,
} from "@/types/api";
import type {
	BreadcrumbItem,
	FileState,
	FileStoreGet,
	FileStoreSet,
	SortBy,
	SortOrder,
} from "./types";
import { createRootBreadcrumb } from "./types";

interface WorkspaceRequestHandle {
	requestId: number;
	revision: number;
	signal: AbortSignal;
}

export function getStored<T extends string>(
	key: string,
	fallback: T,
	normalize: (value: unknown, fallback: T) => T,
): T {
	return normalize(readLocalStorage(key), fallback);
}

export function setStored(key: string, value: string): void {
	writeLocalStorage(key, value);
}

export function getInitialPageParams(
	sortBy: SortBy,
	sortOrder: SortOrder,
): FolderListParams {
	return {
		folder_limit: FOLDER_LIMIT,
		file_limit: FILE_PAGE_SIZE,
		sort_by: sortBy,
		sort_order: sortOrder,
	};
}

export async function fetchFolder(
	folderId: number | null,
	params: FolderListParams | undefined,
	signal: AbortSignal,
) {
	return folderId === null
		? await fileService.listRoot(params, { signal })
		: await fileService.listFolder(folderId, params, { signal });
}

export async function resolveBreadcrumb(
	folderId: number | null,
	breadcrumbPath: BreadcrumbItem[] | undefined,
	signal: AbortSignal,
): Promise<BreadcrumbItem[]> {
	if (folderId === null) {
		return createRootBreadcrumb();
	}

	if (breadcrumbPath && breadcrumbPath.length > 0) {
		return breadcrumbPath;
	}

	const ancestors = await fileService.getFolderAncestors(folderId, { signal });
	return [
		...createRootBreadcrumb(),
		...ancestors.map((item) => ({ id: item.id, name: item.name })),
	];
}

export async function runSearch(
	params: SearchParams,
	signal: AbortSignal,
): Promise<SearchResults> {
	return await searchService.search(params, { signal });
}

export function beginWorkspaceRequest(
	set: FileStoreSet,
	get: FileStoreGet,
): WorkspaceRequestHandle {
	get()._workspaceRequestController?.abort();

	const requestId = get()._workspaceRequestId + 1;
	const controller = new AbortController();
	set({
		_workspaceRequestId: requestId,
		_workspaceRequestController: controller,
	});

	return {
		requestId,
		revision: get().workspaceRequestRevision,
		signal: controller.signal,
	};
}

export function cancelWorkspaceRequest(set: FileStoreSet, get: FileStoreGet) {
	const controller = get()._workspaceRequestController;
	if (!controller) {
		return;
	}

	controller.abort();
	set((state) => ({
		_workspaceRequestController: null,
		_workspaceRequestId: state._workspaceRequestId + 1,
	}));
}

export function isCurrentWorkspaceRequest(
	get: FileStoreGet,
	request: WorkspaceRequestHandle,
): boolean {
	const state = get();
	return (
		state.workspaceRequestRevision === request.revision &&
		state._workspaceRequestId === request.requestId
	);
}

export function finishWorkspaceRequest(
	set: FileStoreSet,
	get: FileStoreGet,
	request: WorkspaceRequestHandle,
) {
	if (!isCurrentWorkspaceRequest(get, request)) {
		return false;
	}

	set({ _workspaceRequestController: null });
	return true;
}

export function applyWorkspaceRequestState(
	set: FileStoreSet,
	get: FileStoreGet,
	request: WorkspaceRequestHandle,
	state: Partial<FileState> | ((current: FileState) => Partial<FileState>),
) {
	if (!isCurrentWorkspaceRequest(get, request)) {
		return false;
	}

	if (typeof state === "function") {
		set((current) => ({
			...state(current),
			_workspaceRequestController: null,
		}));
		return true;
	}

	set({
		...state,
		_workspaceRequestController: null,
	});
	return true;
}

export { isRequestCanceled };
