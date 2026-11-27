import { beforeEach, describe, expect, it, vi } from "vitest";
import { webdavAccountService } from "@/services/webdavAccountService";

const mockState = vi.hoisted(() => ({
	delete: vi.fn(),
	get: vi.fn(),
	post: vi.fn(),
}));

vi.mock("@/services/http", () => ({
	api: {
		delete: mockState.delete,
		get: mockState.get,
		post: mockState.post,
	},
}));

describe("webdavAccountService", () => {
	beforeEach(() => {
		mockState.delete.mockReset();
		mockState.get.mockReset();
		mockState.post.mockReset();
	});

	it("builds list queries and account management endpoints", () => {
		webdavAccountService.list({ limit: 20, offset: 40 });
		webdavAccountService.listForTeam(3, { limit: 10, offset: 30 });
		webdavAccountService.create({
			username: "alice",
			password: "secret",
			root_folder_id: 7,
		});
		webdavAccountService.createForTeam(3, {
			username: "team-alice",
			password: "team-secret",
			root_folder_id: 9,
		});
		webdavAccountService.delete(7);
		webdavAccountService.deleteForTeam(3, 8);
		webdavAccountService.toggle(7);
		webdavAccountService.toggleForTeam(3, 8);
		webdavAccountService.setAccessMode(7, "read_only");
		webdavAccountService.setAccessModeForTeam(3, 8, "read_write");
		webdavAccountService.test({ username: "alice", password: "secret" });

		expect(mockState.get).toHaveBeenCalledWith(
			"/webdav-accounts?limit=20&offset=40",
		);
		expect(mockState.get).toHaveBeenCalledWith(
			"/teams/3/webdav-accounts?limit=10&offset=30",
		);
		expect(mockState.post).toHaveBeenNthCalledWith(1, "/webdav-accounts", {
			username: "alice",
			password: "secret",
			root_folder_id: 7,
		});
		expect(mockState.post).toHaveBeenNthCalledWith(
			2,
			"/teams/3/webdav-accounts",
			{
				username: "team-alice",
				password: "team-secret",
				root_folder_id: 9,
			},
		);
		expect(mockState.delete).toHaveBeenCalledWith("/webdav-accounts/7");
		expect(mockState.delete).toHaveBeenCalledWith("/teams/3/webdav-accounts/8");
		expect(mockState.post).toHaveBeenNthCalledWith(
			3,
			"/webdav-accounts/7/toggle",
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			4,
			"/teams/3/webdav-accounts/8/toggle",
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			5,
			"/webdav-accounts/7/access-mode",
			{ access_mode: "read_only" },
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			6,
			"/teams/3/webdav-accounts/8/access-mode",
			{ access_mode: "read_write" },
		);
		expect(mockState.post).toHaveBeenNthCalledWith(7, "/webdav-accounts/test", {
			username: "alice",
			password: "secret",
		});
	});

	it("uses null as the default root folder id and omits query params when absent", () => {
		webdavAccountService.list();
		webdavAccountService.listForTeam(2);
		webdavAccountService.create({
			username: "bob",
			password: undefined,
			root_folder_id: null,
		});
		webdavAccountService.createForTeam(2, {
			username: "team-bob",
			password: undefined,
			root_folder_id: null,
		});

		expect(mockState.get).toHaveBeenCalledWith("/webdav-accounts");
		expect(mockState.get).toHaveBeenCalledWith("/teams/2/webdav-accounts");
		expect(mockState.post).toHaveBeenCalledWith("/webdav-accounts", {
			username: "bob",
			password: undefined,
			root_folder_id: null,
		});
		expect(mockState.post).toHaveBeenCalledWith("/teams/2/webdav-accounts", {
			username: "team-bob",
			password: undefined,
			root_folder_id: null,
		});
	});
});
