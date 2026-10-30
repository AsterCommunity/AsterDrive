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

it("requests a workspace recycle bin with an encoded expiry cursor and no folder query", () => {
	adminContentAccessService.trash({
		access_id: "grant",
		file_after_id: 9,
		file_after_expires_at: "2026-11-06T00:00:00+08:00",
	});
	const uri = mocks.get.mock.calls.at(-1)?.[0] as string;
	expect(uri).toContain("/admin/content-access/trash?");
	expect(uri).toContain("%2B08%3A00");
	expect(uri).not.toContain("folder_id");
});

it("builds a thumbnail path scoped to the active inspection without another grant", () => {
	const posts = mocks.post.mock.calls.length;
	const path = adminContentAccessService.thumbnailPath("context +/", 42);
	expect(path).toBe(
		"/admin/content-access/files/42/thumbnail?access_id=context+%2B%2F",
	);
	expect(mocks.post).toHaveBeenCalledTimes(posts);
});

it("loads complete folder details within the current inspection context", async () => {
	mocks.get.mockResolvedValueOnce({
		folder: { id: 19 },
		child_count: { folders: 0, files: 2 },
	});
	const result = await adminContentAccessService.getFolder("context +/", 19);
	expect(mocks.get).toHaveBeenLastCalledWith(
		"/admin/content-access/folders/19?access_id=context+%2B%2F",
	);
	expect(result.child_count).toEqual({ folders: 0, files: 2 });
});
