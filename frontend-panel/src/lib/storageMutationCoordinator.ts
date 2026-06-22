import { publishStorageChange } from "@/lib/storageChangeBus";
import type { StorageChangeEventPayload } from "@/lib/storageEventEcho";
import {
	forgetStorageEventEchoes,
	rememberStorageDeleteEchoes,
	rememberStorageEventEcho,
} from "@/lib/storageEventEcho";
import type { Workspace } from "@/lib/workspace";

export type StorageUsageRefreshDecision =
	| "none"
	| "personal_quota"
	| "teams"
	| "all";

export interface StorageMutationFolderSnapshot {
	breadcrumbFolderIds: number[];
	currentFolderId: number | null;
}

export interface RemoteStorageMutationContext {
	currentWorkspace: Workspace;
	folder: StorageMutationFolderSnapshot;
	isRefreshGateActive: boolean;
	pathname?: string;
}

export interface RemoteStorageMutationDecision {
	invalidateAllResourceCaches: boolean;
	invalidateFileResourceCacheIds: number[];
	publishToWorkspace: boolean;
	refreshCurrentFolder: "none" | "now" | "defer";
	refreshStorageUsage: StorageUsageRefreshDecision;
}

export interface UploadQueueSettledContext {
	currentFolderId: number | null;
	hasDeferredRemoteRefresh: boolean;
	pendingRefreshFolderIds: Iterable<number | null>;
	storageEventStreamEnabled: boolean;
}

export interface UploadQueueSettledDecision {
	refreshCurrentFolder: boolean;
	refreshPersonalQuota: boolean;
}

export interface LocalStorageDeleteMutationInput {
	fileIds?: number[];
	folderIds?: number[];
	workspace: Workspace;
}

export interface LocalStorageMutationRecord {
	rollback: () => void;
}

export interface LocalStorageMoveMutationInput {
	fileIds?: number[];
	folderIds?: number[];
	targetFolderId: number | null;
	workspace: Workspace;
}

export interface LocalStorageMoveMutationRecord
	extends LocalStorageMutationRecord {
	publish: () => void;
}

export interface VirtualStorageViewContext {
	currentWorkspace: Workspace;
	view: "category" | "search";
}

export interface FolderTreeStorageRefreshContext {
	currentWorkspace: Workspace;
	expandedFolderIds: Iterable<number>;
	folderParentIdsById: ReadonlyMap<number, number | null>;
}

function eventMatchesWorkspace(
	eventWorkspace: StorageChangeEventPayload["workspace"],
	workspace: Workspace,
) {
	if (!eventWorkspace) {
		return true;
	}
	if (workspace.kind === "personal") {
		return eventWorkspace.kind === "personal";
	}
	return (
		eventWorkspace.kind === "team" &&
		eventWorkspace.team_id === workspace.teamId
	);
}

function isTagChangeEvent(event: StorageChangeEventPayload) {
	return event.kind.startsWith("tag.");
}

function isFolderChangeEvent(event: StorageChangeEventPayload) {
	return event.kind.startsWith("folder.");
}

function isFileChangeEvent(event: StorageChangeEventPayload) {
	return event.kind.startsWith("file.") || event.kind.startsWith("lock.");
}

function hasResourceReference(event: StorageChangeEventPayload) {
	return (
		event.root_affected ||
		event.file_ids.length > 0 ||
		event.folder_ids.length > 0 ||
		event.affected_parent_ids.length > 0
	);
}

function toStorageChangeWorkspace(workspace: Workspace) {
	return workspace.kind === "personal"
		? ({ kind: "personal" } as const)
		: ({ kind: "team", team_id: workspace.teamId } as const);
}

export function isVirtualFileBrowserPath(pathname = "") {
	return (
		/(?:^|\/)search$/.test(pathname) ||
		/(?:^|\/)teams\/\d+\/search$/.test(pathname) ||
		/(?:^|\/)category\/[^/]+$/.test(pathname) ||
		/(?:^|\/)teams\/\d+\/category\/[^/]+$/.test(pathname)
	);
}

function shouldRefreshCurrentFolder(
	event: StorageChangeEventPayload,
	folder: StorageMutationFolderSnapshot,
	pathname?: string,
) {
	if (isVirtualFileBrowserPath(pathname)) {
		return false;
	}
	if (event.root_affected && folder.currentFolderId === null) {
		return true;
	}
	if (
		folder.currentFolderId !== null &&
		(event.affected_parent_ids.includes(folder.currentFolderId) ||
			event.folder_ids.includes(folder.currentFolderId))
	) {
		return true;
	}
	return folder.breadcrumbFolderIds.some((id) => event.folder_ids.includes(id));
}

function storageUsageRefreshDecision(
	event: StorageChangeEventPayload,
): StorageUsageRefreshDecision {
	if (!event.affects_quota) {
		return "none";
	}
	if (event.kind === "sync.required" || !event.workspace) {
		return "all";
	}
	return event.workspace.kind === "personal" ? "personal_quota" : "teams";
}

function refreshCurrentFolderDecision(
	shouldRefresh: boolean,
	isRefreshGateActive: boolean,
) {
	if (!shouldRefresh) return "none";
	return isRefreshGateActive ? "defer" : "now";
}

export function decideRemoteStorageMutation(
	event: StorageChangeEventPayload,
	context: RemoteStorageMutationContext,
): RemoteStorageMutationDecision {
	const refreshStorageUsage = storageUsageRefreshDecision(event);
	const matchesWorkspace = eventMatchesWorkspace(
		event.workspace,
		context.currentWorkspace,
	);

	if (!matchesWorkspace) {
		return {
			invalidateAllResourceCaches: false,
			invalidateFileResourceCacheIds: [],
			publishToWorkspace: false,
			refreshCurrentFolder: "none",
			refreshStorageUsage,
		};
	}

	if (event.kind === "sync.required") {
		return {
			invalidateAllResourceCaches: true,
			invalidateFileResourceCacheIds: [],
			publishToWorkspace: true,
			refreshCurrentFolder: refreshCurrentFolderDecision(
				!isVirtualFileBrowserPath(context.pathname),
				context.isRefreshGateActive,
			),
			refreshStorageUsage,
		};
	}

	return {
		invalidateAllResourceCaches: false,
		invalidateFileResourceCacheIds: isTagChangeEvent(event)
			? []
			: event.file_ids,
		publishToWorkspace: true,
		refreshCurrentFolder: refreshCurrentFolderDecision(
			shouldRefreshCurrentFolder(event, context.folder, context.pathname),
			context.isRefreshGateActive,
		),
		refreshStorageUsage,
	};
}

export function decideUploadQueueSettledRefresh({
	currentFolderId,
	hasDeferredRemoteRefresh,
	pendingRefreshFolderIds,
	storageEventStreamEnabled,
}: UploadQueueSettledContext): UploadQueueSettledDecision {
	const pendingFolderIds = new Set(pendingRefreshFolderIds);
	return {
		refreshCurrentFolder:
			hasDeferredRemoteRefresh || pendingFolderIds.has(currentFolderId),
		refreshPersonalQuota:
			pendingFolderIds.size > 0 && !storageEventStreamEnabled,
	};
}

export function decideVirtualStorageViewRefresh(
	event: StorageChangeEventPayload,
	context: VirtualStorageViewContext,
) {
	if (!eventMatchesWorkspace(event.workspace, context.currentWorkspace)) {
		return false;
	}
	if (event.kind === "sync.required") {
		return true;
	}
	if (context.view === "category") {
		if (isFileChangeEvent(event)) {
			return event.root_affected || event.file_ids.length > 0;
		}
		if (isTagChangeEvent(event)) {
			return event.root_affected || event.file_ids.length > 0;
		}
		return false;
	}
	if (isTagChangeEvent(event)) {
		return hasResourceReference(event);
	}
	return hasResourceReference(event);
}

export function decideFolderTreeStorageRefresh(
	event: StorageChangeEventPayload,
	context: FolderTreeStorageRefreshContext,
) {
	if (!eventMatchesWorkspace(event.workspace, context.currentWorkspace)) {
		return [];
	}

	const parentsToRefresh = new Set<number | null>();
	const expandedFolderIds = Array.from(context.expandedFolderIds);
	if (event.kind === "sync.required") {
		return [null, ...expandedFolderIds];
	}
	if (!isFolderChangeEvent(event)) {
		return [];
	}

	if (event.root_affected) {
		parentsToRefresh.add(null);
	}
	for (const parentId of event.affected_parent_ids) {
		parentsToRefresh.add(parentId);
	}
	for (const folderId of event.folder_ids) {
		if (context.folderParentIdsById.has(folderId)) {
			parentsToRefresh.add(context.folderParentIdsById.get(folderId) ?? null);
		}
		if (expandedFolderIds.includes(folderId)) {
			parentsToRefresh.add(folderId);
		}
	}

	return Array.from(parentsToRefresh);
}

export function beginLocalStorageDeleteMutation(
	input: LocalStorageDeleteMutationInput,
): LocalStorageMutationRecord {
	const echoIds = rememberStorageDeleteEchoes(input);
	return {
		rollback: () => forgetStorageEventEchoes(echoIds),
	};
}

export function beginLocalStorageMoveMutation(
	input: LocalStorageMoveMutationInput,
): LocalStorageMoveMutationRecord {
	const fileIds = input.fileIds ?? [];
	const folderIds = input.folderIds ?? [];
	const echoIds: number[] = [];
	if (fileIds.length > 0) {
		echoIds.push(
			rememberStorageEventEcho({
				kind: "file.updated",
				workspace: input.workspace,
				fileIds,
			}),
		);
	}
	if (folderIds.length > 0) {
		echoIds.push(
			rememberStorageEventEcho({
				kind: "folder.updated",
				workspace: input.workspace,
				folderIds,
			}),
		);
	}

	return {
		publish: () => {
			if (folderIds.length === 0) return;
			publishStorageChange({
				affected_parent_ids:
					input.targetFolderId === null ? [] : [input.targetFolderId],
				affects_quota: false,
				at: new Date().toISOString(),
				file_ids: [],
				folder_ids: folderIds,
				kind: "folder.updated",
				root_affected: input.targetFolderId === null,
				storage_delta: null,
				workspace: toStorageChangeWorkspace(input.workspace),
			});
		},
		rollback: () => forgetStorageEventEchoes(echoIds),
	};
}
