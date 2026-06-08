import { beforeEach, describe, expect, it, vi } from "vitest";
import { PERSONAL_WORKSPACE } from "@/lib/workspace";
import { useWorkspaceStore } from "@/stores/workspaceStore";

const mockState = vi.hoisted(() => ({
	delete: vi.fn(),
	get: vi.fn(),
	patch: vi.fn(),
	post: vi.fn(),
	put: vi.fn(),
}));

vi.mock("@/services/http", () => ({
	api: {
		delete: mockState.delete,
		get: mockState.get,
		patch: mockState.patch,
		post: mockState.post,
		put: mockState.put,
	},
}));

describe("tagService", () => {
	beforeEach(() => {
		mockState.delete.mockReset();
		mockState.get.mockReset();
		mockState.patch.mockReset();
		mockState.post.mockReset();
		mockState.put.mockReset();
		useWorkspaceStore.getState().setWorkspace(PERSONAL_WORKSPACE);
	});

	it("uses the expected personal tag endpoints and request payloads", async () => {
		const { createTagService } = await import("@/services/tagService");
		const signal = new AbortController().signal;
		const service = createTagService(PERSONAL_WORKSPACE);

		service.listTags({ params: { limit: 20, offset: 10, q: "ops" }, signal });
		service.createTag({ name: "Ops", color: "#3b82f6" });
		service.patchTag(7, { name: "Infra", color: "#16a34a" });
		service.deleteTag(7);
		service.listEntityTags("file", 9, { signal });
		service.replaceEntityTags("folder", 4, [1, 2, 2]);
		service.attachTag(1, "file", 9);
		service.detachTag(1, "folder", 4);
		service.batchAttachTag(2, { file_ids: [9], folder_ids: [4] });
		service.batchDetachTag(2, { file_ids: [9], folder_ids: [4] });

		expect(mockState.get).toHaveBeenNthCalledWith(1, "/tags", {
			params: { limit: 20, offset: 10, q: "ops" },
			signal,
		});
		expect(mockState.post).toHaveBeenCalledWith("/tags", {
			name: "Ops",
			color: "#3b82f6",
		});
		expect(mockState.patch).toHaveBeenCalledWith("/tags/7", {
			name: "Infra",
			color: "#16a34a",
		});
		expect(mockState.delete).toHaveBeenNthCalledWith(1, "/tags/7");
		expect(mockState.get).toHaveBeenNthCalledWith(2, "/tags/file/9", {
			signal,
		});
		expect(mockState.put).toHaveBeenNthCalledWith(1, "/tags/folder/4", {
			tag_ids: [1, 2, 2],
		});
		expect(mockState.put).toHaveBeenNthCalledWith(2, "/tags/1/file/9");
		expect(mockState.delete).toHaveBeenNthCalledWith(2, "/tags/1/folder/4");
		expect(mockState.put).toHaveBeenNthCalledWith(3, "/tags/2/batch", {
			file_ids: [9],
			folder_ids: [4],
		});
		expect(mockState.delete).toHaveBeenNthCalledWith(3, "/tags/2/batch", {
			data: { file_ids: [9], folder_ids: [4] },
		});
	});

	it("prefixes every endpoint for explicit team workspaces", async () => {
		const { createTagService } = await import("@/services/tagService");
		const service = createTagService({ kind: "team", teamId: 12 });

		service.listTags();
		service.createTag({ name: "Team", color: "#0f766e" });
		service.patchTag(7, { color: "#dc2626" });
		service.deleteTag(7);
		service.listEntityTags("folder", 4);
		service.replaceEntityTags("file", 9, [1, 2]);
		service.attachTag(1, "file", 9);
		service.detachTag(1, "folder", 4);
		service.batchAttachTag(2, { file_ids: [9], folder_ids: [4] });
		service.batchDetachTag(2, { file_ids: [9], folder_ids: [4] });

		expect(mockState.get).toHaveBeenNthCalledWith(
			1,
			"/teams/12/tags",
			undefined,
		);
		expect(mockState.post).toHaveBeenCalledWith("/teams/12/tags", {
			name: "Team",
			color: "#0f766e",
		});
		expect(mockState.patch).toHaveBeenCalledWith("/teams/12/tags/7", {
			color: "#dc2626",
		});
		expect(mockState.delete).toHaveBeenNthCalledWith(1, "/teams/12/tags/7");
		expect(mockState.get).toHaveBeenNthCalledWith(
			2,
			"/teams/12/tags/folder/4",
			undefined,
		);
		expect(mockState.put).toHaveBeenNthCalledWith(1, "/teams/12/tags/file/9", {
			tag_ids: [1, 2],
		});
		expect(mockState.put).toHaveBeenNthCalledWith(2, "/teams/12/tags/1/file/9");
		expect(mockState.delete).toHaveBeenNthCalledWith(
			2,
			"/teams/12/tags/1/folder/4",
		);
		expect(mockState.put).toHaveBeenNthCalledWith(3, "/teams/12/tags/2/batch", {
			file_ids: [9],
			folder_ids: [4],
		});
		expect(mockState.delete).toHaveBeenNthCalledWith(
			3,
			"/teams/12/tags/2/batch",
			{ data: { file_ids: [9], folder_ids: [4] } },
		);
	});

	it("resolves the current workspace when a bound method is reused", async () => {
		const { tagService } = await import("@/services/tagService");
		const listTags = tagService.listTags;

		listTags();
		useWorkspaceStore.getState().setWorkspace({ kind: "team", teamId: 15 });
		listTags({ params: { q: "release" } });
		useWorkspaceStore.getState().setWorkspace(PERSONAL_WORKSPACE);
		listTags();

		expect(mockState.get).toHaveBeenNthCalledWith(1, "/tags", undefined);
		expect(mockState.get).toHaveBeenNthCalledWith(2, "/teams/15/tags", {
			params: { q: "release" },
		});
		expect(mockState.get).toHaveBeenNthCalledWith(3, "/tags", undefined);
	});
});
