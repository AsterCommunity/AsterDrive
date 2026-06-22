import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FilePreviewBody } from "@/components/files/preview/FilePreviewBody";
import type { OpenWithOption } from "@/components/files/preview/types";

const mockState = vi.hoisted(() => ({
	blobImagePreview: vi.fn(),
	musicPreview: vi.fn(),
	videoPreview: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/files/preview/BlobImagePreview", () => ({
	BlobImagePreview: (props: unknown) => {
		mockState.blobImagePreview(props);
		return <div data-testid="blob-image-preview" />;
	},
}));

vi.mock("@/components/files/preview/MusicPreview", () => ({
	MusicPreview: (props: unknown) => {
		mockState.musicPreview(props);
		return <div data-testid="music-preview" />;
	},
}));

vi.mock("@/components/files/preview/VideoPreview", () => ({
	VideoPreview: (props: unknown) => {
		mockState.videoPreview(props);
		return <div data-testid="video-preview" />;
	},
}));

vi.mock("@/components/files/preview/UrlTemplatePreview", () => ({
	UrlTemplatePreview: () => <div data-testid="url-template-preview" />,
}));

vi.mock("@/components/files/preview/WopiPreview", () => ({
	WopiPreview: () => <div data-testid="wopi-preview" />,
}));

function option(mode: OpenWithOption["mode"]): OpenWithOption {
	return {
		icon: "File",
		key: `builtin.${mode}`,
		labelKey: `open_with_${mode}`,
		mode,
	};
}

function renderBody(overrides: Partial<Parameters<typeof FilePreviewBody>[0]>) {
	return render(
		<FilePreviewBody
			file={{
				id: 7,
				name: "preview.bin",
				mime_type: "application/octet-stream",
			}}
			activeOption={option("pdf")}
			profile={{
				category: "pdf",
				defaultMode: "builtin.pdf",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [option("pdf")],
			}}
			previewAppsLoaded
			contentPreviewPath="/files/7/download"
			downloadPath="/files/7/download"
			getOptionLabel={(item) => item.labelKey}
			onDirtyChange={vi.fn()}
			editable
			formattedCategory="json"
			isExpanded={false}
			{...overrides}
		/>,
	);
}

describe("FilePreviewBody", () => {
	it("shows loading for pdf previews while the content preview path is resolving", () => {
		renderBody({
			activeOption: option("pdf"),
			contentPreviewPath: null,
		});

		expect(screen.getByText("files:loading_preview")).toBeInTheDocument();
	});

	it("shows loading for markdown previews while the content preview path is resolving", () => {
		renderBody({
			activeOption: option("markdown"),
			contentPreviewPath: null,
			profile: {
				category: "markdown",
				defaultMode: "builtin.markdown",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: true,
				options: [option("markdown")],
			},
		});

		expect(screen.getByText("files:loading_preview")).toBeInTheDocument();
	});

	it.each([
		["table", "csv"],
		["formatted", "json"],
		["code", "text"],
	] as const)("shows loading for %s previews while the content preview path is resolving", (mode, category) => {
		renderBody({
			activeOption: option(mode),
			contentPreviewPath: null,
			formattedCategory: category === "json" ? "json" : "xml",
			profile: {
				category,
				defaultMode: `builtin.${mode}`,
				isBlobPreview: true,
				isEditableText: mode === "code",
				isTextBased: true,
				options: [option(mode)],
			},
		});

		expect(screen.getByText("files:loading_preview")).toBeInTheDocument();
	});

	it("passes a nullable content path through image previews", () => {
		renderBody({
			activeOption: option("image"),
			contentPreviewPath: null,
			imagePreviewPath: "/files/7/image-preview",
			isExpanded: true,
			profile: {
				category: "image",
				defaultMode: "builtin.image",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [option("image")],
			},
		});

		expect(screen.getByTestId("blob-image-preview")).toBeInTheDocument();
		expect(mockState.blobImagePreview).toHaveBeenCalledWith(
			expect.objectContaining({
				fallbackPath: "/files/7/image-preview",
				fillContainer: true,
				path: null,
			}),
		);
	});

	it("passes a nullable content path through audio previews", () => {
		const loadMusicBackendMetadata = vi.fn();
		const mediaStreamLinkFactory = vi.fn();

		renderBody({
			activeOption: option("audio"),
			contentPreviewPath: null,
			loadMusicBackendMetadata,
			mediaStreamLinkFactory,
			thumbnailPath: "/files/7/thumbnail",
			profile: {
				category: "audio",
				defaultMode: "builtin.audio",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [option("audio")],
			},
		});

		expect(screen.getByTestId("music-preview")).toBeInTheDocument();
		expect(mockState.musicPreview).toHaveBeenCalledWith(
			expect.objectContaining({
				loadBackendMetadata: loadMusicBackendMetadata,
				mediaStreamLinkFactory,
				path: null,
				thumbnailPath: "/files/7/thumbnail",
			}),
		);
	});

	it("passes a nullable content path through video previews", () => {
		const mediaStreamLinkFactory = vi.fn();

		renderBody({
			activeOption: option("video"),
			contentPreviewPath: null,
			mediaStreamLinkFactory,
			profile: {
				category: "video",
				defaultMode: "builtin.video",
				isBlobPreview: true,
				isEditableText: false,
				isTextBased: false,
				options: [option("video")],
			},
		});

		expect(screen.getByTestId("video-preview")).toBeInTheDocument();
		expect(mockState.videoPreview).toHaveBeenCalledWith(
			expect.objectContaining({
				mediaStreamLinkFactory,
				path: null,
			}),
		);
	});

	it("renders url template previews without requiring a resolved content path", () => {
		renderBody({
			activeOption: option("url_template"),
			contentPreviewPath: null,
			profile: {
				category: "document",
				defaultMode: "builtin.url_template",
				isBlobPreview: false,
				isEditableText: false,
				isTextBased: false,
				options: [option("url_template")],
			},
		});

		expect(screen.getByTestId("url-template-preview")).toBeInTheDocument();
	});

	it("shows unavailable for WOPI previews without a session resource", () => {
		renderBody({
			activeOption: option("wopi"),
			contentPreviewPath: null,
			profile: {
				category: "document",
				defaultMode: "builtin.wopi",
				isBlobPreview: false,
				isEditableText: false,
				isTextBased: false,
				options: [option("wopi")],
			},
			wopiSessionResource: null,
		});

		expect(screen.getByText("preview_not_available")).toBeInTheDocument();
	});
});
