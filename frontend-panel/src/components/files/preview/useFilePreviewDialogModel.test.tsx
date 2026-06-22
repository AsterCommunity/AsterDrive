import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	type ResourcePath,
	resourceCacheKey,
	resourceCanonicalEtag,
	resourceRequestPath,
} from "@/lib/resourceRequest";
import type { FileListItem, MediaMetadataInfo } from "@/types/api";
import type { FilePreviewResources } from "./filePreviewResources";
import type { FilePreviewProfile, OpenWithOption } from "./types";
import { useFilePreviewDialogModel } from "./useFilePreviewDialogModel";

const codeOption: OpenWithOption = {
	icon: "TextT",
	key: "builtin.code",
	labelKey: "mode_code",
	mode: "code",
};
const markdownOption: OpenWithOption = {
	icon: "MarkdownLogo",
	key: "builtin.markdown",
	labelKey: "mode_markdown",
	mode: "markdown",
};
const archiveOption: OpenWithOption = {
	icon: "Archive",
	key: "builtin.archive",
	labelKey: "mode_archive",
	mode: "archive",
};
const pdfOption: OpenWithOption = {
	icon: "FileText",
	key: "builtin.pdf",
	labelKey: "open_with_pdf",
	mode: "pdf",
};
const wopiOption: OpenWithOption = {
	config: { mode: "iframe" },
	icon: "FileText",
	key: "onlyoffice",
	labelKey: "open_with_onlyoffice",
	mode: "wopi",
};

function expectResourcePath(
	resource: ResourcePath | null,
	expected: {
		cacheKey: string;
		deliveryMode?: string;
		etag?: string | null;
		requestPath: string;
	},
) {
	expect(resource).not.toBeNull();
	expect(resourceCacheKey(resource as ResourcePath)).toBe(expected.cacheKey);
	expect(resourceRequestPath(resource as ResourcePath)).toBe(
		expected.requestPath,
	);
	expect(resourceCanonicalEtag(resource as ResourcePath)).toBe(
		expected.etag ?? null,
	);
	if (
		resource &&
		typeof resource === "object" &&
		"kind" in resource &&
		resource.kind === "ready" &&
		expected.deliveryMode
	) {
		expect(resource.delivery.mode).toBe(expected.deliveryMode);
	}
}

const mockState = vi.hoisted(() => ({
	backendAudioMetadataToTrackMetadata: vi.fn((metadata: MediaMetadataInfo) => ({
		title:
			metadata.kind === "audio" && metadata.metadata?.kind === "audio"
				? metadata.metadata.title
				: undefined,
	})),
	detectFilePreviewProfile: vi.fn(),
	downloadPath: vi.fn((fileId: number) => `/files/${fileId}/download`),
	getFileExtension: vi.fn((file: { name: string }) =>
		file.name.includes(".") ? file.name.split(".").pop()?.toLowerCase() : "",
	),
	getMediaMetadata: vi.fn(),
	imagePreviewPath: vi.fn((fileId: number) => `/files/${fileId}/image-preview`),
	resolveResourceHandle: vi.fn(
		async (
			fileId: number,
			request: { delivery_mode: string; representation?: string },
		) => {
			const isImagePreview = request.representation === "image_preview";
			const isThumbnail = request.representation === "thumbnail";
			const requestPath = isImagePreview
				? `/files/${fileId}/image-preview`
				: isThumbnail
					? `/files/${fileId}/thumbnail`
					: `/files/${fileId}/download?disposition=inline`;
			return {
				kind: "ready",
				identity: {
					cacheKey: isImagePreview
						? `/files/${fileId}/image-preview`
						: isThumbnail
							? `/files/${fileId}/thumbnail`
							: `/files/${fileId}/download`,
					etag: null,
					scope: "personal",
				},
				request: {
					url: requestPath,
					credentials: "include",
					conditionalHeaders: "allowed",
					redirectPolicy: "same_origin_only",
				},
				delivery: {
					mode: request.delivery_mode,
				},
			};
		},
	),
	mediaDataSupportStore: {
		config: {
			enabled: true,
			kinds: {
				audio: {
					enabled: true,
					extensions: ["mp3"],
					match: "extensions",
				},
				image: { enabled: true, extensions: ["jpg"], match: "extensions" },
				video: { enabled: true, extensions: ["mp4"], match: "extensions" },
			},
			max_source_bytes: 1024 * 1024 * 1024,
			version: 1,
		},
		isLoaded: true,
		load: vi.fn(async () => {}),
	},
	previewAppStore: {
		config: null,
		isLoaded: true,
		load: vi.fn(async () => {}),
	},
	thumbnailSupportStore: {
		config: {
			audio_thumbnail: { enabled: false, extensions: [] },
			image_preview: { enabled: true, extensions: ["heic", "nef", "raw"] },
			image_thumbnail: { enabled: true, extensions: ["heic", "nef", "raw"] },
			video_thumbnail: { enabled: false, extensions: [] },
			version: 1,
		},
		isLoaded: true,
		load: vi.fn(async () => {}),
	},
	thumbnailPath: vi.fn((fileId: number) => `/files/${fileId}/thumbnail`),
	videoBrowserOption: null as OpenWithOption | null,
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		downloadPath: (...args: unknown[]) => mockState.downloadPath(...args),
		getMediaMetadata: (...args: unknown[]) =>
			mockState.getMediaMetadata(...args),
		imagePreviewPath: (...args: unknown[]) =>
			mockState.imagePreviewPath(...args),
		resolveResourceHandle: (...args: unknown[]) =>
			mockState.resolveResourceHandle(...args),
		thumbnailPath: (...args: unknown[]) => mockState.thumbnailPath(...args),
	},
}));

vi.mock("@/stores/mediaDataSupportStore", () => ({
	useMediaDataSupportStore: (
		selector: (state: typeof mockState.mediaDataSupportStore) => unknown,
	) => selector(mockState.mediaDataSupportStore),
}));

vi.mock("@/stores/previewAppStore", () => ({
	usePreviewAppStore: (
		selector: (state: typeof mockState.previewAppStore) => unknown,
	) => selector(mockState.previewAppStore),
}));

vi.mock("@/stores/thumbnailSupportStore", () => ({
	useThumbnailSupportStore: (
		selector: (state: typeof mockState.thumbnailSupportStore) => unknown,
	) => selector(mockState.thumbnailSupportStore),
}));

vi.mock("@/lib/musicPlayer", () => ({
	backendAudioMetadataToTrackMetadata: (...args: unknown[]) =>
		mockState.backendAudioMetadataToTrackMetadata(...args),
}));

vi.mock("./file-capabilities", () => ({
	detectFilePreviewProfile: (...args: unknown[]) =>
		mockState.detectFilePreviewProfile(...args),
	getFileExtension: (...args: unknown[]) => mockState.getFileExtension(...args),
}));

vi.mock("./video-browser-config", () => ({
	getVideoBrowserOpenWithOption: () => mockState.videoBrowserOption,
}));

function file(overrides: Partial<FileListItem> = {}): FileListItem {
	return {
		compound_extension: null,
		extension: "md",
		file_category: "document",
		id: 7,
		is_locked: false,
		is_shared: false,
		mime_type: "text/markdown",
		name: "notes.md",
		size: 128,
		updated_at: "2026-01-01T00:00:00Z",
		...overrides,
	};
}

function profile(
	overrides: Partial<FilePreviewProfile> = {},
): FilePreviewProfile {
	return {
		category: "markdown",
		defaultMode: "builtin.code",
		isBlobPreview: false,
		isEditableText: true,
		isTextBased: true,
		options: [codeOption, markdownOption],
		...overrides,
	};
}

function renderModel(
	overrides: Partial<Parameters<typeof useFilePreviewDialogModel>[0]> = {},
) {
	const onClose = vi.fn();
	const resources: FilePreviewResources = overrides.resources ?? {
		scope: "personal",
		paths: {
			download: "/files/7/download",
			imagePreview: "/files/7/image-preview",
			thumbnail: "/files/7/thumbnail",
		},
		resolve: mockState.resolveResourceHandle,
		actions: {},
	};
	const props = {
		open: true,
		file: file(),
		onClose,
		resources,
		translateFileLabel: (key: string) => `files:${key}`,
		...overrides,
	};
	const hook = renderHook(
		(nextProps: typeof props) => useFilePreviewDialogModel(nextProps),
		{ initialProps: props },
	);
	return { ...hook, onClose };
}

function modelProps(
	overrides: Partial<Parameters<typeof useFilePreviewDialogModel>[0]> = {},
) {
	return {
		open: true,
		file: file(),
		onClose: vi.fn(),
		resources: {
			scope: "personal",
			paths: {
				download: "/files/7/download",
				imagePreview: "/files/7/image-preview",
				thumbnail: "/files/7/thumbnail",
			},
			resolve: mockState.resolveResourceHandle,
			actions: {},
		} satisfies FilePreviewResources,
		translateFileLabel: (key: string) => `files:${key}`,
		...overrides,
	};
}

function testResources(
	overrides: Partial<FilePreviewResources> = {},
): FilePreviewResources {
	return {
		scope: "personal",
		paths: {
			download: "/files/7/download",
			imagePreview: "/files/7/image-preview",
			thumbnail: "/files/7/thumbnail",
			...overrides.paths,
		},
		resolve: mockState.resolveResourceHandle,
		...overrides,
		actions: {
			...overrides.actions,
		},
	};
}

describe("useFilePreviewDialogModel", () => {
	beforeEach(() => {
		mockState.backendAudioMetadataToTrackMetadata.mockClear();
		mockState.detectFilePreviewProfile.mockReset();
		mockState.detectFilePreviewProfile.mockReturnValue(profile());
		mockState.downloadPath.mockClear();
		mockState.getFileExtension.mockClear();
		mockState.getMediaMetadata.mockReset();
		mockState.resolveResourceHandle.mockClear();
		mockState.getMediaMetadata.mockResolvedValue({
			kind: "audio",
			metadata: {
				has_embedded_picture: false,
				kind: "audio",
				title: "Backend Song",
			},
			status: "ready",
		});
		mockState.imagePreviewPath.mockClear();
		mockState.mediaDataSupportStore.isLoaded = true;
		mockState.mediaDataSupportStore.load.mockReset();
		mockState.mediaDataSupportStore.load.mockResolvedValue(undefined);
		mockState.previewAppStore.config = null;
		mockState.previewAppStore.isLoaded = true;
		mockState.previewAppStore.load.mockReset();
		mockState.previewAppStore.load.mockResolvedValue(undefined);
		mockState.thumbnailSupportStore.config = {
			audio_thumbnail: { enabled: false, extensions: [] },
			image_preview: { enabled: true, extensions: ["heic", "nef", "raw"] },
			image_thumbnail: { enabled: true, extensions: ["heic", "nef", "raw"] },
			video_thumbnail: { enabled: false, extensions: [] },
			version: 1,
		};
		mockState.thumbnailSupportStore.isLoaded = true;
		mockState.thumbnailSupportStore.load.mockReset();
		mockState.thumbnailSupportStore.load.mockResolvedValue(undefined);
		mockState.thumbnailPath.mockClear();
		mockState.videoBrowserOption = null;
		vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
			matches: false,
			media: query,
			onchange: null,
			addEventListener: vi.fn(),
			removeEventListener: vi.fn(),
			addListener: vi.fn(),
			removeListener: vi.fn(),
			dispatchEvent: vi.fn(),
		}));
		vi.useRealTimers();
	});

	it("loads bootstrap stores and hides profile-dependent UI until preview apps load", () => {
		mockState.mediaDataSupportStore.isLoaded = false;
		mockState.previewAppStore.isLoaded = false;
		mockState.thumbnailSupportStore.isLoaded = false;

		const { result } = renderModel();

		expect(mockState.previewAppStore.load).toHaveBeenCalledTimes(1);
		expect(mockState.thumbnailSupportStore.load).toHaveBeenCalledTimes(1);
		expect(result.current.profile).toBeNull();
		expect(result.current.activeMode).toBeNull();
		expect(result.current.showOpenMethodChooser).toBe(false);
	});

	it("does not resolve content resources for audio files in the preview dialog", async () => {
		const audioFile = file({
			extension: "mp3",
			file_category: "audio",
			mime_type: "audio/mpeg",
			name: "Backend Song.mp3",
		});
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				category: "audio",
				defaultMode: "builtin.audio",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [
					{
						icon: "FileAudio",
						key: "builtin.audio",
						labelKey: "open_with_audio",
						mode: "audio",
					},
				],
			}),
		);

		const { result } = renderModel({ file: audioFile, openMode: "direct" });

		expect(result.current.resolvedDownloadPath).toBe("/files/7/download");
		expect(result.current.resolvedContentPreviewPath).toBeNull();
		expect(result.current.resolvedImagePreviewPath).toBe(
			"/files/7/image-preview",
		);
		expect(result.current.resolvedThumbnailPath).toBe("/files/7/thumbnail");
		expect(mockState.getMediaMetadata).not.toHaveBeenCalled();
	});

	it("resolves read-only content previews through the resource resolver", async () => {
		const createExternalPreviewLink = vi.fn(async () => ({
			etag: '"etag-manual"',
			expires_at: "2026-06-21T22:30:00Z",
			max_uses: 5,
			path: "/pv/token/manual.pdf",
		}));
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				category: "pdf",
				defaultMode: "builtin.pdf",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [pdfOption],
			}),
		);

		const { result } = renderModel({
			file: file({
				extension: "pdf",
				file_category: "document",
				mime_type: "application/pdf",
				name: "manual.pdf",
			}),
			openMode: "direct",
			resources: {
				scope: "personal",
				paths: {
					download: "/files/7/download",
					imagePreview: "/files/7/image-preview",
					thumbnail: "/files/7/thumbnail",
				},
				resolve: mockState.resolveResourceHandle,
				actions: {
					createExternalPreviewLink: createExternalPreviewLink,
				},
			},
		});

		expect(result.current.resolvedContentPreviewPath).toBeNull();
		await waitFor(() => {
			expectResourcePath(result.current.resolvedContentPreviewPath, {
				cacheKey: "/files/7/download",
				deliveryMode: "blob_url",
				requestPath: "/files/7/download?disposition=inline",
			});
		});
		expect(result.current.resolvedDownloadPath).toBe("/files/7/download");
		expect(createExternalPreviewLink).not.toHaveBeenCalled();
	});

	it("reuses the in-flight resource request when parent callbacks change", async () => {
		let resolveResource!: (
			value: Awaited<ReturnType<typeof mockState.resolveResourceHandle>>,
		) => void;
		const resourcePromise = new Promise<
			Awaited<ReturnType<typeof mockState.resolveResourceHandle>>
		>((resolve) => {
			resolveResource = resolve;
		});
		const firstResolve = vi.fn(() => resourcePromise);
		const secondResolve = vi.fn(() => resourcePromise);

		const { rerender, result } = renderModel({
			openMode: "direct",
			resources: {
				scope: "personal",
				paths: {
					download: "/files/7/download",
					imagePreview: "/files/7/image-preview",
					thumbnail: "/files/7/thumbnail",
				},
				resolve: firstResolve,
				actions: {},
			},
		});

		rerender(
			modelProps({
				openMode: "direct",
				resources: {
					scope: "personal",
					paths: {
						download: "/files/7/download",
						imagePreview: "/files/7/image-preview",
						thumbnail: "/files/7/thumbnail",
					},
					resolve: secondResolve,
					actions: {},
				},
			}),
		);

		expect(result.current.resolvedContentPreviewPath).toBeNull();
		expect(firstResolve).toHaveBeenCalledTimes(1);
		expect(secondResolve).not.toHaveBeenCalled();

		resolveResource({
			kind: "ready",
			identity: {
				cacheKey: "/files/7/download",
				etag: '"etag-notes"',
				scope: "personal",
			},
			request: {
				url: "/files/7/download?disposition=inline",
				credentials: "include",
				conditionalHeaders: "allowed",
				redirectPolicy: "same_origin_only",
			},
			delivery: {
				mode: "text",
			},
		});

		await waitFor(() => {
			expectResourcePath(result.current.resolvedContentPreviewPath, {
				cacheKey: "/files/7/download",
				deliveryMode: "text",
				etag: '"etag-notes"',
				requestPath: "/files/7/download?disposition=inline",
			});
		});
		expect(firstResolve).toHaveBeenCalledTimes(1);
		expect(secondResolve).not.toHaveBeenCalled();
	});

	it("clears content preview resources while the kept-mounted dialog is closing", async () => {
		const { rerender, result } = renderModel({
			openMode: "direct",
		});

		await waitFor(() => {
			expectResourcePath(result.current.resolvedContentPreviewPath, {
				cacheKey: "/files/7/download",
				deliveryMode: "text",
				requestPath: "/files/7/download?disposition=inline",
			});
		});

		rerender(
			modelProps({
				open: false,
				file: file(),
				onClose: vi.fn(),
				openMode: "direct",
			}),
		);

		expect(result.current.resolvedContentPreviewPath).toBeNull();
		expect(result.current.resolvedDownloadPath).toBe("/files/7/download");
		expect(mockState.resolveResourceHandle).toHaveBeenCalledTimes(1);
	});

	it("keeps content preview unresolved when the resource resolver fails", async () => {
		const resolve = vi.fn(async () => {
			throw new Error("resource resolve failed");
		});

		const { result } = renderModel({
			openMode: "direct",
			resources: testResources({ resolve }),
		});

		await waitFor(() => {
			expect(resolve).toHaveBeenCalledTimes(1);
		});
		expect(result.current.resolvedContentPreviewPath).toBeNull();
	});

	it("does not retry resource resolution after a failed request while the file stays open", async () => {
		const resolve = vi.fn(async () => {
			throw new Error("resource resolve failed");
		});

		const { rerender, result } = renderModel({
			openMode: "direct",
			resources: testResources({ resolve }),
		});

		await waitFor(() => {
			expect(resolve).toHaveBeenCalledTimes(1);
		});
		expect(result.current.resolvedContentPreviewPath).toBeNull();

		rerender(
			modelProps({
				open: true,
				file: file(),
				onClose: vi.fn(),
				openMode: "direct",
				resources: testResources({ resolve }),
			}),
		);

		expect(result.current.resolvedContentPreviewPath).toBeNull();
		expect(resolve).toHaveBeenCalledTimes(1);
	});

	it("does not retry resource resolution after a ready resource while the file stays open", async () => {
		const resolve = vi.fn(mockState.resolveResourceHandle);

		const { rerender, result } = renderModel({
			openMode: "direct",
			resources: testResources({ resolve }),
		});

		await waitFor(() => {
			expectResourcePath(result.current.resolvedContentPreviewPath, {
				cacheKey: "/files/7/download",
				deliveryMode: "text",
				requestPath: "/files/7/download?disposition=inline",
			});
		});

		rerender(
			modelProps({
				open: true,
				file: file(),
				onClose: vi.fn(),
				openMode: "direct",
				resources: testResources({ resolve }),
			}),
		);

		expectResourcePath(result.current.resolvedContentPreviewPath, {
			cacheKey: "/files/7/download",
			deliveryMode: "text",
			requestPath: "/files/7/download?disposition=inline",
		});
		expect(resolve).toHaveBeenCalledTimes(1);
	});

	it("ignores stale resource resolver results after file changes", async () => {
		let resolveFirst!: (
			value: Awaited<ReturnType<typeof mockState.resolveResourceHandle>>,
		) => void;
		const firstPromise = new Promise<
			Awaited<ReturnType<typeof mockState.resolveResourceHandle>>
		>((resolve) => {
			resolveFirst = resolve;
		});
		const resolve = vi
			.fn()
			.mockReturnValueOnce(firstPromise)
			.mockResolvedValueOnce({
				kind: "ready",
				identity: {
					cacheKey: "/files/8/download",
					etag: '"etag-next"',
					scope: "personal",
				},
				request: {
					url: "/files/8/download?disposition=inline",
					credentials: "include",
					conditionalHeaders: "allowed",
					redirectPolicy: "same_origin_only",
				},
				delivery: {
					mode: "text",
				},
			});

		const { rerender, result } = renderModel({
			file: file({ id: 7, name: "manual.pdf" }),
			openMode: "direct",
			resources: testResources({ resolve }),
		});

		rerender(
			modelProps({
				open: true,
				file: file({ id: 8, name: "next.pdf" }),
				onClose: vi.fn(),
				openMode: "direct",
				resources: testResources({
					paths: {
						download: "/files/8/download",
						imagePreview: "/files/8/image-preview",
						thumbnail: "/files/8/thumbnail",
					},
					resolve,
				}),
			}),
		);

		resolveFirst({
			kind: "ready",
			identity: {
				cacheKey: "/files/7/download",
				etag: '"etag-manual"',
				scope: "personal",
			},
			request: {
				url: "/files/7/download?disposition=inline",
				credentials: "include",
				conditionalHeaders: "allowed",
				redirectPolicy: "same_origin_only",
			},
			delivery: {
				mode: "text",
			},
		});

		await waitFor(() => {
			expectResourcePath(result.current.resolvedContentPreviewPath, {
				cacheKey: "/files/8/download",
				deliveryMode: "text",
				etag: '"etag-next"',
				requestPath: "/files/8/download?disposition=inline",
			});
		});
		expect(
			resourceRequestPath(
				result.current.resolvedContentPreviewPath as ResourcePath,
			),
		).not.toBe("/files/7/download?disposition=inline");
	});

	it("does not expose audio metadata loaders from the preview dialog model", async () => {
		const audioFile = file({
			extension: "mp3",
			file_category: "audio",
			mime_type: "audio/mpeg",
			name: "Manual.mp3",
		});

		const { result } = renderModel({
			file: audioFile,
		});

		expect(result.current.resolvedContentPreviewPath).toBeNull();
		expect(mockState.getMediaMetadata).not.toHaveBeenCalled();
	});

	it("filters unavailable archive and WOPI options and falls back to all options when needed", () => {
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				allOptions: [codeOption, archiveOption, wopiOption],
				defaultMode: "builtin.archive",
				options: [archiveOption, wopiOption],
			}),
		);

		const { result } = renderModel();

		expect(result.current.allOptions.map((option) => option.key)).toEqual([
			"builtin.code",
		]);
		expect(result.current.visibleOptions.map((option) => option.key)).toEqual([
			"builtin.code",
		]);
		expect(result.current.activeMode).toBe("builtin.code");
		expect(result.current.showOpenMethodChooser).toBe(false);
	});

	it("switches to hidden archive methods and calls the latest archive factory ref", async () => {
		const initialArchiveFactory = vi.fn(async () => ({ entries: [] }));
		const latestArchiveFactory = vi.fn(async () => ({
			entries: [{ name: "new.txt" }],
		}));
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				allOptions: [codeOption, archiveOption],
				defaultMode: "builtin.code",
				options: [codeOption],
			}),
		);

		const { rerender, result } = renderModel({
			resources: testResources({
				actions: { loadArchiveManifest: initialArchiveFactory },
			}),
		});

		expect(result.current.hiddenOptions.map((option) => option.key)).toEqual([
			"builtin.archive",
		]);

		act(() => {
			result.current.handleOpenMethodSelect("builtin.archive");
		});
		await waitFor(() => {
			expect(result.current.activeOption?.mode).toBe("archive");
		});
		expect(result.current.showAllOpenMethods).toBe(true);
		expect(result.current.activeArchiveManifestLoader).toBeTypeOf("function");

		rerender(
			modelProps({
				open: true,
				file: file(),
				onClose: vi.fn(),
				resources: testResources({
					actions: { loadArchiveManifest: latestArchiveFactory },
				}),
			}),
		);

		await expect(
			result.current.activeArchiveManifestLoader?.({
				signal: new AbortController().signal,
			}),
		).resolves.toEqual({ entries: [{ name: "new.txt" }] });
		expect(initialArchiveFactory).not.toHaveBeenCalled();
		expect(latestArchiveFactory).toHaveBeenCalledWith({
			signal: expect.any(AbortSignal),
		});
	});

	it("opens WOPI methods only when a session factory is available", async () => {
		const launchWopiSession = vi.fn(async (appKey: string) => ({
			app_key: appKey,
			expires_at: "2026-01-01T00:00:00Z",
			launch_url: "https://office.example/launch",
			token: "token",
		}));
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				defaultMode: "onlyoffice",
				options: [wopiOption],
			}),
		);

		const { result } = renderModel({
			resources: testResources({
				actions: { launchWopiSession: launchWopiSession },
			}),
		});

		expect(result.current.activeMode).toBe("onlyoffice");
		expect(result.current.usesInnerScroll).toBe(true);
		expect(result.current.fillsViewportHeight).toBe(true);
		await expect(result.current.launchWopiSession?.()).resolves.toMatchObject({
			app_key: "onlyoffice",
		});
		expect(launchWopiSession).toHaveBeenCalledWith("onlyoffice");
	});

	it("uses new-tab embedded options without forcing inner scroll or viewport height", () => {
		const urlTemplateOption: OpenWithOption = {
			config: { mode: "new_tab" },
			icon: "Globe",
			key: "docs",
			labelKey: "open_with_docs",
			mode: "url_template",
		};
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				defaultMode: "docs",
				options: [urlTemplateOption],
			}),
		);

		const { result } = renderModel({ openMode: "direct" });

		expect(result.current.activeMode).toBe("docs");
		expect(result.current.usesInnerScroll).toBe(false);
		expect(result.current.fillsViewportHeight).toBe(false);
	});

	it("skips the chooser for text-based image previews even with multiple methods", () => {
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				category: "image",
				defaultMode: "builtin.image",
				isBlobPreview: true,
				isEditableText: true,
				isTextBased: true,
				options: [
					{
						icon: "FileImage",
						key: "builtin.image",
						labelKey: "open_with_image",
						mode: "image",
					},
					codeOption,
				],
			}),
		);

		const { result } = renderModel({
			file: file({
				extension: "svg",
				file_category: "image",
				mime_type: "image/svg+xml",
				name: "diagram.svg",
			}),
		});

		expect(result.current.showOpenMethodChooser).toBe(false);
		expect(result.current.activeMode).toBe("builtin.image");
	});

	it("defaults image previews to fullscreen with image-only overlay styling", () => {
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				category: "image",
				defaultMode: "builtin.image",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [
					{
						icon: "FileImage",
						key: "builtin.image",
						labelKey: "open_with_image",
						mode: "image",
					},
				],
			}),
		);

		const { result } = renderModel({
			file: file({
				extension: "png",
				file_category: "image",
				mime_type: "image/png",
				name: "photo.png",
			}),
			openMode: "direct",
		});

		expect(result.current.isImagePreview).toBe(true);
		expect(result.current.isExpanded).toBe(true);
		expect(result.current.dialogContentClassName.split(/\s+/)).toEqual(
			expect.arrayContaining([
				"group/image-preview",
				"top-0",
				"left-0",
				"h-screen",
				"w-screen",
				"rounded-none",
			]),
		);
		expect(result.current.dialogOverlayClassName).toContain("bg-zinc-950/88");
	});

	it("lets desktop image previews collapse after a manual expand toggle", () => {
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				category: "image",
				defaultMode: "builtin.image",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [
					{
						icon: "FileImage",
						key: "builtin.image",
						labelKey: "open_with_image",
						mode: "image",
					},
				],
			}),
		);

		const { result } = renderModel({
			file: file({
				extension: "png",
				file_category: "image",
				mime_type: "image/png",
				name: "photo.png",
			}),
			openMode: "direct",
		});

		expect(result.current.isExpanded).toBe(true);

		act(() => {
			result.current.handleExpandToggle();
		});

		expect(result.current.isExpanded).toBe(false);
		expect(result.current.dialogContentClassName.split(/\s+/)).not.toContain(
			"top-0",
		);
	});

	it("keeps PDF previews windowed on desktop", () => {
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				category: "pdf",
				defaultMode: "builtin.pdf",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [pdfOption],
			}),
		);

		const { result } = renderModel({
			file: file({
				extension: "pdf",
				file_category: "document",
				mime_type: "application/pdf",
				name: "manual.pdf",
			}),
			openMode: "direct",
		});

		expect(result.current.isImagePreview).toBe(false);
		expect(result.current.isExpanded).toBe(false);
		expect(result.current.dialogContentClassName.split(/\s+/)).not.toContain(
			"top-0",
		);
		expect(result.current.dialogOverlayClassName).toBeUndefined();
	});

	it("forces non-image previews to fullscreen on mobile", () => {
		vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
			matches: query === "(max-width: 767px)",
			media: query,
			onchange: null,
			addEventListener: vi.fn(),
			removeEventListener: vi.fn(),
			addListener: vi.fn(),
			removeListener: vi.fn(),
			dispatchEvent: vi.fn(),
		}));
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				category: "pdf",
				defaultMode: "builtin.pdf",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [pdfOption],
			}),
		);

		const { result } = renderModel({
			file: file({
				extension: "pdf",
				file_category: "document",
				mime_type: "application/pdf",
				name: "manual.pdf",
			}),
			openMode: "direct",
		});

		expect(result.current.isImagePreview).toBe(false);
		expect(result.current.isExpanded).toBe(true);
		expect(result.current.dialogContentClassName.split(/\s+/)).toEqual(
			expect.arrayContaining([
				"top-0",
				"left-0",
				"h-screen",
				"w-screen",
				"rounded-none",
			]),
		);

		act(() => {
			result.current.handleExpandToggle();
		});

		expect(result.current.isExpanded).toBe(true);
		expect(result.current.dialogContentClassName.split(/\s+/)).toContain(
			"top-0",
		);
	});

	it("does not apply image overlay styling to non-image previews", () => {
		const { result } = renderModel({ openMode: "direct" });

		expect(result.current.isImagePreview).toBe(false);
		expect(result.current.dialogOverlayClassName).toBeUndefined();
	});

	it("guards close while dirty, discards changes, and routes chooser close directly", () => {
		const { onClose, result } = renderModel({ openMode: "direct" });

		act(() => {
			result.current.handleDialogOpenChange(true);
		});
		expect(onClose).not.toHaveBeenCalled();

		act(() => {
			result.current.setIsDirty(true);
		});
		act(() => {
			result.current.handleDialogOpenChange(false);
		});
		expect(result.current.confirmOpen).toBe(true);
		expect(onClose).not.toHaveBeenCalled();

		act(() => {
			result.current.handleDiscardChanges();
		});
		expect(onClose).toHaveBeenCalledTimes(1);
		expect(result.current.isDirty).toBe(false);
		expect(result.current.confirmOpen).toBe(false);

		const chooser = renderModel();
		act(() => {
			chooser.result.current.handleDialogOpenChange(false);
		});
		expect(chooser.onClose).toHaveBeenCalledTimes(1);
	});

	it("disables dialog animation after open and resets expanded state on file change", async () => {
		vi.useFakeTimers();
		const { rerender, result } = renderModel({ openMode: "direct" });

		expect(result.current.isDialogAnimationEnabled).toBe(true);
		act(() => {
			vi.advanceTimersByTime(120);
		});
		expect(result.current.isDialogAnimationEnabled).toBe(false);

		act(() => {
			result.current.handleExpandToggle();
		});
		expect(result.current.isExpanded).toBe(true);

		rerender(
			modelProps({
				open: true,
				file: file({ id: 8, name: "next.md" }),
				onClose: vi.fn(),
				openMode: "direct",
			}),
		);
		expect(result.current.isExpanded).toBe(false);
	});

	it("adds a custom video browser only when the base profile lacks it", () => {
		mockState.videoBrowserOption = {
			config: { mode: "iframe" },
			icon: "Globe",
			key: "video-browser",
			label: "Jellyfin",
			labelKey: "open_with_custom_video_browser",
			mode: "url_template",
		};
		mockState.detectFilePreviewProfile.mockReturnValue(
			profile({
				category: "video",
				defaultMode: "builtin.video",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [
					{
						icon: "Monitor",
						key: "builtin.video",
						labelKey: "open_with_video",
						mode: "video",
					},
				],
			}),
		);

		const { result } = renderModel({
			file: file({
				extension: "mp4",
				file_category: "video",
				mime_type: "video/mp4",
				name: "clip.mp4",
			}),
		});

		expect(result.current.allOptions.map((option) => option.key)).toEqual([
			"builtin.video",
			"video-browser",
		]);
		expect(result.current.getOptionLabel(result.current.allOptions[1])).toBe(
			"Jellyfin",
		);
	});
});
