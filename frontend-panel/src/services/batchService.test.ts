import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	batchService,
	createBatchService,
	resolveCopyDispatch,
	resolveMoveDispatch,
} from "@/services/batchService";

const apiPost = vi.hoisted(() => vi.fn());

vi.mock("@/services/http", () => ({
	api: {
		post: apiPost,
	},
}));

describe("batchService", () => {
	beforeEach(() => {
		apiPost.mockReset();
		document.body.innerHTML = "";
	});

	it("posts delete, move, and copy batch payloads", () => {
		batchService.batchDelete([1, 2], [3]);
		batchService.batchMove([1], [2, 3], 9);
		batchService.batchCopy([4, 5], [], null);

		expect(apiPost).toHaveBeenNthCalledWith(1, "/batch/delete", {
			file_ids: [1, 2],
			folder_ids: [3],
		});
		expect(apiPost).toHaveBeenNthCalledWith(2, "/batch/move", {
			file_ids: [1],
			folder_ids: [2, 3],
			target_folder_id: 9,
		});
		expect(apiPost).toHaveBeenNthCalledWith(3, "/batch/copy", {
			file_ids: [4, 5],
			folder_ids: [],
			target_folder_id: null,
		});

		const teamBatchService = createBatchService({ kind: "team", teamId: 4 });
		teamBatchService.batchDelete([1], []);
		teamBatchService.batchMove([], [2], 8);
		teamBatchService.batchCopy([3, 6], [], null);
		teamBatchService.copyToWorkspace({ kind: "personal" }, [7], [8], null);
		teamBatchService.moveToWorkspace({ kind: "personal" }, [9], [10], 11);

		expect(apiPost).toHaveBeenNthCalledWith(4, "/teams/4/batch/delete", {
			file_ids: [1],
			folder_ids: [],
		});
		expect(apiPost).toHaveBeenNthCalledWith(5, "/teams/4/batch/move", {
			file_ids: [],
			folder_ids: [2],
			target_folder_id: 8,
		});
		expect(apiPost).toHaveBeenNthCalledWith(6, "/teams/4/batch/copy", {
			file_ids: [3, 6],
			folder_ids: [],
			target_folder_id: null,
		});
		expect(apiPost).toHaveBeenNthCalledWith(7, "/workspace-transfer/copy", {
			source_workspace: { kind: "team", team_id: 4 },
			file_ids: [7],
			folder_ids: [8],
			destination_workspace: { kind: "personal" },
			target_folder_id: null,
		});
		expect(apiPost).toHaveBeenNthCalledWith(8, "/workspace-transfer/move", {
			source_workspace: { kind: "team", team_id: 4 },
			file_ids: [9],
			folder_ids: [10],
			destination_workspace: { kind: "personal" },
			target_folder_id: 11,
		});
	});

	it("posts workspace transfer copy payloads from personal workspaces", () => {
		batchService.copyToWorkspace({ kind: "team", teamId: 9 }, [1], [2], 6);

		expect(apiPost).toHaveBeenCalledWith("/workspace-transfer/copy", {
			source_workspace: { kind: "personal" },
			file_ids: [1],
			folder_ids: [2],
			destination_workspace: { kind: "team", team_id: 9 },
			target_folder_id: 6,
		});
	});

	it("dispatches copy through the workspace-aware helper", async () => {
		const dispatcher = {
			batchCopy: vi
				.fn()
				.mockResolvedValue({ errors: [], failed: 0, succeeded: 2 }),
			copyToWorkspace: vi
				.fn()
				.mockResolvedValue({ errors: [], failed: 0, succeeded: 2 }),
		};

		await resolveCopyDispatch({
			currentWorkspace: { kind: "personal" },
			targetWorkspace: { kind: "personal" },
			fileIds: [1],
			folderIds: [2],
			targetFolderId: null,
			dispatcher,
		});
		await resolveCopyDispatch({
			currentWorkspace: { kind: "personal" },
			targetWorkspace: { kind: "team", teamId: 9 },
			fileIds: [3],
			folderIds: [4],
			targetFolderId: 5,
			dispatcher,
		});

		expect(dispatcher.batchCopy).toHaveBeenCalledWith([1], [2], null);
		expect(dispatcher.copyToWorkspace).toHaveBeenCalledWith(
			{ kind: "team", teamId: 9 },
			[3],
			[4],
			5,
		);
	});

	it("dispatches single same-workspace copies through single-item handlers", async () => {
		const dispatcher = {
			batchCopy: vi.fn(),
			copyToWorkspace: vi.fn(),
			singleFileCopy: vi
				.fn()
				.mockResolvedValue({ errors: [], failed: 0, succeeded: 1 }),
			singleFolderCopy: vi
				.fn()
				.mockResolvedValue({ errors: [], failed: 0, succeeded: 1 }),
		};

		await resolveCopyDispatch({
			currentWorkspace: { kind: "personal" },
			targetWorkspace: { kind: "personal" },
			fileIds: [1],
			folderIds: [],
			targetFolderId: 9,
			dispatcher,
		});
		await resolveCopyDispatch({
			currentWorkspace: { kind: "personal" },
			targetWorkspace: { kind: "personal" },
			fileIds: [],
			folderIds: [2],
			targetFolderId: 9,
			dispatcher,
		});

		expect(dispatcher.singleFileCopy).toHaveBeenCalledWith(1, 9);
		expect(dispatcher.singleFolderCopy).toHaveBeenCalledWith(2, 9);
		expect(dispatcher.batchCopy).not.toHaveBeenCalled();
	});

	it("dispatches move with the source workspace kept separate from the target", async () => {
		const dispatcher = {
			batchMove: vi
				.fn()
				.mockResolvedValue({ errors: [], failed: 0, succeeded: 1 }),
			moveToWorkspace: vi
				.fn()
				.mockResolvedValue({ errors: [], failed: 0, succeeded: 1 }),
		};

		await resolveMoveDispatch({
			currentWorkspace: { kind: "team", teamId: 4 },
			targetWorkspace: { kind: "personal" },
			fileIds: [7],
			folderIds: [8],
			targetFolderId: 9,
			dispatcher,
		});

		expect(dispatcher.moveToWorkspace).toHaveBeenCalledWith(
			{ kind: "personal" },
			[7],
			[8],
			9,
		);
		expect(dispatcher.batchMove).not.toHaveBeenCalled();
	});

	it("dispatches same-workspace move through the workspace-bound batch endpoint", async () => {
		const dispatcher = {
			batchMove: vi
				.fn()
				.mockResolvedValue({ errors: [], failed: 0, succeeded: 2 }),
			moveToWorkspace: vi.fn(),
		};

		await resolveMoveDispatch({
			currentWorkspace: { kind: "team", teamId: 4 },
			targetWorkspace: { kind: "team", teamId: 4 },
			fileIds: [7, 8],
			folderIds: [],
			targetFolderId: 9,
			dispatcher,
		});

		expect(dispatcher.batchMove).toHaveBeenCalledWith([7, 8], [], 9);
		expect(dispatcher.moveToWorkspace).not.toHaveBeenCalled();
	});

	it("dispatches single same-workspace moves through single-item handlers", async () => {
		const dispatcher = {
			batchMove: vi.fn(),
			singleFileMove: vi
				.fn()
				.mockResolvedValue({ errors: [], failed: 0, succeeded: 1 }),
			singleFolderMove: vi
				.fn()
				.mockResolvedValue({ errors: [], failed: 0, succeeded: 1 }),
			moveToWorkspace: vi.fn(),
		};

		await resolveMoveDispatch({
			currentWorkspace: { kind: "personal" },
			targetWorkspace: { kind: "personal" },
			fileIds: [7],
			folderIds: [],
			targetFolderId: 9,
			dispatcher,
		});
		await resolveMoveDispatch({
			currentWorkspace: { kind: "personal" },
			targetWorkspace: { kind: "personal" },
			fileIds: [],
			folderIds: [8],
			targetFolderId: 9,
			dispatcher,
		});

		expect(dispatcher.singleFileMove).toHaveBeenCalledWith(7, 9);
		expect(dispatcher.singleFolderMove).toHaveBeenCalledWith(8, 9);
		expect(dispatcher.batchMove).not.toHaveBeenCalled();
		expect(dispatcher.moveToWorkspace).not.toHaveBeenCalled();
	});

	it("creates archive download tickets with JSON bodies and triggers iframe downloads", async () => {
		vi.useFakeTimers();
		apiPost
			.mockResolvedValueOnce({
				token: "personal-ticket",
				download_path: "/api/v1/batch/archive-download/personal-ticket",
				expires_at: "2026-04-10T12:00:00Z",
			})
			.mockResolvedValueOnce({
				token: "team-ticket",
				download_path: "/api/v1/teams/4/batch/archive-download/team-ticket",
				expires_at: "2026-04-10T12:00:00Z",
			});

		await batchService.streamArchiveDownload([1], [2], "bundle.zip");
		const teamBatchService = createBatchService({ kind: "team", teamId: 4 });
		await teamBatchService.streamArchiveDownload([], [9]);

		expect(apiPost).toHaveBeenNthCalledWith(1, "/batch/archive-download", {
			file_ids: [1],
			folder_ids: [2],
			archive_name: "bundle.zip",
		});
		expect(apiPost).toHaveBeenNthCalledWith(
			2,
			"/teams/4/batch/archive-download",
			{
				file_ids: [],
				folder_ids: [9],
			},
		);

		const iframes = Array.from(document.querySelectorAll("iframe"));
		expect(iframes).toHaveLength(2);
		expect(iframes[0]).toHaveAttribute(
			"src",
			"/api/v1/batch/archive-download/personal-ticket",
		);
		expect(iframes[1]).toHaveAttribute(
			"src",
			"/api/v1/teams/4/batch/archive-download/team-ticket",
		);

		vi.advanceTimersByTime(60_000);
		expect(document.querySelector("iframe")).toBeNull();
		vi.useRealTimers();
	});

	it("creates archive compress tasks with workspace-scoped payloads", () => {
		batchService.createArchiveCompressTask([1], [2], "bundle.zip", 7);

		const teamBatchService = createBatchService({ kind: "team", teamId: 4 });
		teamBatchService.createArchiveCompressTask([], [9]);

		expect(apiPost).toHaveBeenNthCalledWith(1, "/batch/archive-compress", {
			file_ids: [1],
			folder_ids: [2],
			archive_name: "bundle.zip",
			target_folder_id: 7,
		});
		expect(apiPost).toHaveBeenNthCalledWith(
			2,
			"/teams/4/batch/archive-compress",
			{
				file_ids: [],
				folder_ids: [9],
			},
		);
	});
});
