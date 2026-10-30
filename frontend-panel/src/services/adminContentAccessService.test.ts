import { describe, expect, it, vi } from "vitest";
import { adminContentAccessService } from "./adminContentAccessService";

const mocks = vi.hoisted(() => ({ get: vi.fn(), post: vi.fn() }));
vi.mock("./http", () => ({ api: mocks }));
describe("administrator content API", () => {
	it("keeps reason and remark in the POST body and scope authorization separate from navigation", () => {
		adminContentAccessService.create({
			scope_kind: "team",
			target_id: 3,
			reason: "user_support",
			remark: "private case",
		});
		expect(mocks.post).toHaveBeenCalledWith("/admin/content-access", {
			scope_kind: "team",
			target_id: 3,
			reason: "user_support",
			remark: "private case",
		});
		adminContentAccessService.list({
			access_id: "context",
			folder_id: 8,
			deleted: true,
			file_after_id: 9,
			file_after_value: "a b",
		});
		const url = mocks.get.mock.calls[0][0] as string;
		expect(url).toContain("access_id=context");
		expect(url).toContain("file_after_value=a+b");
		expect(url).not.toContain("private");
		expect(url).not.toContain("target_id");
	});
});
