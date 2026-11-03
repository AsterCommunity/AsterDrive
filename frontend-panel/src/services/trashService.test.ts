import { beforeEach, describe, expect, it, vi } from "vitest";
import { createTrashService, trashService } from "@/services/trashService";

const { apiDelete, apiGet, apiPost } = vi.hoisted(() => ({
	apiDelete: vi.fn(),
	apiGet: vi.fn(),
	apiPost: vi.fn(),
}));

vi.mock("@/services/http", () => ({
	api: {
		delete: apiDelete,
		get: apiGet,
		post: apiPost,
	},
}));

describe("trashService", () => {
	beforeEach(() => {
		apiDelete.mockReset();
		apiGet.mockReset();
		apiPost.mockReset();
	});

	it("uses the expected trash list and restore routes", () => {
		const params = {
			file_limit: 100,
			file_after_expires_at: "2026-04-04T00:00:00Z",
			file_after_id: 9,
		};

		trashService.list(params);
		trashService.restoreFile(12);
		trashService.restoreFolder(34);

		expect(apiGet).toHaveBeenCalledWith("/trash", { params });
		expect(apiPost).toHaveBeenCalledWith("/trash/file/12/restore");
		expect(apiPost).toHaveBeenCalledWith(
			"/trash/folder/34/restore",
			undefined,
			{ accept202: true },
		);
	});

	it("uses the expected purge routes", () => {
		trashService.purgeFile(12);
		trashService.purgeFolder(34);
		trashService.purgeAll();

		expect(apiDelete).toHaveBeenCalledWith("/trash/file/12");
		expect(apiDelete).toHaveBeenCalledWith("/trash/folder/34");
		expect(apiDelete).toHaveBeenCalledWith("/trash");

		const teamTrashService = createTrashService({ kind: "team", teamId: 6 });
		teamTrashService.list();
		teamTrashService.restoreFile(12);
		teamTrashService.restoreFolder(34);
		teamTrashService.purgeFile(12);
		teamTrashService.purgeFolder(34);
		teamTrashService.purgeAll();

		expect(apiGet).toHaveBeenNthCalledWith(1, "/teams/6/trash", {
			params: undefined,
		});
		expect(apiPost).toHaveBeenNthCalledWith(
			1,
			"/teams/6/trash/file/12/restore",
		);
		expect(apiPost).toHaveBeenNthCalledWith(
			2,
			"/teams/6/trash/folder/34/restore",
			undefined,
			{ accept202: true },
		);
		expect(apiDelete).toHaveBeenNthCalledWith(4, "/teams/6/trash/file/12");
		expect(apiDelete).toHaveBeenNthCalledWith(5, "/teams/6/trash/folder/34");
		expect(apiDelete).toHaveBeenNthCalledWith(6, "/teams/6/trash");
	});
});
