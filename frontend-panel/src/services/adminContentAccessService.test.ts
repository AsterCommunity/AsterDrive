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

it("reads metadata and revision pages using the existing access id without another create request", async () => {
	mocks.get
		.mockResolvedValueOnce({ id: 42, name: "target.txt" })
		.mockResolvedValueOnce([]);
	await adminContentAccessService.getFile("context", 42);
	await adminContentAccessService.listVersions("context", 42, 7);
	expect(mocks.get).toHaveBeenCalledWith(
		"/admin/content-access/files/42?access_id=context",
	);
	expect(mocks.get).toHaveBeenCalledWith(
		"/admin/content-access/files/42/versions?access_id=context&after_sequence=7&limit=100",
	);
});
