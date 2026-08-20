import { beforeEach, describe, expect, it, vi } from "vitest";
import { auditService } from "@/services/auditService";

const apiGet = vi.hoisted(() => vi.fn());
const downloadFile = vi.hoisted(() => vi.fn());

vi.mock("@/services/http", () => ({
	api: {
		get: apiGet,
	},
	downloadFile,
}));

describe("auditService", () => {
	beforeEach(() => {
		apiGet.mockReset();
		downloadFile.mockReset();
	});

	it("builds filtered audit log queries and omits empty values", () => {
		auditService.list({
			user_id: 7,
			action: "user_login",
			entity_type: "",
			entity_id: 9,
			after: "2026-01-01T00:00:00Z",
			before: null as never,
			limit: 20,
			offset: 40,
			sort_by: "action",
			sort_order: "asc",
		});

		expect(apiGet).toHaveBeenCalledWith(
			"/admin/audit-logs?limit=20&offset=40&sort_by=action&sort_order=asc&user_id=7&action=user_login&entity_id=9&after=2026-01-01T00%3A00%3A00Z",
		);
	});

	it("uses the base endpoint when no filters are provided", () => {
		auditService.list();

		expect(apiGet).toHaveBeenCalledWith("/admin/audit-logs");
	});

	it("exports the complete filtered result without pagination parameters", () => {
		auditService.export({
			action: "team_update",
			entity_type: "team" as never,
			sort_by: "created_at" as never,
			sort_order: "desc" as never,
		});

		expect(downloadFile).toHaveBeenCalledWith(
			"/admin/audit-logs/export?action=team_update&entity_type=team&sort_by=created_at&sort_order=desc",
			undefined,
			"asterdrive-audit.csv",
		);
	});
});
