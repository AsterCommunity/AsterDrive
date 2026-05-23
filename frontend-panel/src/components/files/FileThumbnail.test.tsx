import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileThumbnail } from "@/components/files/FileThumbnail";

const mockState = vi.hoisted(() => ({
	enteredViewportRef: vi.fn(),
	thumbnailSupportStore: {
		config: {
			version: 1,
			extensions: ["png", "heic"],
		},
		invalidate: vi.fn(),
		isLoaded: true,
		load: vi.fn(),
	},
	useEnteredViewport: vi.fn(),
	useBlobUrl: vi.fn(),
	thumbnailPath: vi.fn((id: number) => `/thumb/${id}`),
}));

vi.mock("@/hooks/useEnteredViewport", () => ({
	useEnteredViewport: (...args: unknown[]) =>
		mockState.useEnteredViewport(...args),
}));

vi.mock("@/hooks/useBlobUrl", () => ({
	useBlobUrl: (...args: unknown[]) => mockState.useBlobUrl(...args),
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		thumbnailPath: mockState.thumbnailPath,
	},
}));

vi.mock("@/stores/thumbnailSupportStore", () => ({
	useThumbnailSupportStore: (
		selector: (state: typeof mockState.thumbnailSupportStore) => unknown,
	) => selector(mockState.thumbnailSupportStore),
}));

vi.mock("@/components/files/FileTypeIcon", () => ({
	FileTypeIcon: ({
		mimeType,
		fileName,
		className,
	}: {
		mimeType: string;
		fileName?: string;
		className?: string;
	}) => (
		<span
			data-testid="file-type-icon"
			data-mime-type={mimeType}
			data-file-name={fileName}
			className={className}
		/>
	),
}));

const pngFile = {
	id: 7,
	name: "photo.png",
	mime_type: "image/png",
};

const heicFile = {
	id: 8,
	name: "capture.heic",
	mime_type: "application/octet-stream",
};

describe("FileThumbnail", () => {
	beforeEach(() => {
		mockState.enteredViewportRef.mockReset();
		mockState.thumbnailPath.mockClear();
		mockState.thumbnailSupportStore.config = {
			version: 1,
			extensions: ["png", "heic"],
		};
		mockState.thumbnailSupportStore.isLoaded = true;
		mockState.thumbnailSupportStore.load.mockReset();
		mockState.useEnteredViewport.mockReset();
		mockState.useBlobUrl.mockReset();
		mockState.useEnteredViewport.mockReturnValue({
			ref: mockState.enteredViewportRef,
			hasEnteredViewport: true,
		});
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: false,
			loading: false,
		});
	});

	it("uses the generated thumbnail path for supported extensions", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: "blob:1",
			error: false,
			loading: false,
		});

		const { container } = render(<FileThumbnail file={pngFile} />);

		expect(mockState.thumbnailPath).toHaveBeenCalledWith(7);
		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/thumb/7", {
			lane: "thumbnail",
		});
		expect(container.querySelector("img")).toHaveAttribute("src", "blob:1");
	});

	it("uses the provided thumbnail path override", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: "blob:override",
			error: false,
			loading: false,
		});

		render(<FileThumbnail file={pngFile} thumbnailPath="/custom-thumb" />);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/custom-thumb", {
			lane: "thumbnail",
		});
	});

	it("waits until the thumbnail enters the viewport before queueing the blob fetch", () => {
		mockState.useEnteredViewport.mockReturnValue({
			ref: mockState.enteredViewportRef,
			hasEnteredViewport: false,
		});

		render(<FileThumbnail file={pngFile} />);

		expect(mockState.thumbnailPath).not.toHaveBeenCalled();
		expect(mockState.useBlobUrl).toHaveBeenCalledWith(null, {
			lane: "thumbnail",
		});
		expect(screen.getByTestId("file-type-icon")).toBeInTheDocument();
	});

	it("loads thumbnail support on demand before deciding whether to fetch", async () => {
		mockState.thumbnailSupportStore.config = null;
		mockState.thumbnailSupportStore.isLoaded = false;

		render(<FileThumbnail file={pngFile} />);

		await waitFor(() => {
			expect(mockState.thumbnailSupportStore.load).toHaveBeenCalledTimes(1);
		});
		expect(mockState.useBlobUrl).toHaveBeenCalledWith(null, {
			lane: "thumbnail",
		});
		expect(screen.getByTestId("file-type-icon")).toBeInTheDocument();
	});

	it("falls back to the file type icon when the suffix is not supported", () => {
		render(
			<FileThumbnail
				file={{ id: 9, name: "report.pdf", mime_type: "application/pdf" }}
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith(null, {
			lane: "thumbnail",
		});
		expect(screen.getByTestId("file-type-icon")).toHaveAttribute(
			"data-mime-type",
			"application/pdf",
		);
	});

	it("matches supported suffixes even when the MIME type is not image/*", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: "blob:heic",
			error: false,
			loading: false,
		});

		const { container } = render(<FileThumbnail file={heicFile} />);

		expect(mockState.thumbnailPath).toHaveBeenCalledWith(8);
		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/thumb/8", {
			lane: "thumbnail",
		});
		expect(container.querySelector("img")).toHaveAttribute("src", "blob:heic");
	});

	it("falls back to the icon when blob loading fails or is absent", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: true,
			loading: false,
		});

		const { rerender } = render(<FileThumbnail file={pngFile} size="lg" />);
		expect(screen.getByTestId("file-type-icon")).toHaveClass("size-12");

		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: false,
			loading: false,
		});
		rerender(<FileThumbnail file={pngFile} />);

		expect(screen.getByTestId("file-type-icon")).toHaveClass("size-4");
	});

	it("renders medium thumbnails across loading, fallback, and success states", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: false,
			loading: true,
		});

		const { container, rerender } = render(
			<FileThumbnail file={pngFile} size="md" />,
		);

		expect(screen.getByTestId("thumbnail-loading")).toHaveClass("size-4");

		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: true,
			loading: false,
		});
		rerender(<FileThumbnail file={pngFile} size="md" />);

		expect(screen.getByTestId("file-type-icon")).toHaveClass("size-5");

		mockState.useBlobUrl.mockReturnValue({
			blobUrl: "blob:3",
			error: false,
			loading: false,
		});
		rerender(<FileThumbnail file={pngFile} size="md" />);

		expect(container.querySelector("img")).toHaveAttribute("src", "blob:3");
		expect(container.querySelector("img")).toHaveClass(
			"h-full",
			"w-full",
			"object-cover",
		);
	});

	it("renders a large image preview when a blob URL is available", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: "blob:2",
			error: false,
			loading: false,
		});

		const { container } = render(<FileThumbnail file={pngFile} size="lg" />);

		expect(container.querySelector("img")).toHaveAttribute("src", "blob:2");
		expect(container.querySelector("img")).toHaveClass(
			"h-full",
			"w-auto",
			"shrink-0",
			"max-w-none",
		);
	});

	it("shows a large spinner while a large thumbnail is still loading", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: false,
			loading: true,
		});

		render(<FileThumbnail file={pngFile} size="lg" />);

		expect(screen.getByTestId("thumbnail-loading")).toHaveClass("size-5");
		expect(screen.queryByTestId("file-type-icon")).not.toBeInTheDocument();
	});

	it("shows a spinner while a thumbnail is still loading", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: false,
			loading: true,
		});

		render(<FileThumbnail file={pngFile} />);

		expect(screen.getByTestId("thumbnail-loading")).toHaveClass("animate-spin");
		expect(screen.queryByTestId("file-type-icon")).not.toBeInTheDocument();
	});
});
