import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	parseVideoBrowserConfig,
	resolveUrlTemplateTarget,
	resolveVideoBrowserTarget,
} from "@/components/files/preview/capabilities/video-browser-config";

describe("video browser config", () => {
	beforeEach(() => {
		window.history.replaceState({}, "", "/files");
	});

	it("parses config with defaults", () => {
		expect(
			parseVideoBrowserConfig({
				VITE_VIDEO_BROWSER_URL_TEMPLATE: "/watch?file={{file_id}}",
			}),
		).toEqual({
			label: "Custom Video Browser",
			mode: "iframe",
			urlTemplate: "/watch?file={{file_id}}",
			allowedOrigins: [],
		});
	});

	it("resolves same-origin templates with encoded values", () => {
		const origin = window.location.origin;
		const config = parseVideoBrowserConfig({
			VITE_VIDEO_BROWSER_LABEL: "Jellyfin",
			VITE_VIDEO_BROWSER_URL_TEMPLATE:
				"/watch?file={{file_id}}&name={{file_name}}&src={{download_url}}",
		});

		const target = resolveVideoBrowserTarget(
			{
				id: 7,
				name: "clip 1.mp4",
				mime_type: "video/mp4",
				size: 2048,
			},
			"/api/v1/files/7/download",
			config,
		);

		expect(target).toEqual({
			label: "Jellyfin",
			mode: "iframe",
			url: `${origin}/watch?file=7&name=clip%201.mp4&src=${encodeURIComponent(`${origin}/api/v1/files/7/download`)}`,
		});
	});

	it("rejects cross-origin targets that are not explicitly allowed", () => {
		const config = parseVideoBrowserConfig({
			VITE_VIDEO_BROWSER_LABEL: "Jellyfin",
			VITE_VIDEO_BROWSER_URL_TEMPLATE:
				"https://videos.example.com/watch?file={{file_id}}",
		});

		expect(
			resolveVideoBrowserTarget(
				{
					id: 7,
					name: "clip.mp4",
					mime_type: "video/mp4",
				},
				"/api/v1/files/7/download",
				config,
			),
		).toBeNull();
	});

	it("allows whitelisted cross-origin targets and supports new-tab mode", () => {
		const config = parseVideoBrowserConfig({
			VITE_VIDEO_BROWSER_LABEL: "Jellyfin",
			VITE_VIDEO_BROWSER_MODE: "new_tab",
			VITE_VIDEO_BROWSER_URL_TEMPLATE:
				"https://videos.example.com/watch?file={{file_id}}",
			VITE_VIDEO_BROWSER_ALLOWED_ORIGINS: "https://videos.example.com",
		});

		expect(
			resolveVideoBrowserTarget(
				{
					id: 7,
					name: "clip.mp4",
					mime_type: "video/mp4",
				},
				"/api/v1/files/7/download",
				config,
			),
		).toEqual({
			label: "Jellyfin",
			mode: "new_tab",
			url: "https://videos.example.com/watch?file=7",
		});
	});

	it("resolves file preview links when the url template needs file_preview_url", async () => {
		const createExternalPreviewLink = vi.fn(async () => ({
			etag: '"etag-report"',
			expires_at: "2026-04-11T12:00:00Z",
			path: "/pv/token/report.docx",
		}));

		const target = await resolveUrlTemplateTarget(
			{
				id: 7,
				name: "report.docx",
				mime_type:
					"application/vnd.openxmlformats-officedocument.wordprocessingml.document",
				size: 4096,
			},
			"/api/v1/files/7/download",
			"Microsoft Viewer",
			{
				allowed_origins: ["https://view.officeapps.live.com"],
				mode: "iframe",
				url_template:
					"https://view.officeapps.live.com/op/embed.aspx?src={{file_preview_url}}",
			},
			createExternalPreviewLink,
		);

		expect(createExternalPreviewLink).toHaveBeenCalledTimes(1);
		expect(target).toEqual({
			label: "Microsoft Viewer",
			mode: "iframe",
			url: `https://view.officeapps.live.com/op/embed.aspx?src=${encodeURIComponent(`${window.location.origin}/pv/token/report.docx`)}`,
		});
	});

	it("does not require a preview link when file_preview_url is not used", async () => {
		const target = await resolveUrlTemplateTarget(
			{
				id: 7,
				name: "clip.mp4",
				mime_type: "video/mp4",
				size: 2048,
			},
			"/api/v1/files/7/download",
			"Jellyfin",
			{
				allowed_origins: ["https://videos.example.com"],
				mode: "iframe",
				url_template:
					"https://videos.example.com/watch?file={{file_id}}&src={{download_url}}",
			},
		);

		expect(target).toEqual({
			label: "Jellyfin",
			mode: "iframe",
			url: `https://videos.example.com/watch?file=7&src=${encodeURIComponent(`${window.location.origin}/api/v1/files/7/download`)}`,
		});
	});
});
