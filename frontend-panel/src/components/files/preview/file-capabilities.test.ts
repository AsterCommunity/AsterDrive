import { describe, expect, it } from "vitest";
import {
	detectFilePreviewProfile,
	getAvailableOpenWithOptions,
	getDefaultOpenWith,
	getEditorLanguage,
	getFileExtension,
	getFileTypeInfo,
	isEditableTextFile,
} from "@/components/files/preview/file-capabilities";

describe("file preview capabilities", () => {
	it("detects file extensions and editor languages", () => {
		expect(
			getFileExtension({ name: "README.MD", mime_type: "text/markdown" }),
		).toBe("md");
		expect(
			getEditorLanguage({ name: "Dockerfile", mime_type: "text/plain" }),
		).toBe("dockerfile");
		expect(getEditorLanguage({ name: ".env", mime_type: "text/plain" })).toBe(
			"plaintext",
		);
		expect(
			getEditorLanguage({ name: "script.tsx", mime_type: "text/typescript" }),
		).toBe("typescript");
		// Newly added extensions
		expect(
			getEditorLanguage({ name: "App.vue", mime_type: "text/plain" }),
		).toBe("html");
		expect(
			getEditorLanguage({ name: "main.dart", mime_type: "text/plain" }),
		).toBe("dart");
		expect(
			getEditorLanguage({ name: "buf.proto", mime_type: "text/plain" }),
		).toBe("protobuf");
		expect(
			getEditorLanguage({ name: "schema.graphql", mime_type: "text/plain" }),
		).toBe("graphql");
		expect(
			getEditorLanguage({ name: "main.tf", mime_type: "text/plain" }),
		).toBe("hcl");
		expect(
			getEditorLanguage({ name: "config.toml", mime_type: "text/plain" }),
		).toBe("toml");
		expect(
			getEditorLanguage({ name: "build.sbt", mime_type: "text/plain" }),
		).toBe("scala");
		expect(getEditorLanguage({ name: "app.ex", mime_type: "text/plain" })).toBe(
			"elixir",
		);
		expect(
			getEditorLanguage({ name: "job.groovy", mime_type: "text/plain" }),
		).toBe("groovy");
		expect(
			getEditorLanguage({ name: "build.gradle", mime_type: "text/plain" }),
		).toBe("java");
		expect(
			getEditorLanguage({ name: "intro.tex", mime_type: "text/plain" }),
		).toBe("plaintext");
		expect(
			getEditorLanguage({ name: "deploy.ps1", mime_type: "text/plain" }),
		).toBe("powershell");
		// Special filenames
		expect(
			getEditorLanguage({ name: ".dockerignore", mime_type: "text/plain" }),
		).toBe("plaintext");
		expect(
			getEditorLanguage({ name: "Jenkinsfile", mime_type: "text/plain" }),
		).toBe("groovy");
		expect(
			getEditorLanguage({ name: "Gemfile", mime_type: "text/plain" }),
		).toBe("ruby");
	});

	it("maps mime types and extensions to file categories", () => {
		expect(
			getFileTypeInfo({ name: "manual.pdf", mime_type: "application/pdf" }),
		).toMatchObject({
			category: "pdf",
			icon: "FileText",
		});
		expect(
			getFileTypeInfo({
				name: "table.csv",
				mime_type: "application/octet-stream",
			}),
		).toMatchObject({
			category: "csv",
			icon: "Table",
		});
		expect(
			getFileTypeInfo({ name: "photo.svg", mime_type: "image/svg+xml" }),
		).toMatchObject({
			category: "image",
			icon: "FileImage",
		});
		expect(
			getFileTypeInfo({
				name: "photo.jpg",
				mime_type: "application/octet-stream",
			}),
		).toMatchObject({
			category: "image",
			icon: "FileImage",
		});
		expect(
			getFileTypeInfo({ name: "notes.txt", mime_type: "text/xml" }),
		).toMatchObject({
			category: "xml",
			icon: "BracketsCurly",
		});
		expect(
			getFileTypeInfo({
				name: "deck.pptx",
				mime_type:
					"application/vnd.openxmlformats-officedocument.presentationml.presentation",
			}),
		).toMatchObject({
			category: "presentation",
			icon: "Presentation",
		});
		expect(
			getFileTypeInfo({
				name: "sheet.xlsx",
				mime_type:
					"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
			}),
		).toMatchObject({
			category: "spreadsheet",
			icon: "Table",
		});
		expect(
			getFileTypeInfo({
				name: "report.docx",
				mime_type:
					"application/vnd.openxmlformats-officedocument.wordprocessingml.document",
			}),
		).toMatchObject({
			category: "document",
			icon: "FileText",
		});
		expect(
			getFileTypeInfo({
				name: "report.docx",
				mime_type: "application/octet-stream",
			}),
		).toMatchObject({
			category: "document",
		});
		expect(
			getFileTypeInfo({
				name: "archive.bin",
				mime_type: "application/octet-stream",
			}),
		).toMatchObject({
			category: "unknown",
			icon: "File",
		});
		expect(
			getFileTypeInfo({
				name: "playwright-code.ts",
				mime_type: "video/vnd.dlna.mpeg-tts",
			}),
		).toMatchObject({
			category: "text",
			icon: "FileCode",
		});
	});

	it("derives preview profiles and open-with options", () => {
		const markdown = { name: "notes.md", mime_type: "text/markdown" };
		const json = { name: "data.json", mime_type: "application/json" };
		const xml = { name: "config.xml", mime_type: "application/xml" };
		const image = { name: "photo.png", mime_type: "image/png" };
		const svg = { name: "photo.svg", mime_type: "image/svg+xml" };
		const document = {
			name: "report.docx",
			mime_type:
				"application/vnd.openxmlformats-officedocument.wordprocessingml.document",
		};
		const tsv = {
			name: "report.tsv",
			mime_type: "text/tab-separated-values",
		};
		const shell = { name: "deploy", mime_type: "application/x-sh" };
		const unknown = {
			name: "archive.bin",
			mime_type: "application/octet-stream",
		};

		expect(detectFilePreviewProfile(markdown)).toMatchObject({
			category: "markdown",
			isBlobPreview: false,
			isTextBased: true,
			isEditableText: true,
			defaultMode: "builtin.markdown",
		});
		expect(detectFilePreviewProfile(json)).toMatchObject({
			category: "json",
			defaultMode: "builtin.formatted",
		});
		expect(detectFilePreviewProfile(xml)).toMatchObject({
			category: "xml",
			defaultMode: "builtin.formatted",
		});
		expect(detectFilePreviewProfile(image)).toMatchObject({
			category: "image",
			isBlobPreview: true,
			defaultMode: "builtin.image",
		});
		expect(detectFilePreviewProfile(svg)).toMatchObject({
			category: "image",
			isBlobPreview: true,
			isTextBased: true,
			isEditableText: true,
			defaultMode: "builtin.image",
		});
		expect(detectFilePreviewProfile(document)).toMatchObject({
			category: "document",
			isBlobPreview: false,
			defaultMode: "builtin.office_microsoft",
			isEditableText: false,
		});
		expect(detectFilePreviewProfile(tsv)).toMatchObject({
			category: "tsv",
			defaultMode: "builtin.table",
			isEditableText: true,
		});
		expect(detectFilePreviewProfile(shell)).toMatchObject({
			category: "text",
			defaultMode: "builtin.code",
		});
		expect(detectFilePreviewProfile(unknown)).toMatchObject({
			category: "unknown",
			defaultMode: null,
			isEditableText: true,
			options: [
				{
					key: "builtin.try_text",
					mode: "code",
					labelKey: "open_with_try_text",
				},
			],
		});

		expect(getAvailableOpenWithOptions(json)).toEqual([
			expect.objectContaining({
				key: "builtin.formatted",
				mode: "formatted",
			}),
			expect.objectContaining({ mode: "code" }),
		]);
		expect(getAvailableOpenWithOptions(svg)).toEqual([
			expect.objectContaining({
				key: "builtin.image",
				mode: "image",
			}),
			expect.objectContaining({
				key: "builtin.code",
				mode: "code",
			}),
		]);
		expect(getDefaultOpenWith(json)).toBe("builtin.formatted");
		expect(getDefaultOpenWith(svg)).toBe("builtin.image");
		expect(getDefaultOpenWith(document)).toBe("builtin.office_microsoft");
		expect(getDefaultOpenWith(tsv)).toBe("builtin.table");
		expect(isEditableTextFile(markdown)).toBe(true);
		expect(isEditableTextFile(image)).toBe(false);
		expect(isEditableTextFile(svg)).toBe(true);
		expect(isEditableTextFile(shell)).toBe(true);
	});

	it("uses Google viewer as the only office option for OpenDocument files", () => {
		const document = {
			name: "report.odt",
			mime_type: "application/octet-stream",
		};

		expect(detectFilePreviewProfile(document)).toMatchObject({
			category: "document",
			defaultMode: "builtin.office_google",
			options: [
				expect.objectContaining({
					key: "builtin.office_google",
					mode: "url_template",
				}),
			],
		});
	});

	it("uses backend-configured preview app extensions when available", () => {
		const markdown = { name: "notes.md", mime_type: "text/markdown" };
		const previewApps = {
			version: 2,
			apps: [
				{
					extensions: ["md"],
					icon: "Scroll",
					key: "builtin.markdown",
					labels: {
						en: "Markdown preview",
						zh: "Markdown 预览",
					},
					provider: "builtin",
				},
				{
					icon: "FileCode",
					key: "builtin.code",
					labels: {
						en: "Source view",
						zh: "源码视图",
					},
					provider: "builtin",
				},
				{
					config: {
						allowed_origins: ["https://viewer.example.com"],
						mode: "iframe",
						url_template:
							"https://viewer.example.com/open?src={{file_preview_url}}",
					},
					extensions: ["md"],
					icon: "https://cdn.example.com/icons/external-viewer.svg",
					key: "external",
					labels: {
						en: "External Viewer",
						zh: "外部查看器",
					},
					provider: "url_template",
				},
			],
		};

		expect(detectFilePreviewProfile(markdown, previewApps)).toMatchObject({
			category: "markdown",
			defaultMode: "builtin.markdown",
			options: [
				{ key: "builtin.markdown", mode: "markdown" },
				{
					config: {
						allowed_origins: ["https://viewer.example.com"],
						mode: "iframe",
						url_template:
							"https://viewer.example.com/open?src={{file_preview_url}}",
					},
					key: "external",
					labels: {
						en: "External Viewer",
						zh: "外部查看器",
					},
					mode: "url_template",
				},
				{ key: "builtin.code", mode: "code" },
			],
		});
	});

	it("matches configured office apps by extension", () => {
		const spreadsheet = {
			name: "2025级选课名单0320.xlsx",
			mime_type:
				"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
		};
		const previewApps = {
			version: 2,
			apps: [
				{
					config: {
						allowed_origins: ["https://view.officeapps.live.com"],
						mode: "iframe",
						url_template:
							"https://view.officeapps.live.com/op/embed.aspx?src={{file_preview_url}}",
					},
					extensions: ["xls", "xlsx"],
					icon: "/static/preview-apps/microsoft-onedrive.svg",
					key: "builtin.office_microsoft",
					labels: {
						en: "Microsoft Viewer",
						zh: "Microsoft 预览器",
					},
					provider: "url_template",
				},
				{
					config: {
						allowed_origins: ["https://docs.google.com"],
						mode: "iframe",
						url_template:
							"https://docs.google.com/gview?embedded=true&url={{file_preview_url}}",
					},
					extensions: ["xls", "xlsx", "ods"],
					icon: "/static/preview-apps/google-drive.svg",
					key: "builtin.office_google",
					labels: {
						en: "Google Viewer",
						zh: "Google 预览器",
					},
					provider: "url_template",
				},
			],
		};

		expect(detectFilePreviewProfile(spreadsheet, previewApps)).toMatchObject({
			category: "spreadsheet",
			defaultMode: "builtin.office_microsoft",
			options: [
				expect.objectContaining({
					key: "builtin.office_microsoft",
					mode: "url_template",
				}),
				expect.objectContaining({
					key: "builtin.office_google",
					mode: "url_template",
				}),
			],
		});
	});

	it("keeps configured choices first while exposing every registered app", () => {
		const markdown = { name: "notes.md", mime_type: "text/markdown" };
		const previewApps = {
			version: 2,
			apps: [
				{
					extensions: ["md"],
					icon: "/static/preview-apps/markdown.svg",
					key: "builtin.markdown",
					labels: {
						en: "Markdown preview",
						zh: "Markdown 预览",
					},
					provider: "builtin",
				},
				{
					icon: "/static/preview-apps/code.svg",
					key: "builtin.code",
					labels: {
						en: "Source view",
						zh: "源码视图",
					},
					provider: "builtin",
				},
				{
					config: {
						allowed_origins: ["https://viewer.example.com"],
						mode: "iframe",
						url_template:
							"https://viewer.example.com/open?src={{file_preview_url}}",
					},
					extensions: ["md"],
					icon: "https://cdn.example.com/icons/external-viewer.svg",
					key: "external",
					labels: {
						en: "External Viewer",
						zh: "外部查看器",
					},
					provider: "url_template",
				},
				{
					extensions: ["pdf"],
					icon: "/static/preview-apps/pdf.svg",
					key: "builtin.pdf",
					labels: {
						en: "PDF preview",
						zh: "PDF 预览",
					},
					provider: "builtin",
				},
			],
		};

		expect(detectFilePreviewProfile(markdown, previewApps)).toMatchObject({
			category: "markdown",
			defaultMode: "builtin.markdown",
			options: [
				{ key: "builtin.markdown", mode: "markdown" },
				{ key: "external", mode: "url_template" },
				{ key: "builtin.code", mode: "code" },
			],
			allOptions: [
				{ key: "builtin.markdown", mode: "markdown" },
				{ key: "external", mode: "url_template" },
				{ key: "builtin.code", mode: "code" },
				{ key: "builtin.pdf", mode: "pdf" },
			],
		});
	});

	it("recognizes newly added text extensions", () => {
		const vue = { name: "App.vue", mime_type: "application/octet-stream" };
		const dart = { name: "main.dart", mime_type: "application/octet-stream" };
		const proto = { name: "api.proto", mime_type: "application/octet-stream" };
		const tf = { name: "main.tf", mime_type: "application/octet-stream" };

		for (const file of [vue, dart, proto, tf]) {
			expect(isEditableTextFile(file)).toBe(true);
			expect(getDefaultOpenWith(file)).toBe("builtin.code");
		}
	});

	it("provides text fallback for unknown files but not for known binary types", () => {
		const unknown = {
			name: "mystery.xyz",
			mime_type: "application/octet-stream",
		};
		const archive = {
			name: "data.zip",
			mime_type: "application/octet-stream",
		};
		const sevenZip = {
			name: "data.7z",
			mime_type: "application/octet-stream",
		};

		expect(detectFilePreviewProfile(unknown).isEditableText).toBe(true);
		expect(detectFilePreviewProfile(unknown).options).toEqual([
			expect.objectContaining({
				key: "builtin.try_text",
				mode: "code",
				labelKey: "open_with_try_text",
			}),
		]);
		expect(detectFilePreviewProfile(unknown).defaultMode).toBeNull();

		expect(detectFilePreviewProfile(archive).isEditableText).toBe(false);
		expect(detectFilePreviewProfile(archive).options).toEqual([
			expect.objectContaining({
				key: "builtin.archive",
				mode: "archive",
				labelKey: "open_with_archive",
			}),
		]);
		expect(detectFilePreviewProfile(archive).defaultMode).toBe(
			"builtin.archive",
		);
		expect(detectFilePreviewProfile(sevenZip).defaultMode).toBe(
			"builtin.archive",
		);
	});

	it("uses configured builtins as defaults when their built-in bindings match", () => {
		const json = { name: "data.json", mime_type: "application/json" };
		const previewApps = {
			version: 2,
			apps: [
				{
					icon: "BracketsCurly",
					key: "builtin.formatted",
					labels: {
						en: "Formatted view",
						zh: "格式化视图",
					},
					provider: "builtin",
				},
				{
					icon: "FileCode",
					key: "builtin.code",
					labels: {
						en: "Source view",
						zh: "源码视图",
					},
					provider: "builtin",
				},
			],
		};

		expect(getDefaultOpenWith(json, previewApps)).toBe("builtin.formatted");
	});

	it("recognizes configured wopi providers without treating them as url templates", () => {
		const document = {
			name: "proposal.docx",
			mime_type:
				"application/vnd.openxmlformats-officedocument.wordprocessingml.document",
		};
		const previewApps = {
			version: 2,
			apps: [
				{
					config: {
						mode: "iframe",
					},
					extensions: ["docx"],
					icon: "/static/preview-apps/file.svg",
					key: "custom.onlyoffice",
					labels: {
						en: "OnlyOffice",
						zh: "OnlyOffice",
					},
					provider: "wopi",
				},
			],
		};

		expect(detectFilePreviewProfile(document, previewApps)).toMatchObject({
			category: "document",
			defaultMode: "custom.onlyoffice",
			options: [
				expect.objectContaining({
					key: "custom.onlyoffice",
					mode: "wopi",
				}),
			],
		});
	});

	it("does not infer a runtime provider from the app key when provider is missing", () => {
		const markdown = { name: "notes.md", mime_type: "text/markdown" };
		const previewApps = {
			version: 2,
			apps: [
				{
					config: {
						mode: "iframe",
						url_template:
							"https://viewer.example.com/open?src={{file_preview_url}}",
					},
					extensions: ["md"],
					icon: "https://cdn.example.com/icons/external-viewer.svg",
					key: "custom.viewer",
					labels: {
						en: "External Viewer",
						zh: "外部查看器",
					},
					provider: "",
				},
			],
		};

		const profile = detectFilePreviewProfile(markdown, previewApps);
		expect(profile.defaultMode).not.toBe("custom.viewer");
		expect(profile.options).not.toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					key: "custom.viewer",
				}),
			]),
		);
		expect(profile.allOptions).not.toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					key: "custom.viewer",
				}),
			]),
		);
	});

	it("skips disabled configured apps, including disabled builtin fallbacks", () => {
		const markdown = { name: "notes.md", mime_type: "text/markdown" };
		const previewApps = {
			version: 2,
			apps: [
				{
					enabled: false,
					extensions: ["md"],
					icon: "/static/preview-apps/markdown.svg",
					key: "builtin.markdown",
					labels: {
						en: "Markdown preview",
						zh: "Markdown 预览",
					},
					provider: "builtin",
				},
				{
					icon: "/static/preview-apps/code.svg",
					key: "builtin.code",
					labels: {
						en: "Source view",
						zh: "源码视图",
					},
					provider: "builtin",
				},
			],
		};

		expect(detectFilePreviewProfile(markdown, previewApps)).toMatchObject({
			defaultMode: "builtin.code",
			options: [
				expect.objectContaining({
					key: "builtin.code",
					mode: "code",
				}),
			],
		});
		expect(getAvailableOpenWithOptions(markdown, previewApps)).toEqual([
			expect.objectContaining({
				key: "builtin.code",
			}),
		]);
	});
});
