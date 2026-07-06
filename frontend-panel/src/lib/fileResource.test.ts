import { describe, expect, it, vi } from "vitest";
import {
	authenticatedDownloadResource,
	cachePreviewLinkResource,
	clearPreviewLinkResourceCache,
	derivedFileResource,
	previewLinkResource,
	readCachedPreviewLinkResource,
} from "@/lib/fileResource";

describe("fileResource", () => {
	it("builds authenticated inline download resources with stable cache identity", () => {
		const resource = authenticatedDownloadResource("/files/7/download", {
			deliveryMode: "blob_url",
			mimeType: "application/pdf",
		});

		expect(resource.identity.cacheKey).toBe("/files/7/download");
		expect(resource.request.url).toBe("/files/7/download?disposition=inline");
		expect(resource.request.credentials).toBe("include");
		expect(resource.request.conditionalHeaders).toBe("forbidden");
		expect(resource.request.redirectPolicy).toBe("may_cross_origin");
		expect(resource.delivery.mode).toBe("blob_url");
		expect(resource.delivery.mimeType).toBe("application/pdf");
	});

	it("preserves existing query and hash when appending inline disposition", () => {
		const resource = authenticatedDownloadResource(
			"/files/7/download?version=2#page-1",
			{
				deliveryMode: "direct_url",
			},
		);

		expect(resource.identity.cacheKey).toBe(
			"/files/7/download?version=2#page-1",
		);
		expect(resource.request.url).toBe(
			"/files/7/download?version=2&disposition=inline#page-1",
		);
	});

	it("builds same-origin derived resources with conditional headers enabled", () => {
		const resource = derivedFileResource("/files/7/image-preview", {
			deliveryMode: "blob_url",
			mimeType: "image/webp",
		});

		expect(resource.identity.cacheKey).toBe("/files/7/image-preview");
		expect(resource.request.url).toBe("/files/7/image-preview");
		expect(resource.request.credentials).toBe("include");
		expect(resource.request.conditionalHeaders).toBe("allowed");
		expect(resource.request.redirectPolicy).toBe("same_origin_only");
	});

	it("builds preview-link resources with omit credentials and expiry lifecycle metadata", () => {
		const resource = previewLinkResource(
			"/files/7/download",
			{
				etag: '"etag-7"',
				expires_at: "2026-06-23T12:00:00Z",
				path: "/pv/token/report.pdf",
			},
			{
				deliveryMode: "blob_url",
				mimeType: "application/pdf",
			},
		);

		expect(resource.identity.cacheKey).toBe("/files/7/download");
		expect(resource.identity.etag).toBe('"etag-7"');
		expect(resource.request.url).toBe("/pv/token/report.pdf");
		expect(resource.request.credentials).toBe("omit");
		expect(resource.request.conditionalHeaders).toBe("forbidden");
		expect(resource.request.redirectPolicy).toBe("may_cross_origin");
		expect(resource.lifecycle).toEqual({
			expiresAt: "2026-06-23T12:00:00Z",
		});
		expect(resource.lifecycle).not.toHaveProperty("maxUses");
	});

	it("evicts expired preview-link cache entries", () => {
		vi.useFakeTimers();
		try {
			vi.setSystemTime(new Date("2026-06-23T12:00:00Z"));
			cachePreviewLinkResource("/files/7/download", {
				etag: '"etag-7"',
				expires_at: "2026-06-23T12:00:20Z",
				path: "/pv/token/report.pdf",
			});

			expect(
				readCachedPreviewLinkResource("/files/7/download", {
					deliveryMode: "blob_url",
					etag: '"etag-7"',
				})?.request.url,
			).toBe("/pv/token/report.pdf");

			vi.setSystemTime(new Date("2026-06-23T12:00:11Z"));
			expect(
				readCachedPreviewLinkResource("/files/7/download", {
					deliveryMode: "blob_url",
					etag: '"etag-7"',
				}),
			).toBeNull();

			vi.setSystemTime(new Date("2026-06-23T12:00:00Z"));
			expect(
				readCachedPreviewLinkResource("/files/7/download", {
					deliveryMode: "blob_url",
					etag: '"etag-7"',
				}),
			).toBeNull();
		} finally {
			clearPreviewLinkResourceCache();
			vi.useRealTimers();
		}
	});

	it("reads preview-link cache entries by precise etag", () => {
		vi.useFakeTimers();
		try {
			vi.setSystemTime(new Date("2026-06-23T12:00:00Z"));
			cachePreviewLinkResource("/files/7/download", {
				etag: '"etag-7-a"',
				expires_at: "2026-06-23T12:01:00Z",
				path: "/pv/token-a/report.pdf",
			});
			cachePreviewLinkResource("/files/7/download", {
				etag: '"etag-7-b"',
				expires_at: "2026-06-23T12:01:00Z",
				path: "/pv/token-b/report.pdf",
			});

			const resource = readCachedPreviewLinkResource("/files/7/download", {
				deliveryMode: "blob_url",
				etag: '"etag-7-b"',
			});

			expect(resource?.identity.etag).toBe('"etag-7-b"');
			expect(resource?.request.url).toBe("/pv/token-b/report.pdf");
		} finally {
			clearPreviewLinkResourceCache();
			vi.useRealTimers();
		}
	});

	it("treats preview-link cache entries at the skew boundary as unusable", () => {
		vi.useFakeTimers();
		try {
			vi.setSystemTime(new Date("2026-06-23T12:00:00Z"));
			cachePreviewLinkResource("/files/7/download", {
				etag: '"etag-boundary"',
				expires_at: "2026-06-23T12:00:10Z",
				path: "/pv/boundary/report.pdf",
			});

			expect(
				readCachedPreviewLinkResource("/files/7/download", {
					deliveryMode: "blob_url",
					etag: '"etag-boundary"',
				}),
			).toBeNull();

			cachePreviewLinkResource("/files/7/download", {
				etag: '"etag-usable"',
				expires_at: "2026-06-23T12:00:11Z",
				path: "/pv/usable/report.pdf",
			});
			expect(
				readCachedPreviewLinkResource("/files/7/download", {
					deliveryMode: "blob_url",
					etag: '"etag-usable"',
				})?.request.url,
			).toBe("/pv/usable/report.pdf");

			vi.setSystemTime(new Date("2026-06-23T12:00:01Z"));
			expect(
				readCachedPreviewLinkResource("/files/7/download", {
					deliveryMode: "blob_url",
					etag: '"etag-usable"',
				}),
			).toBeNull();
		} finally {
			clearPreviewLinkResourceCache();
			vi.useRealTimers();
		}
	});
});
