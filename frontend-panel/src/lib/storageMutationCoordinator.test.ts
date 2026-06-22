import { beforeEach, describe, expect, it } from "vitest";
import { subscribeStorageChange } from "@/lib/storageChangeBus";
import {
	clearStorageEventEchoes,
	consumeStorageEventEcho,
	type StorageChangeEventPayload,
} from "@/lib/storageEventEcho";
import {
	beginLocalStorageDeleteMutation,
	beginLocalStorageMoveMutation,
	decideFolderTreeStorageRefresh,
	decideRemoteStorageMutation,
	decideUploadQueueSettledRefresh,
	decideVirtualStorageViewRefresh,
	isVirtualFileBrowserPath,
} from "@/lib/storageMutationCoordinator";

function storageEvent(
	overrides: Partial<StorageChangeEventPayload> = {},
): StorageChangeEventPayload {
	return {
		kind: "file.updated",
		workspace: { kind: "personal" },
		file_ids: [7],
		folder_ids: [],
		affected_parent_ids: [3],
		root_affected: false,
		affects_quota: false,
		storage_delta: null,
		at: "2026-06-23T00:00:00Z",
		...overrides,
	};
}

const personalContext = {
	currentWorkspace: { kind: "personal" } as const,
	folder: {
		currentFolderId: 3,
		breadcrumbFolderIds: [1, 3],
	},
	isRefreshGateActive: false,
	pathname: "/",
};

describe("storageMutationCoordinator", () => {
	beforeEach(() => {
		clearStorageEventEchoes();
	});

	it("invalidates matching file resources and refreshes the current folder", () => {
		const decision = decideRemoteStorageMutation(
			storageEvent(),
			personalContext,
		);

		expect(decision).toMatchObject({
			invalidateAllResourceCaches: false,
			invalidateFileResourceCacheIds: [7],
			publishToWorkspace: true,
			refreshCurrentFolder: "now",
			refreshStorageUsage: "none",
		});
	});

	it("keeps quota refresh decisions even when the event is for another workspace", () => {
		const decision = decideRemoteStorageMutation(
			storageEvent({
				workspace: { kind: "team", team_id: 9 },
				affects_quota: true,
			}),
			personalContext,
		);

		expect(decision).toMatchObject({
			publishToWorkspace: false,
			refreshCurrentFolder: "none",
			refreshStorageUsage: "teams",
		});
		expect(decision.invalidateFileResourceCacheIds).toEqual([]);
	});

	it("refreshes personal quota for personal quota-affecting events", () => {
		const decision = decideRemoteStorageMutation(
			storageEvent({ affects_quota: true }),
			personalContext,
		);

		expect(decision.refreshStorageUsage).toBe("personal_quota");
	});

	it("refreshes every storage usage owner for sync.required events", () => {
		const decision = decideRemoteStorageMutation(
			storageEvent({
				kind: "sync.required",
				workspace: null,
				file_ids: [],
				affected_parent_ids: [],
				affects_quota: true,
			}),
			personalContext,
		);

		expect(decision).toMatchObject({
			invalidateAllResourceCaches: true,
			publishToWorkspace: true,
			refreshCurrentFolder: "now",
			refreshStorageUsage: "all",
		});
	});

	it("defers folder refresh while the refresh gate is active", () => {
		const decision = decideRemoteStorageMutation(storageEvent(), {
			...personalContext,
			isRefreshGateActive: true,
		});

		expect(decision.refreshCurrentFolder).toBe("defer");
	});

	it("does not refresh browser folder state on virtual routes", () => {
		const decision = decideRemoteStorageMutation(storageEvent(), {
			...personalContext,
			pathname: "/teams/2/search",
		});

		expect(decision.refreshCurrentFolder).toBe("none");
	});

	it("publishes tag changes without invalidating file content caches", () => {
		const decision = decideRemoteStorageMutation(
			storageEvent({ kind: "tag.assignment_changed" }),
			personalContext,
		);

		expect(decision.publishToWorkspace).toBe(true);
		expect(decision.invalidateFileResourceCacheIds).toEqual([]);
		expect(decision.refreshCurrentFolder).toBe("now");
	});

	it("recognizes virtual file browser paths", () => {
		expect(isVirtualFileBrowserPath("/search")).toBe(true);
		expect(isVirtualFileBrowserPath("/teams/7/category/photo")).toBe(true);
		expect(isVirtualFileBrowserPath("/folder/3")).toBe(false);
	});

	it("falls back to local quota refresh after uploads only when SSE is disabled", () => {
		expect(
			decideUploadQueueSettledRefresh({
				currentFolderId: 3,
				hasDeferredRemoteRefresh: false,
				pendingRefreshFolderIds: [3],
				storageEventStreamEnabled: false,
			}),
		).toEqual({
			refreshCurrentFolder: true,
			refreshPersonalQuota: true,
		});

		expect(
			decideUploadQueueSettledRefresh({
				currentFolderId: 3,
				hasDeferredRemoteRefresh: false,
				pendingRefreshFolderIds: [3],
				storageEventStreamEnabled: true,
			}),
		).toEqual({
			refreshCurrentFolder: true,
			refreshPersonalQuota: false,
		});
	});

	it("flushes deferred remote refresh even when the uploaded folder is not current", () => {
		const decision = decideUploadQueueSettledRefresh({
			currentFolderId: 3,
			hasDeferredRemoteRefresh: true,
			pendingRefreshFolderIds: [9],
			storageEventStreamEnabled: true,
		});

		expect(decision.refreshCurrentFolder).toBe(true);
	});

	it("records local delete mutations for later SSE echo suppression", () => {
		beginLocalStorageDeleteMutation({
			workspace: { kind: "personal" },
			fileIds: [7],
		});

		expect(
			consumeStorageEventEcho(
				storageEvent({ kind: "file.trashed", file_ids: [7] }),
			),
		).toBe(true);
	});

	it("can forget local mutation records after failed local operations", () => {
		const mutation = beginLocalStorageDeleteMutation({
			workspace: { kind: "personal" },
			folderIds: [9],
		});

		mutation.rollback();

		expect(
			consumeStorageEventEcho(
				storageEvent({
					kind: "folder.trashed",
					file_ids: [],
					folder_ids: [9],
				}),
			),
		).toBe(false);
	});

	it("refreshes virtual search views for any referenced resource event", () => {
		expect(
			decideVirtualStorageViewRefresh(
				storageEvent({ kind: "folder.updated", folder_ids: [11] }),
				{ currentWorkspace: { kind: "personal" }, view: "search" },
			),
		).toBe(true);
		expect(
			decideVirtualStorageViewRefresh(
				storageEvent({
					kind: "tag.created",
					file_ids: [],
					affected_parent_ids: [],
				}),
				{ currentWorkspace: { kind: "personal" }, view: "search" },
			),
		).toBe(false);
	});

	it("refreshes category views only for file or file-tag changes", () => {
		expect(
			decideVirtualStorageViewRefresh(
				storageEvent({ kind: "file.updated", file_ids: [7] }),
				{ currentWorkspace: { kind: "personal" }, view: "category" },
			),
		).toBe(true);
		expect(
			decideVirtualStorageViewRefresh(
				storageEvent({ kind: "tag.assignment_changed", file_ids: [7] }),
				{ currentWorkspace: { kind: "personal" }, view: "category" },
			),
		).toBe(true);
		expect(
			decideVirtualStorageViewRefresh(
				storageEvent({ kind: "folder.updated", folder_ids: [9] }),
				{ currentWorkspace: { kind: "personal" }, view: "category" },
			),
		).toBe(false);
	});

	it("returns affected folder tree parents for folder changes", () => {
		const parents = decideFolderTreeStorageRefresh(
			storageEvent({
				kind: "folder.updated",
				folder_ids: [4, 5],
				affected_parent_ids: [9],
			}),
			{
				currentWorkspace: { kind: "personal" },
				expandedFolderIds: [4],
				folderParentIdsById: new Map([
					[4, 2],
					[5, null],
				]),
			},
		);

		expect(parents).toEqual([9, 2, 4, null]);
	});

	it("publishes local move mutations and suppresses matching remote echoes", () => {
		const received: StorageChangeEventPayload[] = [];
		const unsubscribe = subscribeStorageChange((event) => received.push(event));
		const mutation = beginLocalStorageMoveMutation({
			workspace: { kind: "personal" },
			fileIds: [7],
			folderIds: [9],
			targetFolderId: null,
		});

		mutation.publish();
		unsubscribe();

		expect(received).toHaveLength(1);
		expect(received[0]).toMatchObject({
			kind: "folder.updated",
			folder_ids: [9],
			root_affected: true,
		});
		expect(
			consumeStorageEventEcho(
				storageEvent({ kind: "file.updated", file_ids: [7] }),
			),
		).toBe(true);
		expect(
			consumeStorageEventEcho(
				storageEvent({
					kind: "folder.updated",
					file_ids: [],
					folder_ids: [9],
				}),
			),
		).toBe(true);
	});
});
