import { describe, expect, it } from "vitest";
import {
	createPreviewAppDraft,
	getPreviewAppsConfigIssues,
	getPreviewAppsConfigIssuesFromString,
	parsePreviewAppsConfig,
	serializePreviewAppsConfig,
} from "@/components/admin/previewAppsConfigEditorShared";
import { PREVIEW_APP_ICON_URLS } from "@/components/common/previewAppIconUrls";

describe("previewAppsConfigEditorShared", () => {
	it("parses and serializes extension-bound preview app configs", () => {
		const draft = parsePreviewAppsConfig(`{
			"version": 2,
			"apps": [
				{
					"key": "builtin.image",
					"icon": "https://cdn.example.com/image.svg",
					"enabled": true,
					"provider": "builtin",
					"labels": {
						"en": "Image preview",
						"zh": "图片预览"
					}
				},
				{
					"key": "custom.viewer",
					"icon": "https://cdn.example.com/jellyfin.svg",
					"enabled": false,
					"provider": "url_template",
					"labels": {
						"en": "Jellyfin"
					},
					"extensions": ["mp4", "mkv"],
					"config": {
						"mode": "iframe",
						"url_template": "https://videos.example.com/watch?src={{file_preview_url}}",
						"allowed_origins": ["https://videos.example.com"]
					}
				}
			]
		}`);

		expect(draft.version).toBe(2);
		expect(draft.apps).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					key: "builtin.image",
					provider: "builtin",
					labels: {
						en: "Image preview",
						zh: "图片预览",
					},
				}),
				expect.objectContaining({
					config: {
						allowed_origins: ["https://videos.example.com"],
						mode: "iframe",
						url_template:
							"https://videos.example.com/watch?src={{file_preview_url}}",
					},
					enabled: false,
					extensions: ["mp4", "mkv"],
					key: "custom.viewer",
					labels: {
						en: "Jellyfin",
					},
					provider: "url_template",
				}),
				expect.objectContaining({
					extensions: ["zip", "7z"],
					key: "builtin.archive",
					provider: "builtin",
				}),
			]),
		);

		const serialized = JSON.parse(serializePreviewAppsConfig(draft));
		expect(serialized).toMatchObject({
			apps: expect.arrayContaining([
				{
					enabled: true,
					icon: "https://cdn.example.com/image.svg",
					key: "builtin.image",
					labels: {
						en: "Image preview",
						zh: "图片预览",
					},
					provider: "builtin",
				},
				{
					config: {
						allowed_origins: ["https://videos.example.com"],
						mode: "iframe",
						url_template:
							"https://videos.example.com/watch?src={{file_preview_url}}",
					},
					enabled: false,
					extensions: ["mp4", "mkv"],
					icon: "https://cdn.example.com/jellyfin.svg",
					key: "custom.viewer",
					labels: {
						en: "Jellyfin",
					},
					provider: "url_template",
				},
			]),
			version: 2,
		});
		expect(serialized).not.toHaveProperty("rules");
	});

	it("treats default icons as empty overrides", () => {
		const draft = parsePreviewAppsConfig(`{
			"version": 2,
			"apps": [
				{
					"key": "builtin.image",
					"icon": "${PREVIEW_APP_ICON_URLS.image}",
					"enabled": true,
					"provider": "builtin",
					"labels": {
						"zh": "图片预览"
					}
				},
				{
					"key": "custom.viewer",
					"icon": "${PREVIEW_APP_ICON_URLS.web}",
					"enabled": true,
					"provider": "url_template",
					"labels": {
						"zh": "外部查看器"
					},
					"extensions": ["txt"],
					"config": {
						"mode": "iframe",
						"url_template": "https://viewer.example.com/embed?src={{file_preview_url}}"
					}
				}
			]
		}`);

		expect(draft.apps).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					icon: "",
					key: "builtin.image",
				}),
				expect.objectContaining({
					icon: "",
					key: "custom.viewer",
				}),
			]),
		);

		expect(JSON.parse(serializePreviewAppsConfig(draft)).apps).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					icon: "",
					key: "builtin.image",
				}),
				expect.objectContaining({
					icon: "",
					key: "custom.viewer",
				}),
			]),
		);
	});

	it("does not infer provider from the app key when parsing drafts", () => {
		const draft = parsePreviewAppsConfig(`{
			"version": 2,
			"apps": [
				{
					"key": "custom.viewer",
					"icon": "",
					"enabled": true,
					"labels": {
						"zh": "外部查看器"
					},
					"extensions": ["md"],
					"config": {
						"mode": "iframe",
						"url_template": "https://viewer.example.com/embed?src={{file_preview_url}}"
					}
				}
			]
		}`);

		expect(draft.apps).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					extensions: ["md"],
					key: "custom.viewer",
					provider: "",
				}),
			]),
		);
	});

	it("creates useful default app drafts", () => {
		const app = createPreviewAppDraft(["custom.app_1"]);
		expect(app).toMatchObject({
			config: {
				allowed_origins: [],
				mode: "iframe",
				url_template: "",
			},
			enabled: true,
			extensions: [],
			icon: "",
			key: "custom.app_2",
			labels: {},
			provider: "url_template",
		});
	});

	it("reports validation issues for invalid drafts", () => {
		expect(getPreviewAppsConfigIssuesFromString("{bad json")).toEqual([
			{ key: "preview_apps_error_parse" },
		]);

		expect(
			getPreviewAppsConfigIssues({
				apps: [
					{
						config: { mode: "" },
						enabled: true,
						extensions: [],
						icon: "",
						key: "",
						labels: {},
						provider: "",
					},
					{
						config: {},
						enabled: true,
						extensions: [],
						icon: "",
						key: "",
						labels: {},
						provider: "url_template",
					},
					{
						config: {
							mode: "",
						},
						enabled: true,
						extensions: ["docx"],
						icon: "",
						key: "custom.onlyoffice",
						labels: {
							zh: "OnlyOffice",
						},
						provider: "wopi",
					},
				],
				version: 99,
			}).map((issue) => issue.key),
		).toEqual(
			expect.arrayContaining([
				"preview_apps_error_version_mismatch",
				"preview_apps_error_app_key_required",
				"preview_apps_error_app_label_required",
				"preview_apps_error_app_provider_required",
				"preview_apps_error_url_template_mode_required",
				"preview_apps_error_url_template_required",
				"preview_apps_error_wopi_mode_required",
				"preview_apps_error_wopi_target_required",
			]),
		);
	});
});
