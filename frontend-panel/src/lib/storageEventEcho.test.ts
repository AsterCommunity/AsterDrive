import { beforeEach, describe, expect, it } from "vitest";
import {
	clearStorageEventEchoes,
	consumeStorageEventEcho,
	forgetStorageEventEchoes,
	rememberStorageDeleteEchoes,
} from "@/lib/storageEventEcho";

describe("storageEventEcho", () => {
	beforeEach(() => {
		clearStorageEventEchoes();
	});

	it("tracks batch delete file and folder events separately", () => {
		rememberStorageDeleteEchoes({
			workspace: { kind: "personal" },
			fileIds: [2, 1],
			folderIds: [5],
		});

		expect(
			consumeStorageEventEcho({
				kind: "file.deleted",
				workspace: { kind: "personal" },
				file_ids: [1, 2],
				folder_ids: [],
				affected_parent_ids: [7],
				root_affected: false,
				at: "2026-05-13T00:00:00Z",
			}),
		).toBe(true);
		expect(
			consumeStorageEventEcho({
				kind: "folder.deleted",
				workspace: { kind: "personal" },
				file_ids: [],
				folder_ids: [5],
				affected_parent_ids: [7],
				root_affected: false,
				at: "2026-05-13T00:00:00Z",
			}),
		).toBe(true);
		expect(
			consumeStorageEventEcho({
				kind: "file.deleted",
				workspace: { kind: "personal" },
				file_ids: [1, 2],
				folder_ids: [],
				affected_parent_ids: [7],
				root_affected: false,
				at: "2026-05-13T00:00:00Z",
			}),
		).toBe(false);
	});

	it("forgets pending echoes when the local mutation fails", () => {
		const echoIds = rememberStorageDeleteEchoes({
			workspace: { kind: "team", teamId: 9 },
			fileIds: [11],
			folderIds: [12],
		});

		forgetStorageEventEchoes(echoIds);

		expect(
			consumeStorageEventEcho({
				kind: "file.deleted",
				workspace: { kind: "team", team_id: 9 },
				file_ids: [11],
				folder_ids: [],
				affected_parent_ids: [7],
				root_affected: false,
				at: "2026-05-13T00:00:00Z",
			}),
		).toBe(false);
		expect(
			consumeStorageEventEcho({
				kind: "folder.deleted",
				workspace: { kind: "team", team_id: 9 },
				file_ids: [],
				folder_ids: [12],
				affected_parent_ids: [7],
				root_affected: false,
				at: "2026-05-13T00:00:00Z",
			}),
		).toBe(false);
	});
});
