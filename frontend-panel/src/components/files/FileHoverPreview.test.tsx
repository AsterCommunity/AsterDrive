import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileHoverPreview } from "@/components/files/FileHoverPreview";

const mockState = vi.hoisted(() => ({
	onOpenChange: undefined as ((open: boolean) => void) | undefined,
	lastSideOffset: undefined as unknown,
	thumbnailSupportStore: {
		config: {
			version: 1,
			image_preview: { enabled: true, extensions: ["png"] },
			image_thumbnail: { enabled: true, extensions: ["png"] },
			audio_thumbnail: { enabled: true, extensions: ["mp3"] },
			video_thumbnail: { enabled: true, extensions: ["mp4"] },
		} as unknown,
		isLoaded: true,
		load: vi.fn(),
	},
	useBlobUrl: vi.fn(),
	thumbnailPath: vi.fn((id: number) => `/thumb/${id}`),
}));

vi.mock("@base-ui/react/preview-card", () => ({
	PreviewCard: {
		Root: ({
			open,
			onOpenChange,
			children,
		}: {
			open: boolean;
			onOpenChange: (open: boolean) => void;
			children: React.ReactNode;
		}) => {
			mockState.onOpenChange = onOpenChange;
			return open ? <div data-testid="preview-root">{children}</div> : null;
		},
		Portal: ({ children }: { children: React.ReactNode }) => children,
		Positioner: ({
			anchor,
			side,
			align,
			sideOffset,
			className,
			children,
		}: {
			anchor: Element | null;
			side: string;
			align: string;
			sideOffset: unknown;
			className?: string;
			children: React.ReactNode;
		}) => {
			mockState.lastSideOffset = sideOffset;
			return (
				<div
					data-testid="positioner"
					data-side={side}
					data-align={align}
					data-anchored={String(anchor !== null)}
					className={className}
				>
					{children}
				</div>
			);
		},
		Popup: ({
			className,
			children,
		}: {
			className?: string;
			children: React.ReactNode;
		}) => (
			<div data-testid="popup" className={className}>
				{children}
			</div>
		),
	},
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

const pngFile = {
	id: 7,
	name: "photo.png",
	mime_type: "image/png",
};

const anchorEl = document.createElement("div");

describe("FileHoverPreview", () => {
	beforeEach(() => {
		mockState.onOpenChange = undefined;
		mockState.thumbnailSupportStore.config = {
			version: 1,
			image_preview: { enabled: true, extensions: ["png"] },
			image_thumbnail: { enabled: true, extensions: ["png"] },
			audio_thumbnail: { enabled: true, extensions: ["mp3"] },
			video_thumbnail: { enabled: true, extensions: ["mp4"] },
		};
		mockState.thumbnailSupportStore.isLoaded = true;
		mockState.thumbnailSupportStore.load.mockReset();
		mockState.useBlobUrl.mockReset();
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: "blob:hover",
			error: false,
			loading: false,
		});
		mockState.thumbnailPath.mockClear();
	});

	it("renders nothing when the file type has no generated thumbnail", () => {
		const { container } = render(
			<FileHoverPreview
				anchor={anchorEl}
				file={{ id: 9, name: "report.pdf", mime_type: "application/pdf" }}
				open={true}
				onClose={vi.fn()}
			/>,
		);

		expect(container).toBeEmptyDOMElement();
		expect(mockState.useBlobUrl).toHaveBeenCalledWith(null, {
			lane: "thumbnail",
		});
	});

	it("renders nothing without a mounted anchor", () => {
		const { container } = render(
			<FileHoverPreview
				anchor={null}
				file={pngFile}
				open={true}
				onClose={vi.fn()}
			/>,
		);

		expect(container).toBeEmptyDOMElement();
	});

	it("does not subscribe to the blob before the hover intent fires", () => {
		render(
			<FileHoverPreview
				anchor={anchorEl}
				file={pngFile}
				open={false}
				onClose={vi.fn()}
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith(null, {
			lane: "thumbnail",
		});
		expect(screen.queryByTestId("preview-root")).not.toBeInTheDocument();
	});

	it("stays closed while the thumbnail blob is still unavailable", () => {
		mockState.useBlobUrl.mockReturnValue({
			blobUrl: null,
			error: false,
			loading: true,
		});

		render(
			<FileHoverPreview
				anchor={anchorEl}
				file={pngFile}
				open={true}
				onClose={vi.fn()}
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/thumb/7", {
			lane: "thumbnail",
		});
		expect(screen.queryByTestId("preview-root")).not.toBeInTheDocument();
	});

	it("expands the loaded thumbnail as a borderless rounded image above the anchor", () => {
		render(
			<FileHoverPreview
				anchor={anchorEl}
				file={pngFile}
				open={true}
				onClose={vi.fn()}
			/>,
		);

		const positioner = screen.getByTestId("positioner");
		expect(positioner).toHaveAttribute("data-side", "top");
		expect(positioner).toHaveAttribute("data-anchored", "true");
		expect(positioner.className).toContain("z-(--z-popover)");
		// 浮层必须 pointer-through：否则盖住锚点时抢走 hit-test，
		// 锚点收 pointerleave 会把预览瞬间关掉（开-关抖动回归）
		expect(positioner).toHaveClass("pointer-events-none");

		const popup = screen.getByTestId("popup");
		expect(popup).toHaveClass("pointer-events-none");

		const image = popup.querySelector("img");
		expect(image).toHaveAttribute("src", "blob:hover");
		expect(image).toHaveAttribute("alt", "photo.png");
		// 无框机制：圆角直接裁在图上，无 border/ring/背景容器
		expect(image).toHaveClass("rounded-xl", "object-cover", "shadow-xl");
		expect(image?.className).not.toContain("border");
		expect(image?.className).not.toContain("ring");
	});

	it("cover mode overlays the anchor itself with slightly enlarged width", () => {
		render(
			<FileHoverPreview
				anchor={anchorEl}
				file={pngFile}
				open={true}
				onClose={vi.fn()}
				cover
			/>,
		);

		const image = screen.getByTestId("popup").querySelector("img");
		// 宽度 = 锚点 ×1.15（略突出卡片，避免超宽图看起来"没展开"），高度按比例封顶
		expect(image).toHaveClass("w-[calc(var(--anchor-width)*1.15)]", "max-h-72");
		expect(image?.className).not.toContain("max-w-64");

		// 负偏移把浮层压回锚点区域：预览更高时底边对齐（不遮文件名），
		// 预览比锚点矮（超宽窄条）时垂直居中
		expect(typeof mockState.lastSideOffset).toBe("function");
		const sideOffset = mockState.lastSideOffset as (data: {
			anchor: { width: number; height: number };
			positioner: { width: number; height: number };
		}) => number;
		expect(
			sideOffset({
				anchor: { width: 180, height: 80 },
				positioner: { width: 207, height: 200 },
			}),
		).toBe(-80);
		expect(
			sideOffset({
				anchor: { width: 180, height: 80 },
				positioner: { width: 207, height: 30 },
			}),
		).toBe(-55);
	});

	it("uses a plain numeric side offset outside cover mode", () => {
		render(
			<FileHoverPreview
				anchor={anchorEl}
				file={pngFile}
				open={true}
				onClose={vi.fn()}
			/>,
		);

		expect(mockState.lastSideOffset).toBe(4);
		const image = screen.getByTestId("popup").querySelector("img");
		expect(image).toHaveClass("max-w-64", "max-h-56");
	});

	it("prefers the provided thumbnail path override", () => {
		render(
			<FileHoverPreview
				anchor={anchorEl}
				file={pngFile}
				open={true}
				onClose={vi.fn()}
				thumbnailPath="/custom-thumb"
			/>,
		);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/custom-thumb", {
			lane: "thumbnail",
		});
	});

	it("closes when the preview card requests a close", () => {
		const onClose = vi.fn();

		render(
			<FileHoverPreview
				anchor={anchorEl}
				file={pngFile}
				open={true}
				onClose={onClose}
			/>,
		);

		mockState.onOpenChange?.(false);
		expect(onClose).toHaveBeenCalledTimes(1);

		mockState.onOpenChange?.(true);
		expect(onClose).toHaveBeenCalledTimes(1);
	});
});
