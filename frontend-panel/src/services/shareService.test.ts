import { beforeEach, describe, expect, it, vi } from "vitest";
import { PERSONAL_WORKSPACE } from "@/lib/workspace";
import { createShareService, shareService } from "@/services/shareService";
import type { FolderListParams } from "@/types/api";

const { apiDelete, apiGet, apiPatch, apiPost } = vi.hoisted(() => ({
	apiDelete: vi.fn(),
	apiGet: vi.fn(),
	apiPatch: vi.fn(),
	apiPost: vi.fn(),
}));

vi.mock("@/services/http", () => ({
	api: {
		delete: apiDelete,
		get: apiGet,
		patch: apiPatch,
		post: apiPost,
	},
}));

describe("shareService", () => {
	beforeEach(async () => {
		apiDelete.mockReset();
		apiGet.mockReset();
		apiPatch.mockReset();
		apiPost.mockReset();
		const { setPublicSiteUrls } = await import("@/lib/publicSiteUrl");
		setPublicSiteUrls(null);
	});

	it("uses the expected authenticated share routes", () => {
		const createPayload = {
			target: { type: "file" as const, id: 7 },
			password: "secret",
			max_downloads: 3,
		};

		shareService.create(createPayload);
		shareService.listMine({ limit: 20, offset: 40 });
		shareService.update(7, {
			password: "updated-secret",
			expires_at: "2026-03-31T12:00:00Z",
			max_downloads: 9,
		});
		shareService.delete(7);
		shareService.batchDelete([7, 8]);

		expect(apiPost).toHaveBeenCalledWith("/shares", createPayload);
		expect(apiGet).toHaveBeenCalledWith("/shares", {
			params: { limit: 20, offset: 40 },
		});
		expect(apiPatch).toHaveBeenCalledWith("/shares/7", {
			password: "updated-secret",
			expires_at: "2026-03-31T12:00:00Z",
			max_downloads: 9,
		});
		expect(apiDelete).toHaveBeenCalledWith("/shares/7");
		expect(apiPost).toHaveBeenNthCalledWith(2, "/shares/batch-delete", {
			share_ids: [7, 8],
		});

		const teamShareService = createShareService({ kind: "team", teamId: 5 });
		teamShareService.create(createPayload);
		teamShareService.listMine({ limit: 10 });
		teamShareService.update(7, {
			password: "team-secret",
			expires_at: null,
			max_downloads: 2,
		});
		teamShareService.delete(7);
		teamShareService.batchDelete([7]);

		expect(apiPost).toHaveBeenNthCalledWith(
			3,
			"/teams/5/shares",
			createPayload,
		);
		expect(apiGet).toHaveBeenNthCalledWith(2, "/teams/5/shares", {
			params: { limit: 10 },
		});
		expect(apiPatch).toHaveBeenNthCalledWith(2, "/teams/5/shares/7", {
			password: "team-secret",
			expires_at: null,
			max_downloads: 2,
		});
		expect(apiDelete).toHaveBeenNthCalledWith(2, "/teams/5/shares/7");
		expect(apiPost).toHaveBeenNthCalledWith(4, "/teams/5/shares/batch-delete", {
			share_ids: [7],
		});
	});

	it("uses the expected public share routes and download helpers", () => {
		const params: FolderListParams = {
			file_limit: 50,
			sort_by: "updated_at",
		};

		shareService.getInfo("token-1");
		shareService.verifyPassword("token-1", "secret");
		shareService.listContent("token-1", params);
		shareService.listSubfolderContent("token-1", 42, params);

		expect(apiGet).toHaveBeenNthCalledWith(1, "/s/token-1");
		expect(apiPost).toHaveBeenCalledWith("/s/token-1/verify", {
			password: "secret",
		});
		expect(apiGet).toHaveBeenNthCalledWith(2, "/s/token-1/content", {
			params,
		});
		expect(apiGet).toHaveBeenNthCalledWith(3, "/s/token-1/folders/42/content", {
			params,
		});
		expect(shareService.pagePath("token-1")).toBe("/s/token-1");
		expect(shareService.pageUrl("token-1")).toBe(
			new URL("/s/token-1", window.location.origin).toString(),
		);
		expect(shareService.downloadPath("token-1")).toBe("/s/token-1/download");
		expect(shareService.thumbnailPath("token-1")).toBe("/s/token-1/thumbnail");
		expect(shareService.downloadFolderPath("token-1", 42)).toBe(
			"/s/token-1/files/42/download",
		);
		expect(shareService.downloadUrl("token-1")).toBe(
			"/api/v1/s/token-1/download",
		);
		expect(shareService.downloadFolderFileUrl("token-1", 42)).toBe(
			"/api/v1/s/token-1/files/42/download",
		);
	});

	it("normalizes trailing slashes when building public download URLs", async () => {
		vi.resetModules();
		vi.doMock("@/config/app", () => ({
			config: {
				apiBaseUrl: "/api/v1///",
				appName: "AsterDrive",
				appVersion: "test",
			},
		}));

		const { createShareService } = await import("@/services/shareService");
		const service = createShareService(PERSONAL_WORKSPACE);
		expect(service.downloadUrl("token-1")).toBe("/api/v1/s/token-1/download");
		expect(service.downloadFolderFileUrl("token-1", 42)).toBe(
			"/api/v1/s/token-1/files/42/download",
		);

		vi.doUnmock("@/config/app");
	});

	it("requires an explicit workspace when creating a share service directly", () => {
		expect(() =>
			(
				createShareService as unknown as (
					workspace?: unknown,
				) => ReturnType<typeof createShareService>
			)(),
		).toThrow("workspace is required");
	});

	it("uses the configured public site URL for public share pages", async () => {
		vi.resetModules();
		const { setPublicSiteUrls } = await import("@/lib/publicSiteUrl");
		const { createShareService } = await import("@/services/shareService");

		setPublicSiteUrls(["https://drive.example.com"]);

		expect(createShareService(PERSONAL_WORKSPACE).pageUrl("token-1")).toBe(
			"https://drive.example.com/s/token-1",
		);

		setPublicSiteUrls(null);
	});
});
