import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FilePreviewDialog } from "@/components/files/preview/FilePreviewDialog";
import {
	type ResourcePath,
	resourceCacheKey,
	resourceCanonicalEtag,
	resourceRequestPath,
} from "@/lib/resourceRequest";
import type { FilePreviewResources } from "./filePreviewResources";

const mockState = vi.hoisted(() => ({
	downloadPath: vi.fn((fileId: number) => `/files/${fileId}/download`),
	getMediaMetadata: vi.fn(),
	imagePreviewPath: vi.fn((fileId: number) => `/files/${fileId}/image-preview`),
	resolveResourceHandle: vi.fn(
		async (
			fileId: number,
			request: { delivery_mode: string; representation?: string },
		) => {
			const basePath =
				request.representation === "image_preview"
					? `/files/${fileId}/image-preview`
					: request.representation === "thumbnail"
						? `/files/${fileId}/thumbnail`
						: `/files/${fileId}/download?disposition=inline`;
			return {
				kind: "ready",
				identity: {
					cacheKey:
						request.representation === "image_preview"
							? `/files/${fileId}/image-preview`
							: `/files/${fileId}/download`,
					etag: null,
					scope: "personal",
				},
				request: {
					url: basePath,
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
	thumbnailPath: vi.fn((fileId: number) => `/files/${fileId}/thumbnail`),
	profile: {
		category: "markdown",
		defaultMode: "builtin.code",
		isBlobPreview: false,
		isEditableText: true,
		isTextBased: true,
		options: [
			{
				icon: "TextT",
				key: "builtin.code",
				labelKey: "mode_code",
				mode: "code",
			},
			{
				icon: "MarkdownLogo",
				key: "builtin.markdown",
				labelKey: "mode_markdown",
				mode: "markdown",
			},
		],
	},
	mediaDataSupportStore: {
		config: {
			enabled: true,
			kinds: {
				audio: {
					enabled: true,
					extensions: ["mp3", "flac"],
					match: "extensions",
				},
				image: { enabled: true, extensions: ["jpg"], match: "extensions" },
				video: { enabled: false, extensions: [], match: "extensions" },
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
			audio_thumbnail: { enabled: true, extensions: ["mp3"] },
			extensions: ["heic", "nef", "raw", "mp3"],
			image_preview: {
				enabled: true,
				extensions: ["heic", "nef", "raw"],
			},
			image_thumbnail: {
				enabled: true,
				extensions: ["heic", "nef", "raw"],
			},
			video_thumbnail: { enabled: true, extensions: ["mp4"] },
			version: 1,
		},
		isLoaded: true,
		load: vi.fn(async () => {}),
	},
	videoBrowserOption: null as {
		config?: Record<string, unknown>;
		icon: string;
		key: string;
		label?: string;
		labelKey: string;
		mode: string;
	} | null,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/files/FileTypeIcon", () => ({
	FileTypeIcon: ({
		mimeType,
		fileName,
	}: {
		mimeType: string;
		fileName: string;
	}) => <span>{`${mimeType}:${fileName}`}</span>,
}));

vi.mock("@/components/files/FileThumbnail", () => ({
	FileThumbnail: ({
		file,
		thumbnailPath,
	}: {
		file: { name: string };
		thumbnailPath?: string;
	}) => (
		<span
			data-testid="file-thumbnail"
			data-file-name={file.name}
			data-thumbnail-resource={thumbnailPath ?? ""}
		/>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		onClick,
		className,
		...props
	}: {
		children: React.ReactNode;
		onClick?: () => void;
		className?: string;
	} & React.ButtonHTMLAttributes<HTMLButtonElement>) => (
		<button type="button" onClick={onClick} className={className} {...props}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children }: { children: React.ReactNode }) => (
		<div data-testid="dialog">{children}</div>
	),
	DialogContent: ({
		children,
		className,
		overlayClassName,
	}: {
		children: React.ReactNode;
		className?: string;
		overlayClassName?: string;
	}) => (
		<>
			<div
				data-testid="dialog-overlay"
				className={overlayClassName}
				aria-hidden="true"
			/>
			<div data-testid="dialog-content" className={className}>
				{children}
			</div>
		</>
	),
	DialogHeader: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogTitle: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <h2 className={className}>{children}</h2>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
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

vi.mock("@/components/files/preview/BlobImagePreview", () => ({
	BlobImagePreview: ({
		file,
		fallbackResource,
		fillContainer,
		resource,
	}: {
		file: { name: string };
		fallbackResource?: ResourcePath | null;
		fillContainer?: boolean;
		resource: ResourcePath | null;
	}) => (
		<img
			alt={file.name}
			data-cache-key={resource ? resourceCacheKey(resource) : ""}
			data-preview-etag={
				resource ? (resourceCanonicalEtag(resource) ?? "") : ""
			}
			data-fallback-resource={
				fallbackResource ? resourceRequestPath(fallbackResource) : ""
			}
			data-fill-container={String(Boolean(fillContainer))}
			data-preview-resource={resource ? resourceRequestPath(resource) : ""}
			src={resource ? `blob:${resourceRequestPath(resource)}` : "blob:loading"}
		/>
	),
}));

vi.mock("@/components/files/preview/UrlTemplatePreview", () => ({
	UrlTemplatePreview: ({
		createExternalPreviewLink,
		downloadPath,
		label,
		rawConfig,
	}: {
		createExternalPreviewLink?: (() => Promise<unknown>) | undefined;
		downloadPath: string;
		label: string;
		rawConfig: Record<string, unknown> | null | undefined;
	}) => (
		<div>
			{`url-template:${label}:${downloadPath}:${String(rawConfig?.url_template ?? "")}:${String(Boolean(createExternalPreviewLink))}`}
		</div>
	),
}));

vi.mock("@/components/files/preview/WopiPreview", () => ({
	WopiPreview: ({
		label,
		rawConfig,
		sessionResource,
	}: {
		label: string;
		rawConfig: Record<string, unknown> | null | undefined;
		sessionResource: unknown;
	}) => (
		<div>
			{`wopi:${label}:${String(rawConfig?.mode ?? "")}:${String(Boolean(sessionResource))}`}
		</div>
	),
}));

vi.mock(
	"@/components/files/preview/file-capabilities",
	async (importOriginal) => {
		const actual =
			await importOriginal<
				typeof import("@/components/files/preview/file-capabilities")
			>();
		return {
			...actual,
			detectFilePreviewProfile: () => mockState.profile,
		};
	},
);

vi.mock("@/components/files/preview/video-browser-config", () => ({
	getVideoBrowserOpenWithOption: () => mockState.videoBrowserOption,
}));

vi.mock("@/components/files/preview/PreviewUnavailable", () => ({
	PreviewUnavailable: () => <div>preview-unavailable</div>,
}));

vi.mock("@/components/files/preview/VideoPreview", () => ({
	VideoPreview: ({
		createMediaStreamSession,
		resource,
	}: {
		createMediaStreamSession?: () => Promise<unknown>;
		resource: ResourcePath | null;
	}) => (
		<div
			data-has-media-stream-link-factory={String(
				Boolean(createMediaStreamSession),
			)}
		>{`video:${resource ? resourceRequestPath(resource) : ""}`}</div>
	),
}));

vi.mock("@/components/files/preview/UnsavedChangesGuard", () => ({
	UnsavedChangesGuard: ({
		open,
		onOpenChange,
		onConfirm,
	}: {
		open: boolean;
		onOpenChange: (open: boolean) => void;
		onConfirm: () => void;
	}) =>
		open ? (
			<div>
				<div>unsaved-guard</div>
				<button type="button" onClick={() => onOpenChange(false)}>
					cancel-guard
				</button>
				<button type="button" onClick={onConfirm}>
					discard-changes
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/files/preview/PdfPreview", () => ({
	PdfPreview: ({
		resource,
		fileName,
	}: {
		resource: ResourcePath;
		fileName: string;
	}) => <div>{`pdf:${fileName}:${resourceRequestPath(resource)}`}</div>,
}));

vi.mock("@/components/files/preview/MarkdownPreview", () => ({
	MarkdownPreview: ({ resource }: { resource: ResourcePath }) => (
		<div>{`markdown:${resourceRequestPath(resource)}`}</div>
	),
}));

vi.mock("@/components/files/preview/CsvTablePreview", () => ({
	CsvTablePreview: ({
		resource,
		delimiter,
	}: {
		resource: ResourcePath;
		delimiter: string;
	}) => <div>{`table:${delimiter}:${resourceRequestPath(resource)}`}</div>,
}));

vi.mock("@/components/files/preview/JsonPreview", () => ({
	JsonPreview: ({ resource }: { resource: ResourcePath }) => (
		<div>{`json:${resourceRequestPath(resource)}`}</div>
	),
}));

vi.mock("@/components/files/preview/XmlPreview", () => ({
	XmlPreview: ({
		resource,
		mode,
	}: {
		resource: ResourcePath;
		mode: string;
	}) => <div>{`xml:${mode}:${resourceRequestPath(resource)}`}</div>,
}));

vi.mock("@/components/files/preview/TextCodePreview", () => ({
	TextCodePreview: ({
		resource,
		editable,
		onDirtyChange,
	}: {
		resource: ResourcePath;
		editable: boolean;
		onDirtyChange: (dirty: boolean) => void;
	}) => (
		<div>
			<div>{`code:${resourceRequestPath(resource)}:${String(editable)}`}</div>
			<button type="button" onClick={() => onDirtyChange(true)}>
				mark-dirty
			</button>
		</div>
	),
}));

function renderDialog(
	overrides: Partial<React.ComponentProps<typeof FilePreviewDialog>> = {},
) {
	const onClose = vi.fn();
	const onFileUpdated = vi.fn();
	const file = (overrides.file ?? {
		id: 7,
		mime_type: "text/markdown",
		name: "notes.md",
		size: 128,
	}) as React.ComponentProps<typeof FilePreviewDialog>["file"];
	const resources = overrides.resources ?? testResources(file.id);

	render(
		<FilePreviewDialog
			open
			file={file}
			onClose={onClose}
			onFileUpdated={onFileUpdated}
			resources={resources}
			editable
			{...overrides}
		/>,
	);

	return { onClose, onFileUpdated };
}

function testResources(
	fileId = 7,
	overrides: Partial<FilePreviewResources> = {},
): FilePreviewResources {
	return {
		scope: "personal",
		resolve: mockState.resolveResourceHandle,
		...overrides,
		paths: {
			download: `/files/${fileId}/download`,
			imagePreview: `/files/${fileId}/image-preview`,
			thumbnail: `/files/${fileId}/thumbnail`,
			...overrides.paths,
		},
		actions: {
			...overrides.actions,
		},
	};
}

async function chooseOpenMethod(name: string) {
	const label = await screen.findByText(name);
	const button = label.closest("button");
	if (!button) {
		throw new Error(`Open method button not found for label: ${name}`);
	}
	fireEvent.click(button);
}

describe("FilePreviewDialog", () => {
	beforeEach(() => {
		mockState.downloadPath.mockClear();
		mockState.getMediaMetadata.mockReset();
		mockState.resolveResourceHandle.mockClear();
		mockState.mediaDataSupportStore.isLoaded = true;
		mockState.mediaDataSupportStore.load.mockReset();
		mockState.imagePreviewPath.mockClear();
		mockState.thumbnailPath.mockClear();
		mockState.previewAppStore.load.mockReset();
		mockState.profile = {
			category: "markdown",
			defaultMode: "builtin.code",
			isBlobPreview: false,
			isEditableText: true,
			isTextBased: true,
			options: [
				{
					icon: "TextT",
					key: "builtin.code",
					labelKey: "mode_code",
					mode: "code",
				},
				{
					icon: "MarkdownLogo",
					key: "builtin.markdown",
					labelKey: "mode_markdown",
					mode: "markdown",
				},
			],
		};
		mockState.previewAppStore.config = null;
		mockState.previewAppStore.isLoaded = true;
		mockState.videoBrowserOption = null;
		mockState.getMediaMetadata.mockResolvedValue({
			kind: "audio",
			metadata: {
				artist: "Backend Artist",
				has_embedded_picture: false,
				kind: "audio",
				title: "Backend Title",
			},
			status: "ready",
		});
	});

	it("uses the default open method and the default download path", async () => {
		renderDialog();

		expect(mockState.downloadPath).not.toHaveBeenCalled();
		expect(screen.getByText("files:choose_open_method")).toBeInTheDocument();
		expect(screen.getByText("notes.md")).toBeInTheDocument();
		expect(screen.getByText("bytes:128")).toBeInTheDocument();
		expect(screen.getByTestId("file-thumbnail")).toHaveAttribute(
			"data-thumbnail-resource",
			"/files/7/thumbnail",
		);
		expect(
			screen.getByTestId("dialog-content").className.split(/\s+/),
		).toContain("max-h-[min(90vh,calc(100vh-2rem))]");
		expect(
			screen.getByText("files:mode_code").closest("button")?.className,
		).toContain("border-primary");
		await chooseOpenMethod("files:mode_markdown");
		expect(
			await screen.findByText("markdown:/files/7/download?disposition=inline"),
		).toBeInTheDocument();
	});

	it("keeps a fixed work area for editor-style previews", async () => {
		renderDialog();

		await chooseOpenMethod("files:mode_code");
		await screen.findByText("code:/files/7/download?disposition=inline:true");
		expect(
			screen.getByTestId("dialog-content").className.split(/\s+/),
		).toContain("h-[90vh]");
	});

	it("opens the selected mode from the chooser without persisting the choice", async () => {
		renderDialog();

		await chooseOpenMethod("files:mode_code");
		expect(
			await screen.findByText("code:/files/7/download?disposition=inline:true"),
		).toBeInTheDocument();
		expect(screen.queryByText("files:mode_markdown")).not.toBeInTheDocument();
	});

	it("does not render audio inside the file preview dialog", async () => {
		mockState.profile = {
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
		};

		renderDialog({
			file: {
				id: 8,
				mime_type: "audio/mpeg",
				name: "track.mp3",
				size: 4096,
			} as never,
			openMode: "picker",
		});

		expect(
			screen.queryByRole("heading", { name: "files:choose_open_method" }),
		).not.toBeInTheDocument();
		expect(await screen.findByText("preview-unavailable")).toBeInTheDocument();
		expect(screen.queryByText(/music:/)).not.toBeInTheDocument();
		expect(mockState.getMediaMetadata).not.toHaveBeenCalled();
	});

	it("always shows the more-open-methods button while the chooser is visible", () => {
		renderDialog();

		expect(screen.getByText("files:more_open_methods")).toBeInTheDocument();
	});

	it("opens the only visible app directly and still allows manual method switching", async () => {
		mockState.profile = {
			category: "markdown",
			defaultMode: "builtin.code",
			isBlobPreview: false,
			isEditableText: true,
			isTextBased: true,
			options: [
				{
					icon: "TextT",
					key: "builtin.code",
					labelKey: "mode_code",
					mode: "code",
				},
			],
			allOptions: [
				{
					icon: "TextT",
					key: "builtin.code",
					labelKey: "mode_code",
					mode: "code",
				},
				{
					icon: "MarkdownLogo",
					key: "builtin.markdown",
					labelKey: "mode_markdown",
					mode: "markdown",
				},
			],
		};

		renderDialog();

		expect(
			screen.queryByRole("heading", { name: "files:choose_open_method" }),
		).not.toBeInTheDocument();
		expect(
			await screen.findByText("code:/files/7/download?disposition=inline:true"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "files:choose_open_method" }),
		);
		expect(
			await screen.findByRole("heading", { name: "files:choose_open_method" }),
		).toBeInTheDocument();
		expect(screen.getByText("files:more_open_methods")).toBeInTheDocument();
		expect(screen.queryByText("files:mode_markdown")).not.toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: /files:more_open_methods/i }),
		);
		expect(
			screen.queryByText("files:more_open_methods"),
		).not.toBeInTheDocument();
		expect(screen.getByText("files:mode_markdown")).toBeInTheDocument();
		await chooseOpenMethod("files:mode_markdown");
		expect(
			await screen.findByText("markdown:/files/7/download?disposition=inline"),
		).toBeInTheDocument();
	});

	it("guards closing when dirty and only closes after discard confirmation", async () => {
		const { onClose } = renderDialog();

		await chooseOpenMethod("files:mode_code");
		await screen.findByText("code:/files/7/download?disposition=inline:true");
		fireEvent.click(screen.getByRole("button", { name: "mark-dirty" }));
		fireEvent.click(screen.getByRole("button", { name: "core:close" }));

		expect(screen.getByText("unsaved-guard")).toBeInTheDocument();
		expect(onClose).not.toHaveBeenCalled();

		fireEvent.click(screen.getByRole("button", { name: "discard-changes" }));

		await waitFor(() => {
			expect(onClose).toHaveBeenCalledTimes(1);
		});
	});

	it("expands the preview dialog to fill the window without leaving the page", async () => {
		renderDialog();

		await chooseOpenMethod("files:mode_code");
		expect(
			screen.getByTestId("dialog-content").className.split(/\s+/),
		).not.toContain("top-0");

		fireEvent.click(
			screen.getByRole("button", {
				name: "files:preview_enter_fullscreen",
			}),
		);

		expect(
			screen.getByRole("button", { name: "files:preview_exit_fullscreen" }),
		).toBeInTheDocument();
		expect(screen.getByTestId("dialog-content").className.split(/\s+/)).toEqual(
			expect.arrayContaining([
				"top-0",
				"left-0",
				"h-screen",
				"w-screen",
				"rounded-none",
			]),
		);

		fireEvent.click(
			screen.getByRole("button", {
				name: "files:preview_exit_fullscreen",
			}),
		);

		expect(
			screen.getByRole("button", { name: "files:preview_enter_fullscreen" }),
		).toBeInTheDocument();
		expect(
			screen.getByTestId("dialog-content").className.split(/\s+/),
		).not.toContain("top-0");
	});

	it("falls back to preview unavailable when the profile has no active mode", async () => {
		mockState.profile = {
			category: "unknown",
			defaultMode: null,
			isBlobPreview: false,
			isEditableText: false,
			isTextBased: false,
			options: [],
		};

		renderDialog();

		expect(await screen.findByText("preview-unavailable")).toBeInTheDocument();
	});

	it("opens the configured custom video browser from the chooser", async () => {
		mockState.profile = {
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
		};
		mockState.videoBrowserOption = {
			config: {
				allowed_origins: ["https://videos.example.com"],
				mode: "iframe",
				url_template: "https://videos.example.com/watch?id={{fileId}}",
			},
			icon: "Globe",
			key: "videoBrowser",
			label: "Jellyfin",
			labelKey: "open_with_custom_video_browser",
			mode: "url_template",
		};

		renderDialog({
			file: {
				id: 7,
				mime_type: "video/mp4",
				name: "clip.mp4",
				size: 2048,
			} as never,
		});

		await chooseOpenMethod("Jellyfin");

		await screen.findByText(
			"url-template:Jellyfin:/files/7/download:https://videos.example.com/watch?id={{fileId}}:false",
		);
	});

	it("lets plain video previews size the dialog from their content", async () => {
		mockState.profile = {
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
		};

		renderDialog({
			file: {
				id: 7,
				mime_type: "video/mp4",
				name: "clip.mp4",
				size: 2048,
			} as never,
			resources: testResources(7, {
				actions: {
					createMediaStreamSession: async () => ({
						expires_at: "2026-01-01T00:00:00Z",
						path: "/api/v1/s/share/stream/session/clip.mp4",
					}),
				},
			}),
		});

		await screen.findByText("video:/files/7/download?disposition=inline");
		expect(
			screen.getByText("video:/files/7/download?disposition=inline"),
		).toHaveAttribute("data-has-media-stream-link-factory", "true");
		const classes = screen.getByTestId("dialog-content").className.split(/\s+/);
		expect(classes).toContain("max-h-[90vh]");
		expect(classes).not.toContain("h-[90vh]");
	});

	it("does not route builtin audio previews through the dialog media preview", async () => {
		mockState.profile = {
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
		};

		renderDialog({
			file: {
				id: 8,
				mime_type: "audio/mpeg",
				name: "track.mp3",
				size: 4096,
			} as never,
			resources: testResources(8, {
				actions: {
					createMediaStreamSession: async () => ({
						expires_at: "2026-01-01T00:00:00Z",
						path: "/api/v1/s/share/stream/session/track.mp3",
					}),
				},
			}),
		});

		expect(await screen.findByText("preview-unavailable")).toBeInTheDocument();
		expect(screen.queryByText(/music:/)).not.toBeInTheDocument();
		expect(mockState.getMediaMetadata).not.toHaveBeenCalled();
		expect(
			screen.getByTestId("dialog-content").className.split(/\s+/),
		).not.toContain("h-[90vh]");
	});

	it("loads the preview app registry when the store is still cold", async () => {
		mockState.previewAppStore.isLoaded = false;

		renderDialog();

		await waitFor(() => {
			expect(mockState.previewAppStore.load).toHaveBeenCalledTimes(1);
		});
	});

	it("opens image previews in the fullscreen media viewer with a dark overlay", async () => {
		mockState.profile = {
			category: "image",
			defaultMode: "builtin.image",
			isBlobPreview: true,
			isEditableText: false,
			isTextBased: false,
			options: [
				{
					icon: "Eye",
					key: "builtin.image",
					labelKey: "open_with_image",
					mode: "image",
				},
			],
		};

		renderDialog({
			file: {
				id: 7,
				mime_type: "image/png",
				name: "tall-image.png",
				size: 2048,
			} as never,
		});

		expect(
			await screen.findByRole("img", { name: "tall-image.png" }),
		).toHaveAttribute("src", "blob:/files/7/download?disposition=inline");
		expect(screen.getByRole("img", { name: "tall-image.png" })).toHaveAttribute(
			"data-fill-container",
			"true",
		);
		expect(screen.getByTestId("dialog-content").className.split(/\s+/)).toEqual(
			expect.arrayContaining([
				"group/image-preview",
				"top-0",
				"left-0",
				"h-screen",
				"w-screen",
				"rounded-none",
			]),
		);
		expect(screen.getByTestId("dialog-overlay").className).toContain(
			"bg-zinc-950/88",
		);
	});

	it("resolves image preview content through the resource resolver", async () => {
		mockState.profile = {
			category: "image",
			defaultMode: "builtin.image",
			isBlobPreview: true,
			isEditableText: false,
			isTextBased: false,
			options: [
				{
					icon: "Eye",
					key: "builtin.image",
					labelKey: "open_with_image",
					mode: "image",
				},
			],
		};
		renderDialog({
			file: {
				id: 7,
				mime_type: "image/png",
				name: "r2-image.png",
				size: 2048,
			} as never,
		});

		const image = await screen.findByRole("img", { name: "r2-image.png" });
		await waitFor(() => {
			expect(image).toHaveAttribute(
				"data-preview-resource",
				"/files/7/download?disposition=inline",
			);
			expect(screen.getByRole("img", { name: "r2-image.png" })).toHaveAttribute(
				"data-cache-key",
				"/files/7/download",
			);
			expect(screen.getByRole("img", { name: "r2-image.png" })).toHaveAttribute(
				"data-preview-etag",
				"",
			);
		});
		expect(mockState.resolveResourceHandle).toHaveBeenCalledWith(7, {
			delivery_mode: "blob_url",
			purpose: "preview",
			representation: "original",
		});
	});

	it("keeps image previews fullscreen without a windowed restore control", async () => {
		mockState.profile = {
			category: "image",
			defaultMode: "builtin.image",
			isBlobPreview: true,
			isEditableText: false,
			isTextBased: false,
			options: [
				{
					icon: "Eye",
					key: "builtin.image",
					labelKey: "open_with_image",
					mode: "image",
				},
			],
		};

		renderDialog({
			file: {
				id: 7,
				mime_type: "image/png",
				name: "wide-image.png",
				size: 2048,
			} as never,
		});

		expect(
			await screen.findByRole("img", { name: "wide-image.png" }),
		).toHaveAttribute("data-fill-container", "true");

		expect(
			screen.queryByRole("button", { name: "files:preview_exit_fullscreen" }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "files:preview_enter_fullscreen" }),
		).not.toBeInTheDocument();
		expect(screen.getByRole("img", { name: "wide-image.png" })).toHaveAttribute(
			"data-fill-container",
			"true",
		);
		expect(screen.getByTestId("dialog-content").className.split(/\s+/)).toEqual(
			expect.arrayContaining([
				"top-0",
				"left-0",
				"h-screen",
				"w-screen",
				"rounded-none",
			]),
		);
	});

	it("auto-opens hybrid svg previews directly and still allows switching modes", async () => {
		mockState.profile = {
			category: "image",
			defaultMode: "builtin.image",
			isBlobPreview: true,
			isEditableText: true,
			isTextBased: true,
			options: [
				{
					icon: "Eye",
					key: "builtin.image",
					labelKey: "open_with_image",
					mode: "image",
				},
				{
					icon: "TextT",
					key: "builtin.code",
					labelKey: "mode_code",
					mode: "code",
				},
			],
		};

		renderDialog({
			file: {
				id: 7,
				mime_type: "image/svg+xml",
				name: "logo.svg",
				size: 512,
			} as never,
		});

		expect(
			screen.queryByRole("heading", { name: "files:choose_open_method" }),
		).not.toBeInTheDocument();
		expect(
			await screen.findByRole("img", { name: "logo.svg" }),
		).toHaveAttribute("src", "blob:/files/7/download?disposition=inline");

		fireEvent.click(
			screen.getByRole("button", { name: "files:choose_open_method" }),
		);
		expect(
			await screen.findByRole("heading", { name: "files:choose_open_method" }),
		).toBeInTheDocument();
		await chooseOpenMethod("files:mode_code");
		expect(
			await screen.findByText("code:/files/7/download?disposition=inline:true"),
		).toBeInTheDocument();
	});

	it("renders iframe url-template previews in the fixed-height workspace", async () => {
		mockState.profile = {
			category: "document",
			defaultMode: "builtin.office_microsoft",
			isBlobPreview: false,
			isEditableText: false,
			isTextBased: false,
			options: [
				{
					config: {
						allowed_origins: ["https://view.officeapps.live.com"],
						mode: "iframe",
						url_template:
							"https://view.officeapps.live.com/op/embed.aspx?src={{file_preview_url}}",
					},
					icon: "Globe",
					key: "builtin.office_microsoft",
					labelKey: "open_with_office_microsoft",
					mode: "url_template",
				},
			],
		};

		renderDialog({
			file: {
				id: 7,
				mime_type:
					"application/vnd.openxmlformats-officedocument.wordprocessingml.document",
				name: "report.docx",
				size: 2048,
			} as never,
			resources: testResources(7, {
				actions: {
					createExternalPreviewLink: vi.fn(async () => ({
						etag: '"etag-report"',
						expires_at: "2026-04-08T12:00:00Z",
						max_uses: 5,
						path: "/pv/token/report.docx",
					})),
				},
			}),
		});

		expect(
			await screen.findByText(
				"url-template:files:open_with_office_microsoft:/files/7/download:https://view.officeapps.live.com/op/embed.aspx?src={{file_preview_url}}:true",
			),
		).toBeInTheDocument();
		expect(
			screen.getByTestId("dialog-content").className.split(/\s+/),
		).toContain("h-[90vh]");
	});

	it("passes the configured table delimiter strategy through to the preview", async () => {
		mockState.profile = {
			category: "csv",
			defaultMode: "builtin.table",
			isBlobPreview: false,
			isEditableText: true,
			isTextBased: true,
			options: [
				{
					config: {
						delimiter: "auto",
					},
					icon: "Table",
					key: "builtin.table",
					labelKey: "open_with_table",
					mode: "table",
				},
			],
		};

		renderDialog({
			file: {
				id: 7,
				mime_type: "text/csv",
				name: "people.csv",
				size: 512,
			} as never,
		});

		expect(
			await screen.findByText(
				"table:auto:/files/7/download?disposition=inline",
			),
		).toBeInTheDocument();
	});

	it("passes url-template previews through even without a preview link factory", async () => {
		mockState.profile = {
			category: "document",
			defaultMode: "builtin.office_microsoft",
			isBlobPreview: false,
			isEditableText: false,
			isTextBased: false,
			options: [
				{
					config: {
						allowed_origins: ["https://view.officeapps.live.com"],
						mode: "iframe",
						url_template:
							"https://view.officeapps.live.com/op/embed.aspx?src={{file_preview_url}}",
					},
					icon: "Globe",
					key: "builtin.office_microsoft",
					labelKey: "open_with_office_microsoft",
					mode: "url_template",
				},
			],
		};

		renderDialog({
			file: {
				id: 7,
				mime_type:
					"application/vnd.openxmlformats-officedocument.wordprocessingml.document",
				name: "report.docx",
				size: 2048,
			} as never,
		});

		expect(
			await screen.findByText(
				"url-template:files:open_with_office_microsoft:/files/7/download:https://view.officeapps.live.com/op/embed.aspx?src={{file_preview_url}}:false",
			),
		).toBeInTheDocument();
	});

	it("renders wopi previews in the fixed-height workspace when a session factory is available", async () => {
		mockState.profile = {
			category: "document",
			defaultMode: "custom.onlyoffice",
			isBlobPreview: false,
			isEditableText: false,
			isTextBased: false,
			options: [
				{
					config: {
						mode: "iframe",
						provider: "wopi",
					},
					icon: "Globe",
					key: "custom.onlyoffice",
					labels: {
						zh: "OnlyOffice",
					},
					labelKey: "",
					mode: "wopi",
				},
			],
		};

		renderDialog({
			file: {
				id: 7,
				mime_type:
					"application/vnd.openxmlformats-officedocument.wordprocessingml.document",
				name: "report.docx",
				size: 2048,
			} as never,
			resources: testResources(7, {
				actions: {
					launchWopiSession: vi.fn(async () => ({
						access_token: "token-1",
						access_token_ttl: 600,
						action_url: "https://office.example.com/wopi/files/7",
					})),
				},
			}),
		});

		expect(
			await screen.findByText("wopi:OnlyOffice:iframe:true"),
		).toBeInTheDocument();
		expect(
			screen.getByTestId("dialog-content").className.split(/\s+/),
		).toContain("h-[90vh]");
	});

	it("hides wopi open methods when no session factory is available", () => {
		mockState.profile = {
			category: "document",
			defaultMode: "custom.onlyoffice",
			isBlobPreview: false,
			isEditableText: false,
			isTextBased: false,
			options: [
				{
					config: {
						mode: "iframe",
						provider: "wopi",
					},
					icon: "Globe",
					key: "custom.onlyoffice",
					labels: {
						zh: "OnlyOffice",
					},
					labelKey: "",
					mode: "wopi",
				},
			],
		};

		renderDialog({
			file: {
				id: 7,
				mime_type:
					"application/vnd.openxmlformats-officedocument.wordprocessingml.document",
				name: "report.docx",
				size: 2048,
			} as never,
		});

		expect(screen.queryByText("OnlyOffice")).not.toBeInTheDocument();
		expect(screen.getByText("preview-unavailable")).toBeInTheDocument();
	});
});
