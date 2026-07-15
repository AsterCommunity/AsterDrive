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
		document.body.innerHTML = "";
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
		shareService.batchDelete({ share_ids: [7, 8] });

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
		teamShareService.batchDelete({ share_ids: [7] });

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
		shareService.verifyPassword("token-1", { password: "secret" });
		shareService.createPreviewLink("token-1");
		shareService.getArchivePreview("token-1");
		shareService.getMediaMetadata("token-1");
		shareService.createStreamSession("token-1");
		shareService.getFolderFileArchivePreview("token-1", 42);
		shareService.getFolderFileMediaMetadata("token-1", 42);
		shareService.createFolderFilePreviewLink("token-1", 42);
		shareService.createFolderFileStreamSession("token-1", 42);
		shareService.listContent("token-1", params);
		shareService.listSubfolderContent("token-1", 42, params);
		shareService.getSubfolderAncestors("token-1", 42);

		expect(apiGet).toHaveBeenNthCalledWith(1, "/s/token-1");
		expect(apiGet).toHaveBeenNthCalledWith(
			2,
			"/s/token-1/archive-preview",
			undefined,
		);
		expect(apiGet).toHaveBeenNthCalledWith(
			3,
			"/s/token-1/media-metadata",
			undefined,
		);
		expect(apiGet).toHaveBeenNthCalledWith(
			4,
			"/s/token-1/files/42/archive-preview",
			undefined,
		);
		expect(apiGet).toHaveBeenNthCalledWith(
			5,
			"/s/token-1/files/42/media-metadata",
			undefined,
		);
		expect(apiPost).toHaveBeenCalledWith("/s/token-1/verify", {
			password: "secret",
		});
		expect(apiPost).toHaveBeenCalledWith("/s/token-1/preview-link");
		expect(apiPost).toHaveBeenCalledWith("/s/token-1/stream-session");
		expect(apiPost).toHaveBeenCalledWith("/s/token-1/files/42/preview-link");
		expect(apiPost).toHaveBeenCalledWith("/s/token-1/files/42/stream-session");
		expect(apiGet).toHaveBeenNthCalledWith(6, "/s/token-1/content", {
			params,
		});
		expect(apiGet).toHaveBeenNthCalledWith(7, "/s/token-1/folders/42/content", {
			params,
		});
		expect(apiGet).toHaveBeenNthCalledWith(
			8,
			"/s/token-1/folders/42/ancestors",
		);
		expect(shareService.pagePath("token-1")).toBe("/s/token-1");
		expect(shareService.folderPagePath("token-1", 42)).toBe(
			"/s/token-1/folder/42",
		);
		expect(shareService.pageUrl("token-1")).toBe(
			new URL("/s/token-1", window.location.origin).toString(),
		);
		expect(shareService.downloadPath("token-1")).toBe("/s/token-1/download");
		expect(shareService.thumbnailPath("token-1")).toBe("/s/token-1/thumbnail");
		expect(shareService.folderFileThumbnailPath("token-1", 42)).toBe(
			"/s/token-1/files/42/thumbnail",
		);
		expect(shareService.imagePreviewPath("token-1")).toBe(
			"/s/token-1/image-preview",
		);
		expect(shareService.downloadFolderPath("token-1", 42)).toBe(
			"/s/token-1/files/42/download",
		);
		expect(shareService.folderFileImagePreviewPath("token-1", 42)).toBe(
			"/s/token-1/files/42/image-preview",
		);
		expect(shareService.downloadUrl("token-1")).toBe(
			"/api/v1/s/token-1/download",
		);
		expect(shareService.downloadFolderFileUrl("token-1", 42)).toBe(
			"/api/v1/s/token-1/files/42/download",
		);
	});

	it("triggers iframe downloads for shared archive tickets", async () => {
		vi.useFakeTimers();
		try {
			apiPost.mockResolvedValueOnce({
				token: "shared-ticket",
				download_path: "/s/token-1/archive-download/shared-ticket?download=1",
				expires_at: "2026-04-10T12:00:00Z",
			});

			await shareService.streamArchiveDownload("token-1", [1, 2], [3]);

			expect(apiPost).toHaveBeenCalledWith("/s/token-1/archive-download", {
				file_ids: [1, 2],
				folder_ids: [3],
			});
			const iframe = document.querySelector("iframe");
			expect(iframe).toHaveAttribute(
				"src",
				"/api/v1/s/token-1/archive-download/shared-ticket?download=1",
			);

			vi.advanceTimersByTime(60_000);
			expect(document.querySelector("iframe")).toBeNull();
		} finally {
			vi.useRealTimers();
		}
	});

	it("uses absolute shared archive download paths without API base rewriting", async () => {
		vi.useFakeTimers();
		try {
			apiPost.mockResolvedValueOnce({
				token: "shared-ticket",
				download_path:
					"https://files.example.test/s/token-1/archive-download/shared-ticket?download=1",
				expires_at: "2026-04-10T12:00:00Z",
			});

			await shareService.streamArchiveDownload("token-1", [1], []);

			const iframe = document.querySelector("iframe");
			expect(iframe).toHaveAttribute(
				"src",
				"https://files.example.test/s/token-1/archive-download/shared-ticket?download=1",
			);
		} finally {
			vi.useRealTimers();
		}
	});

	it("forwards abort signals for public preview metadata requests", () => {
		const controller = new AbortController();

		shareService.getArchivePreview("token-1", { signal: controller.signal });
		shareService.getFolderFileArchivePreview("token-1", 42, {
			signal: controller.signal,
		});
		shareService.getMediaMetadata("token-1", { signal: controller.signal });
		shareService.getFolderFileMediaMetadata("token-1", 42, {
			signal: controller.signal,
		});

		expect(apiGet).toHaveBeenNthCalledWith(1, "/s/token-1/archive-preview", {
			signal: controller.signal,
		});
		expect(apiGet).toHaveBeenNthCalledWith(
			2,
			"/s/token-1/files/42/archive-preview",
			{ signal: controller.signal },
		);
		expect(apiGet).toHaveBeenNthCalledWith(3, "/s/token-1/media-metadata", {
			signal: controller.signal,
		});
		expect(apiGet).toHaveBeenNthCalledWith(
			4,
			"/s/token-1/files/42/media-metadata",
			{ signal: controller.signal },
		);
	});

	it("forwards filename encoding for public archive preview requests", () => {
		const controller = new AbortController();

		shareService.getArchivePreview("token-1", { filenameEncoding: "gb18030" });
		shareService.getFolderFileArchivePreview("token-1", 42, {
			filenameEncoding: "cp437",
		});
		shareService.getArchivePreview("token-1", {
			filenameEncoding: "utf8",
			signal: controller.signal,
		});
		shareService.getFolderFileArchivePreview("token-1", 42, {
			filenameEncoding: "utf8",
			signal: controller.signal,
		});

		expect(apiGet).toHaveBeenNthCalledWith(1, "/s/token-1/archive-preview", {
			params: { filename_encoding: "gb18030" },
		});
		expect(apiGet).toHaveBeenNthCalledWith(
			2,
			"/s/token-1/files/42/archive-preview",
			{ params: { filename_encoding: "cp437" } },
		);
		expect(apiGet).toHaveBeenNthCalledWith(3, "/s/token-1/archive-preview", {
			params: { filename_encoding: "utf8" },
			signal: controller.signal,
		});
		expect(apiGet).toHaveBeenNthCalledWith(
			4,
			"/s/token-1/files/42/archive-preview",
			{
				params: { filename_encoding: "utf8" },
				signal: controller.signal,
			},
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
