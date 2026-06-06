import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BlobImagePreview } from "@/components/files/preview/BlobImagePreview";

const mockState = vi.hoisted(() => ({
	imagePreviewPreference: "original_first",
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

vi.mock("@/stores/frontendConfigStore", () => ({
	useFrontendConfigStore: (
		selector: (state: { imagePreviewPreference: string }) => unknown,
	) =>
		selector({
			imagePreviewPreference: mockState.imagePreviewPreference,
		}),
}));

const file = { name: "preview.png", mime_type: "image/png" };

describe("BlobImagePreview", () => {
	beforeEach(() => {
		mockState.imagePreviewPreference = "original_first";
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

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/1", {
			lane: "default",
		});
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

	it("uses the original image first when the public preference is original_first", () => {
		render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/1/download", {
			lane: "default",
		});
		expect(
			screen.queryByRole("button", { name: "preview_show_original" }),
		).not.toBeInTheDocument();
	});

	it("uses the backend preview first when an original-first image is not browser-renderable", () => {
		mockState.imagePreviewPreference = "original_first";
		mockState.useBlobUrl.mockImplementation((path: string | null) => ({
			blobUrl: path?.includes("image-preview")
				? "blob:medium"
				: "blob:original",
			error: false,
			loading: false,
			retry: mockState.retry,
		}));

		render(
			<BlobImagePreview
				file={{ name: "capture.nef", mime_type: "image/x-nikon-nef" }}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		expect(screen.getByRole("img", { name: "capture.nef" })).toHaveAttribute(
			"src",
			"blob:medium",
		);
		expect(mockState.useBlobUrl).toHaveBeenCalledWith(
			"/files/1/image-preview",
			{ lane: "preview" },
		);
		expect(mockState.useBlobUrl).not.toHaveBeenCalledWith("/files/1/download", {
			lane: "default",
		});
		expect(
			screen.queryByRole("button", { name: "preview_show_original" }),
		).not.toBeInTheDocument();
	});

	it("keeps avif on the backend preview path instead of treating it as browser-renderable", () => {
		mockState.imagePreviewPreference = "original_first";
		mockState.useBlobUrl.mockImplementation((path: string | null) => ({
			blobUrl: path?.includes("image-preview")
				? "blob:medium"
				: "blob:original",
			error: false,
			loading: false,
			retry: mockState.retry,
		}));

		render(
			<BlobImagePreview
				file={{ name: "modern.avif", mime_type: "image/avif" }}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		expect(screen.getByRole("img", { name: "modern.avif" })).toHaveAttribute(
			"src",
			"blob:medium",
		);
		expect(mockState.useBlobUrl).toHaveBeenCalledWith(
			"/files/1/image-preview",
			{ lane: "preview" },
		);
		expect(mockState.useBlobUrl).not.toHaveBeenCalledWith("/files/1/download", {
			lane: "default",
		});
		expect(
			screen.queryByRole("button", { name: "preview_show_original" }),
		).not.toBeInTheDocument();
	});

	it("uses the original path for non-renderable formats when no backend preview exists", () => {
		mockState.imagePreviewPreference = "original_first";

		render(
			<BlobImagePreview
				file={{ name: "capture.nef", mime_type: "image/x-nikon-nef" }}
				path="/files/1/download"
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/1/download", {
			lane: "default",
		});
		expect(
			screen.queryByRole("button", { name: "preview_show_original" }),
		).not.toBeInTheDocument();
	});

	it("falls back to the backend preview when an original-first renderable image fails to render", () => {
		mockState.imagePreviewPreference = "original_first";
		mockState.useBlobUrl.mockImplementation((path: string | null) => ({
			blobUrl: path?.includes("image-preview")
				? "blob:medium"
				: "blob:original",
			error: false,
			loading: false,
			retry: mockState.retry,
		}));

		render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
			"src",
			"blob:original",
		);

		fireEvent.error(screen.getByRole("img", { name: "preview.png" }));

		expect(mockState.useBlobUrl).toHaveBeenCalledWith(
			"/files/1/image-preview",
			{ lane: "preview" },
		);
		expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
			"src",
			"blob:medium",
		);
		expect(
			screen.queryByRole("button", { name: "preview_show_original" }),
		).not.toBeInTheDocument();
	});

	it("uses only the backend preview initially when the public preference is preview_first", () => {
		mockState.imagePreviewPreference = "preview_first";

		render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith(
			"/files/1/image-preview",
			{ lane: "preview" },
		);
		expect(mockState.useBlobUrl).not.toHaveBeenCalledWith("/files/1/download", {
			lane: "default",
		});
		expect(
			screen.getByRole("button", { name: "preview_show_original" }),
		).toBeInTheDocument();
	});

	it("uses the original path when preview_first has no backend preview path", () => {
		mockState.imagePreviewPreference = "preview_first";

		render(<BlobImagePreview file={file} path="/files/1/download" />);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/1/download", {
			lane: "default",
		});
		expect(
			screen.queryByRole("button", { name: "preview_show_original" }),
		).not.toBeInTheDocument();
	});

	it("resets inline original request state when the public preference changes", async () => {
		mockState.imagePreviewPreference = "preview_first";
		mockState.useBlobUrl.mockImplementation((path: string | null) => {
			if (path === null) {
				return {
					blobUrl: null,
					error: false,
					loading: false,
					retry: mockState.retry,
				};
			}
			return {
				blobUrl: path.includes("image-preview") ? "blob:medium" : null,
				error: false,
				loading: !path.includes("image-preview"),
				retry: mockState.retry,
			};
		});

		const { rerender } = render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", { name: "preview_show_original" }),
		);
		await waitFor(() =>
			expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/1/download", {
				lane: "default",
			}),
		);

		mockState.imagePreviewPreference = "original_first";
		mockState.useBlobUrl.mockClear();
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: "blob:original",
			error: false,
			loading: false,
			retry: mockState.retry,
		});
		rerender(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith(null, {
			lane: "default",
		});
		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/1/download", {
			lane: "default",
		});
		expect(
			screen.queryByRole("button", { name: "preview_show_original" }),
		).not.toBeInTheDocument();
	});

	it("keeps the backend preview visible while the requested original is loading", async () => {
		mockState.imagePreviewPreference = "preview_first";
		mockState.useBlobUrl.mockImplementation((path: string | null) => ({
			...(path === null
				? {
						blobUrl: null,
						error: false,
						loading: false,
						retry: mockState.retry,
					}
				: {
						blobUrl: path.includes("image-preview") ? "blob:medium" : null,
						error: false,
						loading: !path.includes("image-preview"),
						retry: mockState.retry,
					}),
		}));

		render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith(
			"/files/1/image-preview",
			{ lane: "preview" },
		);
		expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
			"src",
			"blob:medium",
		);

		fireEvent.click(
			screen.getByRole("button", { name: "preview_show_original" }),
		);
		await waitFor(() =>
			expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/1/download", {
				lane: "default",
			}),
		);
		expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
			"src",
			"blob:medium",
		);
		const originalButton = screen.getByRole("button", {
			name: "preview_show_original",
		});
		expect(originalButton).toBeDisabled();
		expect(originalButton.querySelector("svg")).toHaveClass("animate-spin");
	});

	it("switches to the original after the original blob is ready and hides the button after render", async () => {
		mockState.imagePreviewPreference = "preview_first";
		let originalReady = false;
		mockState.useBlobUrl.mockImplementation((path: string | null) => {
			if (path === null) {
				return {
					blobUrl: null,
					error: false,
					loading: false,
					retry: mockState.retry,
				};
			}
			return {
				blobUrl: path.includes("image-preview")
					? "blob:medium"
					: originalReady
						? "blob:original"
						: null,
				error: false,
				loading: path.includes("image-preview") ? false : !originalReady,
				retry: mockState.retry,
			};
		});

		const { rerender } = render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", { name: "preview_show_original" }),
		);
		expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
			"src",
			"blob:medium",
		);

		originalReady = true;
		rerender(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		await waitFor(() =>
			expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
				"src",
				"blob:original",
			),
		);
		expect(
			screen.getByRole("button", { name: "preview_show_original" }),
		).toBeDisabled();
		fireEvent.load(screen.getByRole("img", { name: "preview.png" }));
		expect(
			screen.queryByRole("button", { name: "preview_show_original" }),
		).not.toBeInTheDocument();
	});

	it("falls back to the backend preview when the downloaded original cannot render", async () => {
		mockState.imagePreviewPreference = "preview_first";
		let originalReady = false;
		mockState.useBlobUrl.mockImplementation((path: string | null) => {
			if (path === null) {
				return {
					blobUrl: null,
					error: false,
					loading: false,
					retry: mockState.retry,
				};
			}
			return {
				blobUrl: path.includes("image-preview")
					? "blob:medium"
					: originalReady
						? "blob:original"
						: null,
				error: false,
				loading: path.includes("image-preview") ? false : !originalReady,
				retry: mockState.retry,
			};
		});

		const { rerender } = render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", { name: "preview_show_original" }),
		);
		originalReady = true;
		rerender(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		await waitFor(() =>
			expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
				"src",
				"blob:original",
			),
		);
		fireEvent.error(screen.getByRole("img", { name: "preview.png" }));

		expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
			"src",
			"blob:medium",
		);
		expect(
			screen.getByRole("button", { name: "preview_show_original" }),
		).toBeEnabled();
	});

	it("uses a controlled source without starting the inline original request flow", () => {
		mockState.imagePreviewPreference = "preview_first";
		mockState.useBlobUrl.mockImplementation((path: string | null) => {
			if (path === null) {
				return {
					blobUrl: null,
					error: false,
					loading: false,
					retry: mockState.retry,
				};
			}
			return {
				blobUrl: path.includes("image-preview")
					? "blob:medium"
					: "blob:original",
				error: false,
				loading: false,
				retry: mockState.retry,
			};
		});

		render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
				source="backend_preview"
				showOriginalButtonPlacement="none"
			/>,
		);

		expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
			"src",
			"blob:medium",
		);
		expect(mockState.useBlobUrl).not.toHaveBeenCalledWith("/files/1/download", {
			lane: "default",
		});
	});

	it("keeps inline preview available when original loading fails", () => {
		mockState.imagePreviewPreference = "preview_first";
		mockState.useBlobUrl.mockImplementation((path: string | null) => {
			if (path === null) {
				return {
					blobUrl: null,
					error: false,
					loading: false,
					retry: mockState.retry,
				};
			}
			return {
				blobUrl: path.includes("image-preview") ? "blob:medium" : null,
				error: !path.includes("image-preview"),
				loading: false,
				retry: mockState.retry,
			};
		});

		render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", { name: "preview_show_original" }),
		);

		expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
			"src",
			"blob:medium",
		);
		expect(
			screen.getByRole("button", { name: "preview_show_original" }),
		).toBeEnabled();
	});

	it("retries the original request when the show-original flow failed", () => {
		mockState.imagePreviewPreference = "preview_first";
		mockState.useBlobUrl.mockImplementation((path: string | null) => {
			if (path === null) {
				return {
					blobUrl: null,
					error: false,
					loading: false,
					retry: mockState.retry,
				};
			}
			return {
				blobUrl: path.includes("image-preview") ? "blob:medium" : null,
				error: !path.includes("image-preview"),
				loading: false,
				retry: mockState.retry,
			};
		});

		render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", { name: "preview_show_original" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "preview_show_original" }),
		);

		expect(mockState.retry).toHaveBeenCalledTimes(1);
		expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
			"src",
			"blob:medium",
		);
	});

	it("does not download the original automatically when the backend preview loading fails", () => {
		mockState.imagePreviewPreference = "preview_first";
		mockState.useBlobUrl.mockImplementation((path: string | null) => {
			if (path === null) {
				return {
					blobUrl: null,
					error: false,
					loading: false,
					retry: mockState.retry,
				};
			}
			return {
				blobUrl: path.includes("image-preview") ? null : "blob:original",
				error: path.includes("image-preview"),
				loading: false,
				retry: mockState.retry,
			};
		});

		render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith(
			"/files/1/image-preview",
			{ lane: "preview" },
		);
		expect(mockState.useBlobUrl).not.toHaveBeenCalledWith("/files/1/download", {
			lane: "default",
		});
		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "preview_show_original" }),
		).toBeInTheDocument();
	});

	it("shows the retry state when the selected preview source fails", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: true,
			loading: false,
			retry: mockState.retry,
		});

		render(
			<BlobImagePreview
				file={file}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/1/download", {
			lane: "default",
		});
		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
	});

	it("clears image render failure before retrying the selected source", () => {
		render(<BlobImagePreview file={file} path="/files/1/download" />);

		fireEvent.error(screen.getByRole("img", { name: "preview.png" }));
		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "preview_retry" }));

		expect(mockState.retry).toHaveBeenCalledTimes(1);
		expect(screen.getByRole("img", { name: "preview.png" })).toHaveAttribute(
			"src",
			"blob:image",
		);
	});

	it("does not switch sources automatically on image render errors", () => {
		mockState.imagePreviewPreference = "preview_first";
		mockState.useBlobUrl.mockImplementation((path: string | null) => {
			if (path === null) {
				return {
					blobUrl: null,
					error: false,
					loading: false,
					retry: mockState.retry,
				};
			}
			return {
				blobUrl: path.includes("image-preview")
					? "blob:medium"
					: "blob:original",
				error: false,
				loading: false,
				retry: mockState.retry,
			};
		});

		render(
			<BlobImagePreview
				file={{ name: "photo.heic", mime_type: "image/heic" }}
				path="/files/1/download"
				fallbackPath="/files/1/image-preview"
			/>,
		);

		fireEvent.error(screen.getByRole("img", { name: "photo.heic" }));

		expect(mockState.useBlobUrl).toHaveBeenCalledWith(
			"/files/1/image-preview",
			{ lane: "preview" },
		);
		expect(mockState.useBlobUrl).not.toHaveBeenCalledWith("/files/1/download", {
			lane: "default",
		});
		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
	});
});
