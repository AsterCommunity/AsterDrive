import { beforeEach, describe, expect, it, vi } from "vitest";
import { teamService } from "@/services/teamService";

const mockState = vi.hoisted(() => ({
	delete: vi.fn(),
	downloadFile: vi.fn(),
	get: vi.fn(),
	patch: vi.fn(),
	post: vi.fn(),
}));

vi.mock("@/services/http", () => ({
	api: {
		delete: mockState.delete,
		get: mockState.get,
		patch: mockState.patch,
		post: mockState.post,
	},
	downloadFile: mockState.downloadFile,
}));

describe("teamService", () => {
	beforeEach(() => {
		mockState.delete.mockReset();
		mockState.downloadFile.mockReset();
		mockState.get.mockReset();
		mockState.patch.mockReset();
		mockState.post.mockReset();
	});

	it("builds team list, audit log, and member list endpoints", () => {
		teamService.list({ archived: true, keyword: "ops", limit: 50, offset: 10 });
		teamService.list();
		teamService.listAuditLogs(7, {
			user_id: 9,
			action: "team_member_add",
			after: "2026-04-01T00:00:00Z",
			before: "2026-04-02T00:00:00Z",
			limit: 20,
			offset: 40,
		});
		teamService.listAuditLogs(7);
		teamService.listMembers(7, {
			keyword: "alice",
			role: "owner" as never,
			status: "active" as never,
			limit: 10,
			offset: 30,
		});
		teamService.listMembers(7);

		expect(mockState.get).toHaveBeenNthCalledWith(
			1,
			"/teams?archived=true&keyword=ops&limit=50&offset=10",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(2, "/teams");
		expect(mockState.get).toHaveBeenNthCalledWith(
			3,
			"/teams/7/audit-logs?limit=20&offset=40&user_id=9&action=team_member_add&after=2026-04-01T00%3A00%3A00Z&before=2026-04-02T00%3A00%3A00Z",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(4, "/teams/7/audit-logs");
		expect(mockState.get).toHaveBeenNthCalledWith(
			5,
			"/teams/7/members?limit=10&offset=30&keyword=alice&role=owner&status=active",
		);
		expect(mockState.get).toHaveBeenNthCalledWith(6, "/teams/7/members");
	});

	it("uses the expected detail and mutation endpoints", () => {
		teamService.get(5);
		teamService.create({
			name: "Infra",
			description: "Infra team",
		});
		teamService.update(5, {
			name: "Platform",
			description: "Platform team",
		});
		teamService.delete(5);
		teamService.restore(5);
		teamService.addMember(5, {
			identifier: "alice@example.com",
			role: "member" as never,
		});
		teamService.updateMember(5, 8, { role: "owner" as never });
		teamService.removeMember(5, 8);

		expect(mockState.get).toHaveBeenCalledWith("/teams/5");
		expect(mockState.post).toHaveBeenNthCalledWith(1, "/teams", {
			name: "Infra",
			description: "Infra team",
		});
		expect(mockState.patch).toHaveBeenNthCalledWith(1, "/teams/5", {
			name: "Platform",
			description: "Platform team",
		});
		expect(mockState.delete).toHaveBeenNthCalledWith(1, "/teams/5");
		expect(mockState.post).toHaveBeenNthCalledWith(2, "/teams/5/restore");
		expect(mockState.post).toHaveBeenNthCalledWith(3, "/teams/5/members", {
			identifier: "alice@example.com",
			role: "member",
		});
		expect(mockState.patch).toHaveBeenNthCalledWith(2, "/teams/5/members/8", {
			role: "owner",
		});
		expect(mockState.delete).toHaveBeenNthCalledWith(2, "/teams/5/members/8");
	});

	it("exports team audit logs without pagination parameters", () => {
		teamService.exportAuditLogs(7, {
			action: "team_update",
			before: "2026-01-02T00:00:00Z",
		});

		expect(mockState.downloadFile).toHaveBeenCalledWith(
			"/teams/7/audit-logs/export?action=team_update&before=2026-01-02T00%3A00%3A00Z",
			undefined,
			"asterdrive-team-7-audit.csv",
		);
	});
});
