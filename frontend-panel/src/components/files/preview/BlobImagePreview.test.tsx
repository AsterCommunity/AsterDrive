import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BlobImagePreview } from "@/components/files/preview/BlobImagePreview";

const mockState = vi.hoisted(() => ({
	retry: vi.fn(),
	useBlobUrl: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/hooks/useBlobUrl", () => ({
	useBlobUrl: (...args: unknown[]) => mockState.useBlobUrl(...args),
}));

const file = { name: "preview.png", mime_type: "image/png" };

describe("BlobImagePreview", () => {
	beforeEach(() => {
		mockState.retry.mockReset();
		mockState.useBlobUrl.mockReset();
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: "blob:image",
			error: false,
			loading: false,
			retry: mockState.retry,
		});
	});

	it("shows a loading message while the blob is being fetched", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: false,
			loading: true,
			retry: mockState.retry,
		});

		render(<BlobImagePreview file={file} path="/files/1" />);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/1");
		expect(screen.getByText("loading_preview")).toBeInTheDocument();
	});

	it("renders the retry state when loading fails", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: true,
			loading: false,
			retry: mockState.retry,
		});

		render(<BlobImagePreview file={file} path="/files/1" />);

		fireEvent.click(screen.getByRole("button", { name: "preview_retry" }));

		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
		expect(mockState.retry).toHaveBeenCalledTimes(1);
	});

	it("falls back to the error state when no blob url is available", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: false,
			loading: false,
			retry: mockState.retry,
		});

		render(<BlobImagePreview file={file} path="/files/1" />);

		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
	});

	it("renders an image preview with the file name as alt text", () => {
		render(<BlobImagePreview file={file} path="/files/1" />);

		const image = screen.getByRole("img", { name: "preview.png" });

		expect(image).toHaveAttribute("src", "blob:image");
		expect(image).toHaveClass(
			"block",
			"max-h-[min(70vh,48rem)]",
			"max-w-full",
			"object-contain",
		);
		expect(image.parentElement).toHaveClass("mx-auto", "w-fit", "p-4");
	});

	it("gives svg image previews an explicit layout width", () => {
		render(
			<BlobImagePreview
				file={{ name: "logo.svg", mime_type: "image/svg+xml" }}
				path="/files/svg"
			/>,
		);

		const image = screen.getByRole("img", { name: "logo.svg" });

		expect(image).toHaveClass(
			"h-auto",
			"w-full",
			"max-h-[min(70vh,48rem)]",
			"max-w-[min(70vw,48rem)]",
			"object-contain",
		);
		expect(image.parentElement).toHaveClass("w-full", "p-4");
		expect(image.parentElement).not.toHaveClass("w-fit");
	});

	it("lets expanded image previews fill the available preview surface", () => {
		render(
			<BlobImagePreview file={file} fillContainer path="/files/expanded" />,
		);

		const image = screen.getByRole("img", { name: "preview.png" });

		expect(image).toHaveClass("block", "h-full", "w-full", "object-contain");
		expect(image).not.toHaveClass("max-h-[min(70vh,48rem)]");
		expect(image.parentElement).toHaveClass("h-full", "w-full", "p-4");
		expect(image.parentElement).not.toHaveClass("w-fit");
	});

	it("uses the backend preview directly for HEIC images", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: "blob:fallback",
			error: false,
			loading: false,
			retry: mockState.retry,
		});

		render(
			<BlobImagePreview
				file={{ name: "photo.heic", mime_type: "image/heic" }}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/1/image-preview");
		expect(screen.getByRole("img", { name: "photo.heic" })).toHaveAttribute(
			"src",
			"blob:fallback",
		);
	});

	it("uses the backend preview directly for HEIF files detected by extension", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: "blob:fallback",
			error: false,
			loading: false,
			retry: mockState.retry,
		});

		render(
			<BlobImagePreview
				file={{ name: "camera.HEIF", mime_type: "application/octet-stream" }}
				path="/files/2/download"
				fallbackPath="/files/2/image-preview"
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/2/image-preview");
	});

	it("does not switch ordinary images to the backend preview on render errors", () => {
		render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		fireEvent.error(screen.getByRole("img", { name: "preview.png" }));

		expect(mockState.useBlobUrl).toHaveBeenLastCalledWith("/files/1/download");
		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
	});

	it("shows the retry state when the HEIC backend preview cannot render", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: "blob:fallback",
			error: false,
			loading: false,
			retry: mockState.retry,
		});

		render(
			<BlobImagePreview
				file={{ name: "photo.heic", mime_type: "image/heic" }}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		fireEvent.error(screen.getByRole("img", { name: "photo.heic" }));

		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
	});
});
