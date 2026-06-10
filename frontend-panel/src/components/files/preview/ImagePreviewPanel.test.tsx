import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ImagePreviewPanel } from "@/components/files/preview/ImagePreviewPanel";
import type { ImagePreviewSource } from "./BlobImagePreview";

const mockState = vi.hoisted(() => ({
	blobProps: null as null | {
		imageStyle?: React.CSSProperties;
		source?: ImagePreviewSource;
	},
	imagePreviewPreference: "preview_first",
	originalBlobUrl: null as string | null,
	originalError: false,
	originalLoading: false,
	previewError: false,
	previewLoading: false,
	retry: vi.fn(),
	useBlobUrl: vi.fn(),
}));

vi.mock("@/components/files/preview/BlobImagePreview", () => ({
	BlobImagePreview: ({
		imageRef,
		imageStyle,
		onImageLoad,
		onImageRenderError,
		source,
		viewportRef,
	}: {
		imageRef?: React.Ref<HTMLImageElement>;
		imageStyle?: React.CSSProperties;
		onImageLoad?: (source: ImagePreviewSource) => void;
		onImageRenderError?: (source: ImagePreviewSource) => void;
		source?: ImagePreviewSource;
		viewportRef?: React.Ref<HTMLDivElement>;
	}) => {
		mockState.blobProps = {
			imageStyle,
			source,
		};
		if (mockState.previewError) {
			return (
				<button type="button" onClick={mockState.retry}>
					<svg aria-hidden="true" data-testid="retry-image-icon" />
					Retry image
				</button>
			);
		}
		if (mockState.previewLoading) {
			return <div data-testid="panel-preview-loading" />;
		}

		return (
			<div data-testid="panel-preview-viewport" ref={viewportRef}>
				<img
					alt="panel-preview"
					data-testid="panel-preview-image"
					ref={imageRef}
					src="blob:preview"
					style={imageStyle}
					onLoad={() => source && onImageLoad?.(source)}
					onError={() => source && onImageRenderError?.(source)}
				/>
			</div>
		);
	},
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

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `${value} bytes`,
}));

const file = {
	id: 7,
	mime_type: "image/png",
	name: "photo.png",
	size: 2048,
};

function panelProps(
	overrides: Partial<React.ComponentProps<typeof ImagePreviewPanel>> = {},
) {
	return {
		file,
		allOptionsCount: 1,
		downloadPath: "/files/7/download",
		imagePreviewPath: "/files/7/image-preview",
		onChooseOpenMethod: vi.fn(),
		onClose: vi.fn(),
		chooseOpenMethodLabel: "Choose open method",
		closeLabel: "Close",
		fitToWindowLabel: "Fit to window",
		nextImageLabel: "Next image",
		previousImageLabel: "Previous image",
		previewSourceLabel: "Preview",
		originalSourceLabel: "Original",
		rotateRightLabel: "Rotate right",
		zoomInLabel: "Zoom in",
		zoomOutLabel: "Zoom out",
		...overrides,
	};
}

function renderPanel(
	overrides: Partial<React.ComponentProps<typeof ImagePreviewPanel>> = {},
) {
	const props = panelProps(overrides);
	render(<ImagePreviewPanel {...props} />);
	return props;
}

describe("ImagePreviewPanel", () => {
	beforeEach(() => {
		mockState.blobProps = null;
		mockState.imagePreviewPreference = "preview_first";
		mockState.originalBlobUrl = null;
		mockState.originalError = false;
		mockState.originalLoading = false;
		mockState.previewError = false;
		mockState.previewLoading = false;
		mockState.retry.mockReset();
		mockState.useBlobUrl.mockReset();
		mockState.useBlobUrl.mockImplementation((path: string | null) => ({
			blobUrl:
				path === "/files/7/download"
					? mockState.originalBlobUrl
					: "blob:preview",
			error: path === "/files/7/download" ? mockState.originalError : false,
			loading: path === "/files/7/download" ? mockState.originalLoading : false,
			retry: mockState.retry,
		}));
		Object.defineProperty(HTMLElement.prototype, "setPointerCapture", {
			configurable: true,
			value: vi.fn(),
		});
		Object.defineProperty(HTMLElement.prototype, "hasPointerCapture", {
			configurable: true,
			value: vi.fn(() => true),
		});
		Object.defineProperty(HTMLElement.prototype, "releasePointerCapture", {
			configurable: true,
			value: vi.fn(),
		});
		vi.useRealTimers();
	});

	it("renders media viewer chrome and forwards preview paths", () => {
		renderPanel();

		expect(screen.getByText("photo.png")).toBeInTheDocument();
		expect(screen.getByText("2048 bytes · image/png")).toBeInTheDocument();
		expect(screen.getByText("Original")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "Restore window" }),
		).not.toBeInTheDocument();
		expect(mockState.blobProps?.source).toBe("backend_preview");
	});

	it("keeps the top and bottom chrome position stable during close fade", () => {
		renderPanel();

		const topChrome = screen.getByText("photo.png").closest(".absolute");
		const bottomChrome = screen
			.getByRole("button", { name: "Fit to window" })
			.closest(".absolute");

		expect(topChrome?.className).toContain("transition-opacity");
		expect(topChrome?.className).not.toContain("translate-y");
		expect(bottomChrome?.className).toContain("transition-opacity");
		expect(bottomChrome?.className).not.toContain("translate-y");
	});

	it("shows open-method control only when multiple methods exist", () => {
		const props = renderPanel({ allOptionsCount: 2 });

		fireEvent.click(screen.getByRole("button", { name: "Choose open method" }));

		expect(props.onChooseOpenMethod).toHaveBeenCalledTimes(1);
	});

	it("hides open-method control when there is only one method", () => {
		renderPanel({ allOptionsCount: 1 });

		expect(
			screen.queryByRole("button", { name: "Choose open method" }),
		).not.toBeInTheDocument();
	});

	it("omits fullscreen controls and closes through the toolbar", () => {
		const props = renderPanel();

		expect(
			screen.queryByRole("button", { name: "Restore window" }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "Fill window" }),
		).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "Close" }));

		expect(props.onClose).toHaveBeenCalledTimes(1);
	});

	it("closes when clicking preview letterbox space but not the image", () => {
		const props = renderPanel();
		const viewport = screen.getByTestId("panel-preview-viewport");
		const image = screen.getByTestId("panel-preview-image");

		fireEvent.pointerDown(image, { clientX: 120, clientY: 120 });
		fireEvent.click(image, { clientX: 120, clientY: 120 });

		expect(props.onClose).not.toHaveBeenCalled();

		fireEvent.pointerDown(viewport, { clientX: 24, clientY: 24 });
		fireEvent.click(viewport, { clientX: 24, clientY: 24 });

		expect(props.onClose).toHaveBeenCalledTimes(1);
	});

	it("does not close when clicking an interactive control inside the preview surface", () => {
		mockState.previewError = true;
		const props = renderPanel();
		const retryButton = screen.getByRole("button", { name: "Retry image" });

		fireEvent.pointerDown(retryButton, { clientX: 120, clientY: 120 });
		fireEvent.click(retryButton, { clientX: 120, clientY: 120 });

		expect(mockState.retry).toHaveBeenCalledTimes(1);
		expect(props.onClose).not.toHaveBeenCalled();
	});

	it("does not close when clicking an svg icon inside an interactive control", () => {
		mockState.previewError = true;
		const props = renderPanel();
		const retryIcon = screen.getByTestId("retry-image-icon");

		fireEvent.pointerDown(retryIcon, { clientX: 120, clientY: 120 });
		fireEvent.click(retryIcon, { clientX: 120, clientY: 120 });

		expect(props.onClose).not.toHaveBeenCalled();
	});

	it("does not close when an interactive preview control receives click without a pointer start", () => {
		mockState.previewError = true;
		const props = renderPanel();
		const retryButton = screen.getByRole("button", { name: "Retry image" });

		fireEvent.click(retryButton, { clientX: 120, clientY: 120 });

		expect(mockState.retry).toHaveBeenCalledTimes(1);
		expect(props.onClose).not.toHaveBeenCalled();
	});

	it("navigates to adjacent images through side buttons and arrow keys", () => {
		const previousFile = { ...file, id: 6, name: "previous.png" };
		const nextFile = { ...file, id: 8, name: "next.png" };
		const onNavigateImage = vi.fn();
		renderPanel({
			previousImageFile: previousFile,
			nextImageFile: nextFile,
			onNavigateImage,
		});

		const previousButton = screen.getByRole("button", {
			name: "Previous image",
		});
		const nextButton = screen.getByRole("button", { name: "Next image" });

		expect(previousButton).toHaveAttribute("title", "previous.png");
		expect(nextButton).toHaveAttribute("title", "next.png");

		fireEvent.click(previousButton);
		fireEvent.click(nextButton);
		fireEvent.keyDown(window, { key: "ArrowLeft" });
		fireEvent.keyDown(window, { key: "ArrowRight" });
		fireEvent.keyDown(window, { key: "ArrowUp" });

		expect(onNavigateImage).toHaveBeenNthCalledWith(1, previousFile);
		expect(onNavigateImage).toHaveBeenNthCalledWith(2, nextFile);
		expect(onNavigateImage).toHaveBeenNthCalledWith(3, previousFile);
		expect(onNavigateImage).toHaveBeenNthCalledWith(4, nextFile);
		expect(onNavigateImage).toHaveBeenCalledTimes(4);
	});

	it("captures image navigation keys before focused controls can stop propagation", () => {
		const previousFile = { ...file, id: 6, name: "previous.png" };
		const nextFile = { ...file, id: 8, name: "next.png" };
		const onNavigateImage = vi.fn();
		renderPanel({
			previousImageFile: previousFile,
			nextImageFile: nextFile,
			onNavigateImage,
		});
		const focusedControl = screen.getByRole("button", { name: "Close" });
		const stopPropagation = vi.fn((event: KeyboardEvent) => {
			event.stopPropagation();
		});
		focusedControl.addEventListener("keydown", stopPropagation);

		fireEvent.keyDown(focusedControl, { key: "ArrowLeft" });
		fireEvent.keyDown(focusedControl, { key: "ArrowRight" });

		expect(stopPropagation).toHaveBeenCalledTimes(2);
		expect(onNavigateImage).toHaveBeenNthCalledWith(1, previousFile);
		expect(onNavigateImage).toHaveBeenNthCalledWith(2, nextFile);
		expect(onNavigateImage).toHaveBeenCalledTimes(2);
	});

	it("does not navigate with arrow keys from editable targets", () => {
		const previousFile = { ...file, id: 6, name: "previous.png" };
		const nextFile = { ...file, id: 8, name: "next.png" };
		const onNavigateImage = vi.fn();
		renderPanel({
			previousImageFile: previousFile,
			nextImageFile: nextFile,
			onNavigateImage,
		});

		const input = document.createElement("input");
		const textarea = document.createElement("textarea");
		const editable = document.createElement("div");
		Object.defineProperty(editable, "isContentEditable", {
			configurable: true,
			value: true,
		});
		document.body.append(input, textarea, editable);

		fireEvent.keyDown(input, { key: "ArrowLeft" });
		fireEvent.keyDown(textarea, { key: "ArrowRight" });
		fireEvent.keyDown(editable, { key: "ArrowLeft" });

		expect(onNavigateImage).not.toHaveBeenCalled();
	});

	it("renders only available image navigation directions", () => {
		const previousFile = { ...file, id: 6, name: "previous.png" };
		const nextFile = { ...file, id: 8, name: "next.png" };
		const onNavigateImage = vi.fn();
		const { rerender } = render(
			<ImagePreviewPanel
				{...panelProps({
					previousImageFile: previousFile,
					onNavigateImage,
				})}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Previous image" }));
		fireEvent.keyDown(window, { key: "ArrowRight" });
		fireEvent.keyDown(window, { key: "ArrowLeft" });
		expect(
			screen.queryByRole("button", { name: "Next image" }),
		).not.toBeInTheDocument();
		expect(onNavigateImage).toHaveBeenNthCalledWith(1, previousFile);
		expect(onNavigateImage).toHaveBeenNthCalledWith(2, previousFile);

		rerender(
			<ImagePreviewPanel
				{...panelProps({
					nextImageFile: nextFile,
					onNavigateImage,
				})}
			/>,
		);

		fireEvent.keyDown(window, { key: "ArrowLeft" });
		fireEvent.click(screen.getByRole("button", { name: "Next image" }));
		fireEvent.keyDown(window, { key: "ArrowRight" });
		expect(
			screen.queryByRole("button", { name: "Previous image" }),
		).not.toBeInTheDocument();
		expect(onNavigateImage).toHaveBeenNthCalledWith(3, nextFile);
		expect(onNavigateImage).toHaveBeenNthCalledWith(4, nextFile);
		expect(onNavigateImage).toHaveBeenCalledTimes(4);
	});

	it("hides side navigation when no navigation callback is available", () => {
		renderPanel({
			previousImageFile: { ...file, id: 6, name: "previous.png" },
			nextImageFile: { ...file, id: 8, name: "next.png" },
		});

		expect(
			screen.queryByRole("button", { name: "Previous image" }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "Next image" }),
		).not.toBeInTheDocument();
	});

	it("does not navigate with arrow keys when the key event was already handled", () => {
		const previousFile = { ...file, id: 6, name: "previous.png" };
		const nextFile = { ...file, id: 8, name: "next.png" };
		const onNavigateImage = vi.fn();
		renderPanel({
			previousImageFile: previousFile,
			nextImageFile: nextFile,
			onNavigateImage,
		});

		const leftEvent = new KeyboardEvent("keydown", {
			bubbles: true,
			cancelable: true,
			key: "ArrowLeft",
		});
		leftEvent.preventDefault();
		window.dispatchEvent(leftEvent);

		const rightEvent = new KeyboardEvent("keydown", {
			bubbles: true,
			cancelable: true,
			key: "ArrowRight",
		});
		rightEvent.preventDefault();
		window.dispatchEvent(rightEvent);

		expect(onNavigateImage).not.toHaveBeenCalled();
	});

	it("registers and removes image navigation keys in the capture phase", () => {
		const previousFile = { ...file, id: 6, name: "previous.png" };
		const addEventListenerSpy = vi.spyOn(window, "addEventListener");
		const removeEventListenerSpy = vi.spyOn(window, "removeEventListener");
		const { unmount } = render(
			<ImagePreviewPanel
				{...panelProps({
					previousImageFile: previousFile,
					onNavigateImage: vi.fn(),
				})}
			/>,
		);

		expect(addEventListenerSpy).toHaveBeenCalledWith(
			"keydown",
			expect.any(Function),
			{ capture: true },
		);
		unmount();
		expect(removeEventListenerSpy).toHaveBeenCalledWith(
			"keydown",
			expect.any(Function),
			{ capture: true },
		);
		addEventListenerSpy.mockRestore();
		removeEventListenerSpy.mockRestore();
	});

	it("does not register image navigation keys without files or callback", () => {
		const onNavigateImage = vi.fn();
		const addEventListenerSpy = vi.spyOn(window, "addEventListener");
		const removeEventListenerSpy = vi.spyOn(window, "removeEventListener");
		const { unmount } = render(
			<ImagePreviewPanel {...panelProps({ onNavigateImage })} />,
		);

		fireEvent.keyDown(window, { key: "ArrowLeft" });
		fireEvent.keyDown(window, { key: "ArrowRight" });
		unmount();

		expect(onNavigateImage).not.toHaveBeenCalled();
		expect(addEventListenerSpy).not.toHaveBeenCalledWith(
			"keydown",
			expect.any(Function),
		);
		expect(removeEventListenerSpy).not.toHaveBeenCalledWith(
			"keydown",
			expect.any(Function),
		);
		addEventListenerSpy.mockRestore();
		removeEventListenerSpy.mockRestore();
	});

	it("removes image navigation key handlers after unmount", () => {
		const previousFile = { ...file, id: 6, name: "previous.png" };
		const onNavigateImage = vi.fn();
		const { unmount } = render(
			<ImagePreviewPanel
				{...panelProps({
					previousImageFile: previousFile,
					onNavigateImage,
				})}
			/>,
		);

		fireEvent.keyDown(window, { key: "ArrowLeft" });
		unmount();
		fireEvent.keyDown(window, { key: "ArrowLeft" });

		expect(onNavigateImage).toHaveBeenCalledTimes(1);
		expect(onNavigateImage).toHaveBeenCalledWith(previousFile);
	});

	it("uses the original source immediately when preview-first is disabled", () => {
		mockState.imagePreviewPreference = "original_first";

		renderPanel();

		expect(screen.getByText("Original")).toBeInTheDocument();
		expect(mockState.blobProps?.source).toBe("original");
	});

	it("uses the backend preview first when an original-first image is not browser-renderable", () => {
		mockState.imagePreviewPreference = "original_first";

		renderPanel({
			file: {
				...file,
				mime_type: "image/x-nikon-nef",
				name: "capture.nef",
			},
		});

		expect(screen.getByText("Preview")).toBeInTheDocument();
		expect(mockState.blobProps?.source).toBe("backend_preview");
		expect(
			screen.queryByRole("button", { name: "Original" }),
		).not.toBeInTheDocument();
		expect(mockState.useBlobUrl).not.toHaveBeenCalledWith("/files/7/download", {
			lane: "default",
		});
	});

	it("falls back to the backend preview when an original-first renderable image fails to render", () => {
		mockState.imagePreviewPreference = "original_first";

		renderPanel();

		expect(screen.getByText("Original")).toBeInTheDocument();
		expect(mockState.blobProps?.source).toBe("original");
		fireEvent.error(screen.getByTestId("panel-preview-image"));

		expect(screen.getByText("Preview")).toBeInTheDocument();
		expect(mockState.blobProps?.source).toBe("backend_preview");
		expect(
			screen.queryByRole("button", { name: "Original" }),
		).not.toBeInTheDocument();
	});

	it("uses the original source when preview-first has no backend preview path", () => {
		mockState.imagePreviewPreference = "preview_first";

		renderPanel({ imagePreviewPath: undefined });

		expect(screen.getByText("Original")).toBeInTheDocument();
		expect(mockState.blobProps?.source).toBe("original");
	});

	it("zooms in, zooms out, and resets to fit", () => {
		renderPanel();

		fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("125%");
		expect(mockState.blobProps?.imageStyle?.transform).toContain("scale(1.25)");

		fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("100%");

		fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
		fireEvent.click(screen.getByRole("button", { name: "Fit to window" }));
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("100%");
	});

	it("resets the image transform when switching preview sources", () => {
		const props = panelProps();
		const { rerender } = render(<ImagePreviewPanel {...props} />);

		fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("125%");
		expect(mockState.blobProps?.imageStyle?.transform).toContain("scale(1.25)");

		fireEvent.click(screen.getByRole("button", { name: "Original" }));
		mockState.originalBlobUrl = "blob:original";
		rerender(<ImagePreviewPanel {...props} />);

		expect(mockState.blobProps?.source).toBe("original");
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("100%");
		expect(mockState.blobProps?.imageStyle?.transform).toContain("scale(1)");
	});

	it("clamps zoom controls at the min and max edges", () => {
		renderPanel();

		for (let index = 0; index < 20; index += 1) {
			fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
		}
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("500%");
		expect(screen.getByRole("button", { name: "Zoom in" })).toBeDisabled();

		for (let index = 0; index < 20; index += 1) {
			fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
		}
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("50%");
		expect(screen.getByRole("button", { name: "Zoom out" })).toBeDisabled();
	});

	it("rotates right and resets rotation when fitting to window", () => {
		renderPanel();

		fireEvent.click(screen.getByRole("button", { name: "Rotate right" }));
		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"rotate(90deg)",
		);

		fireEvent.click(screen.getByRole("button", { name: "Fit to window" }));
		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"rotate(0deg)",
		);
	});

	it("resets rotated images through the shortest visual path to an upright angle", () => {
		renderPanel();

		for (let index = 0; index < 3; index += 1) {
			fireEvent.click(screen.getByRole("button", { name: "Rotate right" }));
		}
		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"rotate(270deg)",
		);
		fireEvent.click(screen.getByRole("button", { name: "Fit to window" }));
		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"rotate(360deg)",
		);

		for (let index = 0; index < 3; index += 1) {
			fireEvent.click(screen.getByRole("button", { name: "Rotate right" }));
		}
		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"rotate(630deg)",
		);
		fireEvent.click(screen.getByRole("button", { name: "Fit to window" }));
		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"rotate(720deg)",
		);
	});

	it("keeps right rotation moving forward when wrapping past 270 degrees", () => {
		renderPanel();

		for (let index = 0; index < 4; index += 1) {
			fireEvent.click(screen.getByRole("button", { name: "Rotate right" }));
		}

		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"rotate(360deg)",
		);
		expect(mockState.blobProps?.imageStyle?.transform).not.toContain(
			"rotate(0deg)",
		);
	});

	it("zooms with ctrl wheel and ignores plain scroll", () => {
		renderPanel();
		loadPanelImage();
		const viewport = screen.getByTestId("panel-preview-viewport");

		fireEvent.wheel(viewport, {
			clientX: 200,
			clientY: 150,
			ctrlKey: false,
			deltaY: -100,
		});
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("100%");

		fireEvent.wheel(viewport, {
			clientX: 200,
			clientY: 150,
			ctrlKey: true,
			deltaY: -100,
		});

		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("125%");
		expect(mockState.blobProps?.imageStyle?.transform).toContain("scale(1.25)");
	});

	it("ignores wheel and pointer gestures until the image load completes", () => {
		renderPanel();
		const viewport = screen.getByTestId("panel-preview-viewport");
		const surface = getGestureSurface();

		fireEvent.wheel(viewport, {
			clientX: 200,
			clientY: 150,
			ctrlKey: true,
			deltaY: -100,
		});
		fireEvent.pointerDown(surface, {
			clientX: 100,
			clientY: 100,
			pointerId: 1,
		});
		fireEvent.pointerMove(surface, {
			clientX: 220,
			clientY: 180,
			pointerId: 1,
		});
		fireEvent.pointerUp(surface, {
			clientX: 220,
			clientY: 180,
			pointerId: 1,
		});

		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("100%");
		expect(mockState.blobProps?.imageStyle?.transform).toContain("scale(1)");
		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"translate3d(0px, 0px, 0)",
		);

		loadPanelImage();
		fireEvent.wheel(viewport, {
			clientX: 200,
			clientY: 150,
			ctrlKey: true,
			deltaY: -100,
		});

		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("125%");
	});

	it("disables gestures for the next image until that image load completes", () => {
		const props = panelProps();
		const { rerender } = render(<ImagePreviewPanel {...props} />);
		const viewport = screen.getByTestId("panel-preview-viewport");

		loadPanelImage();
		fireEvent.wheel(viewport, {
			clientX: 200,
			clientY: 150,
			ctrlKey: true,
			deltaY: -100,
		});
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("125%");

		rerender(
			<ImagePreviewPanel
				{...panelProps({
					file: { ...file, id: 8, name: "next.png" },
					downloadPath: "/files/8/download",
					imagePreviewPath: "/files/8/image-preview",
				})}
			/>,
		);

		const nextViewport = screen.getByTestId("panel-preview-viewport");
		fireEvent.wheel(nextViewport, {
			clientX: 200,
			clientY: 150,
			ctrlKey: true,
			deltaY: -100,
		});
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("100%");

		loadPanelImage();
		fireEvent.wheel(nextViewport, {
			clientX: 200,
			clientY: 150,
			ctrlKey: true,
			deltaY: -100,
		});
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("125%");
	});

	it("clears active pointers when gestures are disabled for a new image", () => {
		const { rerender } = render(<ImagePreviewPanel {...panelProps()} />);
		mockImageGeometry();
		const surface = getGestureSurface();
		fireEvent.pointerDown(surface, {
			clientX: 180,
			clientY: 150,
			pointerId: 1,
		});
		fireEvent.pointerDown(surface, {
			clientX: 220,
			clientY: 150,
			pointerId: 2,
		});

		rerender(
			<ImagePreviewPanel
				{...panelProps({
					file: { ...file, id: 8, name: "next.png" },
					downloadPath: "/files/8/download",
					imagePreviewPath: "/files/8/image-preview",
				})}
			/>,
		);
		loadPanelImage();
		const nextSurface = getGestureSurface();
		fireEvent.pointerDown(nextSurface, {
			clientX: 200,
			clientY: 150,
			pointerId: 3,
		});
		fireEvent.pointerMove(nextSurface, {
			clientX: 360,
			clientY: 230,
			pointerId: 3,
		});

		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("100%");
		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"translate3d(0px, 0px, 0)",
		);
	});

	it("zooms with meta wheel and keeps the pointer anchor in bounds", () => {
		renderPanel();
		mockImageGeometry();
		const image = screen.getByTestId("panel-preview-image");
		Object.defineProperty(image, "offsetWidth", {
			configurable: true,
			value: 400,
		});
		Object.defineProperty(image, "offsetHeight", {
			configurable: true,
			value: 300,
		});
		const viewport = screen.getByTestId("panel-preview-viewport");

		fireEvent.wheel(viewport, {
			clientX: 400,
			clientY: 300,
			deltaY: -100,
			metaKey: true,
		});

		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("125%");
		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"translate3d(-50px, -37.5px, 0)",
		);
	});

	it("registers wheel zoom as a non-passive listener on the image viewport", () => {
		const addEventListenerSpy = vi.spyOn(
			HTMLElement.prototype,
			"addEventListener",
		);

		renderPanel();

		expect(addEventListenerSpy).not.toHaveBeenCalledWith(
			"wheel",
			expect.any(Function),
			{ passive: false },
		);
		loadPanelImage();
		expect(addEventListenerSpy).toHaveBeenCalledWith(
			"wheel",
			expect.any(Function),
			{ passive: false },
		);
		addEventListenerSpy.mockRestore();
	});

	it("keeps wheel zoom active when the image viewport appears after loading", () => {
		mockState.previewLoading = true;
		const props = panelProps();
		const { rerender } = render(<ImagePreviewPanel {...props} />);
		const surface = screen.getByTestId("panel-preview-loading").parentElement;
		if (!surface) {
			throw new Error("Image gesture surface not found");
		}

		mockState.previewLoading = false;
		rerender(<ImagePreviewPanel {...props} />);
		mockImageGeometry();
		fireEvent.wheel(surface, {
			clientX: 200,
			clientY: 150,
			ctrlKey: true,
			deltaY: -100,
		});

		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("125%");
		expect(mockState.blobProps?.imageStyle?.transform).toContain("scale(1.25)");
	});

	it("does not drag the image while it is fitted", () => {
		renderPanel();
		mockImageGeometry();
		const surface = getGestureSurface();

		fireEvent.pointerDown(surface, {
			clientX: 100,
			clientY: 100,
			pointerId: 1,
		});
		fireEvent.pointerMove(surface, {
			clientX: 220,
			clientY: 180,
			pointerId: 1,
		});

		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"translate3d(0px, 0px, 0)",
		);
	});

	it("clamps drag movement to the visible zoomed image bounds", () => {
		renderPanel();
		mockImageGeometry();
		for (let index = 0; index < 16; index += 1) {
			fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
		}
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("500%");

		const surface = getGestureSurface();
		fireEvent.pointerDown(surface, {
			clientX: 200,
			clientY: 150,
			pointerId: 1,
		});
		fireEvent.pointerMove(surface, {
			clientX: 1200,
			clientY: 900,
			pointerId: 1,
		});

		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"translate3d(550px, 350px, 0)",
		);

		fireEvent.pointerMove(surface, {
			clientX: -1200,
			clientY: -900,
			pointerId: 1,
		});

		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"translate3d(-550px, -350px, 0)",
		);
	});

	it("reclamps dragged offsets when the viewport is resized", () => {
		renderPanel();
		mockImageGeometry();
		for (let index = 0; index < 16; index += 1) {
			fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
		}

		const surface = getGestureSurface();
		fireEvent.pointerDown(surface, {
			clientX: 200,
			clientY: 150,
			pointerId: 1,
		});
		fireEvent.pointerMove(surface, {
			clientX: 1200,
			clientY: 900,
			pointerId: 1,
		});
		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"translate3d(550px, 350px, 0)",
		);

		const viewport = screen.getByTestId("panel-preview-viewport");
		Object.defineProperty(viewport, "getBoundingClientRect", {
			configurable: true,
			value: () => ({
				bottom: 800,
				height: 800,
				left: 0,
				right: 1000,
				top: 0,
				width: 1000,
				x: 0,
				y: 0,
				toJSON: () => {},
			}),
		});

		act(() => {
			window.dispatchEvent(new Event("resize"));
		});

		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"translate3d(250px, 100px, 0)",
		);
	});

	it("clamps drag movement against sideways dimensions after rotation", () => {
		renderPanel();
		mockImageGeometry();
		fireEvent.click(screen.getByRole("button", { name: "Rotate right" }));
		for (let index = 0; index < 16; index += 1) {
			fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
		}

		const surface = getGestureSurface();
		fireEvent.pointerDown(surface, {
			clientX: 200,
			clientY: 150,
			pointerId: 1,
		});
		fireEvent.pointerMove(surface, {
			clientX: 1200,
			clientY: 900,
			pointerId: 1,
		});

		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"translate3d(300px, 600px, 0)",
		);
	});

	it("pinch-zooms with two pointers and clamps the resulting scale", () => {
		renderPanel();
		mockImageGeometry();
		const surface = getGestureSurface();

		fireEvent.pointerDown(surface, {
			clientX: 180,
			clientY: 150,
			pointerId: 1,
		});
		fireEvent.pointerDown(surface, {
			clientX: 220,
			clientY: 150,
			pointerId: 2,
		});
		fireEvent.pointerMove(surface, {
			clientX: -200,
			clientY: 150,
			pointerId: 1,
		});
		fireEvent.pointerMove(surface, {
			clientX: 600,
			clientY: 150,
			pointerId: 2,
		});

		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("500%");
		expect(mockState.blobProps?.imageStyle?.transform).toContain("scale(5)");
	});

	it("continues dragging the remaining pointer after a pinch ends", () => {
		renderPanel();
		mockImageGeometry();
		const surface = getGestureSurface();

		fireEvent.pointerDown(surface, {
			clientX: 180,
			clientY: 150,
			pointerId: 1,
		});
		fireEvent.pointerDown(surface, {
			clientX: 220,
			clientY: 150,
			pointerId: 2,
		});
		fireEvent.pointerMove(surface, {
			clientX: 120,
			clientY: 150,
			pointerId: 1,
		});
		fireEvent.pointerMove(surface, {
			clientX: 280,
			clientY: 150,
			pointerId: 2,
		});
		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("400%");

		fireEvent.pointerUp(surface, {
			clientX: 280,
			clientY: 150,
			pointerId: 2,
		});
		fireEvent.pointerMove(surface, {
			clientX: 400,
			clientY: 210,
			pointerId: 1,
		});

		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"translate3d(280px, 60px, 0)",
		);
	});

	it("ignores inactive pointer moves and zero-distance pinches", () => {
		renderPanel();
		mockImageGeometry();
		const surface = getGestureSurface();

		fireEvent.pointerMove(surface, {
			clientX: 260,
			clientY: 210,
			pointerId: 99,
		});
		expect(mockState.blobProps?.imageStyle?.transform).toContain(
			"translate3d(0px, 0px, 0)",
		);

		fireEvent.pointerDown(surface, {
			clientX: 200,
			clientY: 150,
			pointerId: 1,
		});
		fireEvent.pointerDown(surface, {
			clientX: 200,
			clientY: 150,
			pointerId: 2,
		});
		fireEvent.pointerMove(surface, {
			clientX: 260,
			clientY: 150,
			pointerId: 2,
		});

		expect(
			screen.getByRole("button", { name: "Fit to window" }),
		).toHaveTextContent("100%");
		expect(mockState.blobProps?.imageStyle?.transform).toContain("scale(1)");
	});

	it("skips releasePointerCapture when the surface no longer owns capture", () => {
		renderPanel();
		loadPanelImage();
		const surface = getGestureSurface();
		const hasPointerCapture = vi
			.spyOn(HTMLElement.prototype, "hasPointerCapture")
			.mockReturnValue(false);
		const releasePointerCapture = vi.spyOn(
			HTMLElement.prototype,
			"releasePointerCapture",
		);

		fireEvent.pointerDown(surface, {
			clientX: 100,
			clientY: 100,
			pointerId: 1,
		});
		fireEvent.pointerUp(surface, {
			clientX: 100,
			clientY: 100,
			pointerId: 1,
		});

		expect(hasPointerCapture).toHaveBeenCalledWith(1);
		expect(releasePointerCapture).not.toHaveBeenCalled();
	});

	it("requests the original and renders loading and success states with collapse animation classes", () => {
		vi.useFakeTimers();
		const props = panelProps();
		const { rerender } = render(<ImagePreviewPanel {...props} />);

		expect(screen.getByText("Preview")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "Original" }));
		expect(mockState.useBlobUrl).toHaveBeenLastCalledWith("/files/7/download", {
			lane: "default",
		});

		const loadingButton = screen.getByRole("button", { name: "Original" });
		expect(loadingButton).toBeDisabled();
		expect(loadingButton.querySelector("svg")).toHaveClass("animate-spin");

		mockState.originalBlobUrl = "blob:original";
		rerender(<ImagePreviewPanel {...props} />);
		expect(mockState.blobProps?.source).toBe("original");
		expect(screen.getByRole("button", { name: "Original" })).toBeDisabled();
		fireEvent.load(screen.getByTestId("panel-preview-image"));

		act(() => {
			vi.advanceTimersByTime(650);
		});
		const collapsedSegment = screen
			.getByRole("button", { name: "Original" })
			.closest("div")?.parentElement;
		expect(collapsedSegment).toHaveClass(
			"max-w-0",
			"translate-x-2",
			"opacity-0",
		);

		act(() => {
			vi.advanceTimersByTime(220);
		});
		expect(
			screen.queryByRole("button", { name: "Original" }),
		).not.toBeInTheDocument();

		act(() => {
			vi.advanceTimersByTime(650 + 220);
		});
		expect(
			screen.queryByRole("button", { name: "Original" }),
		).not.toBeInTheDocument();
	});

	it("returns the original button to available when original loading fails", () => {
		const props = panelProps();
		const { rerender } = render(<ImagePreviewPanel {...props} />);

		fireEvent.click(screen.getByRole("button", { name: "Original" }));
		mockState.originalError = true;
		rerender(<ImagePreviewPanel {...props} />);

		expect(screen.getByRole("button", { name: "Original" })).toBeEnabled();
		expect(mockState.blobProps?.source).toBe("backend_preview");
	});

	it("falls back to the backend preview when the downloaded original cannot render", () => {
		const props = panelProps();
		const { rerender } = render(<ImagePreviewPanel {...props} />);

		fireEvent.click(screen.getByRole("button", { name: "Original" }));
		mockState.originalBlobUrl = "blob:original";
		rerender(<ImagePreviewPanel {...props} />);

		expect(mockState.blobProps?.source).toBe("original");
		fireEvent.error(screen.getByTestId("panel-preview-image"));

		expect(mockState.blobProps?.source).toBe("backend_preview");
		expect(screen.getByRole("button", { name: "Original" })).toBeEnabled();
	});
});

function getGestureSurface() {
	const surface = screen.getByTestId("panel-preview-viewport").parentElement;
	if (!surface) {
		throw new Error("Image gesture surface not found");
	}
	return surface;
}

function loadPanelImage() {
	fireEvent.load(screen.getByTestId("panel-preview-image"));
}

function mockImageGeometry() {
	const viewport = screen.getByTestId("panel-preview-viewport");
	const image = screen.getByTestId("panel-preview-image");
	Object.defineProperty(viewport, "getBoundingClientRect", {
		configurable: true,
		value: () => ({
			bottom: 300,
			height: 300,
			left: 0,
			right: 400,
			top: 0,
			width: 400,
			x: 0,
			y: 0,
			toJSON: () => {},
		}),
	});
	Object.defineProperty(image, "offsetWidth", {
		configurable: true,
		value: 300,
	});
	Object.defineProperty(image, "offsetHeight", {
		configurable: true,
		value: 200,
	});
	loadPanelImage();
}
