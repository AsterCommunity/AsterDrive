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

function renderPanel(
	overrides: Partial<React.ComponentProps<typeof ImagePreviewPanel>> = {},
) {
	const props: React.ComponentProps<typeof ImagePreviewPanel> = {
		file,
		allOptionsCount: 1,
		downloadPath: "/files/7/download",
		imagePreviewPath: "/files/7/image-preview",
		isExpanded: true,
		onChooseOpenMethod: vi.fn(),
		onClose: vi.fn(),
		onToggleExpand: vi.fn(),
		chooseOpenMethodLabel: "Choose open method",
		enterFullscreenLabel: "Fill window",
		exitFullscreenLabel: "Restore window",
		closeLabel: "Close",
		fitToWindowLabel: "Fit to window",
		previewSourceLabel: "Preview",
		originalSourceLabel: "Original",
		rotateRightLabel: "Rotate right",
		zoomInLabel: "Zoom in",
		zoomOutLabel: "Zoom out",
		...overrides,
	};

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
			screen.getByRole("button", { name: "Restore window" }),
		).toBeInTheDocument();
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

	it("toggles fullscreen and closes through toolbar buttons", () => {
		const props = renderPanel();

		fireEvent.click(screen.getByRole("button", { name: "Restore window" }));
		fireEvent.click(screen.getByRole("button", { name: "Close" }));

		expect(props.onToggleExpand).toHaveBeenCalledTimes(1);
		expect(props.onClose).toHaveBeenCalledTimes(1);
	});

	it("uses the original source immediately when preview-first is disabled", () => {
		mockState.imagePreviewPreference = "original_first";

		renderPanel();

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

		expect(addEventListenerSpy).toHaveBeenCalledWith(
			"wheel",
			expect.any(Function),
			{ passive: false },
		);
		addEventListenerSpy.mockRestore();
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
		const { rerender } = render(
			<ImagePreviewPanel
				file={file}
				allOptionsCount={1}
				downloadPath="/files/7/download"
				imagePreviewPath="/files/7/image-preview"
				isExpanded
				onChooseOpenMethod={vi.fn()}
				onClose={vi.fn()}
				onToggleExpand={vi.fn()}
				chooseOpenMethodLabel="Choose open method"
				enterFullscreenLabel="Fill window"
				exitFullscreenLabel="Restore window"
				closeLabel="Close"
				fitToWindowLabel="Fit to window"
				previewSourceLabel="Preview"
				originalSourceLabel="Original"
				rotateRightLabel="Rotate right"
				zoomInLabel="Zoom in"
				zoomOutLabel="Zoom out"
			/>,
		);

		expect(screen.getByText("Preview")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "Original" }));
		expect(mockState.useBlobUrl).toHaveBeenLastCalledWith("/files/7/download", {
			lane: "default",
		});

		const loadingButton = screen.getByRole("button", { name: "Original" });
		expect(loadingButton).toBeDisabled();
		expect(loadingButton.querySelector("svg")).toHaveClass("animate-spin");

		mockState.originalBlobUrl = "blob:original";
		rerender(
			<ImagePreviewPanel
				file={file}
				allOptionsCount={1}
				downloadPath="/files/7/download"
				imagePreviewPath="/files/7/image-preview"
				isExpanded
				onChooseOpenMethod={vi.fn()}
				onClose={vi.fn()}
				onToggleExpand={vi.fn()}
				chooseOpenMethodLabel="Choose open method"
				enterFullscreenLabel="Fill window"
				exitFullscreenLabel="Restore window"
				closeLabel="Close"
				fitToWindowLabel="Fit to window"
				previewSourceLabel="Preview"
				originalSourceLabel="Original"
				rotateRightLabel="Rotate right"
				zoomInLabel="Zoom in"
				zoomOutLabel="Zoom out"
			/>,
		);
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
		const { rerender } = render(
			<ImagePreviewPanel
				file={file}
				allOptionsCount={1}
				downloadPath="/files/7/download"
				imagePreviewPath="/files/7/image-preview"
				isExpanded
				onChooseOpenMethod={vi.fn()}
				onClose={vi.fn()}
				onToggleExpand={vi.fn()}
				chooseOpenMethodLabel="Choose open method"
				enterFullscreenLabel="Fill window"
				exitFullscreenLabel="Restore window"
				closeLabel="Close"
				fitToWindowLabel="Fit to window"
				previewSourceLabel="Preview"
				originalSourceLabel="Original"
				rotateRightLabel="Rotate right"
				zoomInLabel="Zoom in"
				zoomOutLabel="Zoom out"
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Original" }));
		mockState.originalError = true;
		rerender(
			<ImagePreviewPanel
				file={file}
				allOptionsCount={1}
				downloadPath="/files/7/download"
				imagePreviewPath="/files/7/image-preview"
				isExpanded
				onChooseOpenMethod={vi.fn()}
				onClose={vi.fn()}
				onToggleExpand={vi.fn()}
				chooseOpenMethodLabel="Choose open method"
				enterFullscreenLabel="Fill window"
				exitFullscreenLabel="Restore window"
				closeLabel="Close"
				fitToWindowLabel="Fit to window"
				previewSourceLabel="Preview"
				originalSourceLabel="Original"
				rotateRightLabel="Rotate right"
				zoomInLabel="Zoom in"
				zoomOutLabel="Zoom out"
			/>,
		);

		expect(screen.getByRole("button", { name: "Original" })).toBeEnabled();
		expect(mockState.blobProps?.source).toBe("backend_preview");
	});

	it("falls back to the backend preview when the downloaded original cannot render", () => {
		const { rerender } = render(
			<ImagePreviewPanel
				file={file}
				allOptionsCount={1}
				downloadPath="/files/7/download"
				imagePreviewPath="/files/7/image-preview"
				isExpanded
				onChooseOpenMethod={vi.fn()}
				onClose={vi.fn()}
				onToggleExpand={vi.fn()}
				chooseOpenMethodLabel="Choose open method"
				enterFullscreenLabel="Fill window"
				exitFullscreenLabel="Restore window"
				closeLabel="Close"
				fitToWindowLabel="Fit to window"
				previewSourceLabel="Preview"
				originalSourceLabel="Original"
				rotateRightLabel="Rotate right"
				zoomInLabel="Zoom in"
				zoomOutLabel="Zoom out"
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Original" }));
		mockState.originalBlobUrl = "blob:original";
		rerender(
			<ImagePreviewPanel
				file={file}
				allOptionsCount={1}
				downloadPath="/files/7/download"
				imagePreviewPath="/files/7/image-preview"
				isExpanded
				onChooseOpenMethod={vi.fn()}
				onClose={vi.fn()}
				onToggleExpand={vi.fn()}
				chooseOpenMethodLabel="Choose open method"
				enterFullscreenLabel="Fill window"
				exitFullscreenLabel="Restore window"
				closeLabel="Close"
				fitToWindowLabel="Fit to window"
				previewSourceLabel="Preview"
				originalSourceLabel="Original"
				rotateRightLabel="Rotate right"
				zoomInLabel="Zoom in"
				zoomOutLabel="Zoom out"
			/>,
		);

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
}
