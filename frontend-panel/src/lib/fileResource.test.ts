import { describe, expect, it } from "vitest";
import {
	authenticatedDownloadResource,
	derivedFileResource,
	previewLinkResource,
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

	it("builds preview-link resources with omit credentials and lifecycle metadata", () => {
		const resource = previewLinkResource(
			"/files/7/download",
			{
				etag: '"etag-7"',
				expires_at: "2026-06-23T12:00:00Z",
				max_uses: 5,
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
			maxUses: 5,
		});
	});
});
