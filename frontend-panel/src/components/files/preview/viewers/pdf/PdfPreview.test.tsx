import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PdfPreview } from "@/components/files/preview/viewers/pdf/PdfPreview";
import { derivedFileResource } from "@/lib/fileResource";

const mockState = vi.hoisted(() => ({
	documentBlob: new Blob(["%PDF"]),
	documentProps: null as Record<string, unknown> | null,
	pageProps: [] as Record<string, unknown>[],
	startAuthenticatedDownload: vi.fn(),
	useBlobUrl: vi.fn(),
	virtualCount: 0,
	virtualOverscan: 0,
	estimatedSizes: [] as number[],
	virtualItems: [] as {
		key: number;
		index: number;
		start: number;
		end: number;
		size: number;
	}[],
	measureElement: vi.fn(),
	measure: vi.fn(),
	scrollToIndex: vi.fn(),
	getTotalSize: vi.fn(() => 0),
	virtualizerInstance: null as Record<string, unknown> | null,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			key === "pdf_zoom_percent" && options?.zoom != null
				? `${key}:${options.zoom}`
				: key,
	}),
}));

vi.mock("@tanstack/react-virtual", () => ({
	useVirtualizer: (options: {
		count: number;
		overscan?: number;
		estimateSize: (index: number) => number;
	}) => {
		mockState.virtualCount = options.count;
		mockState.virtualOverscan = options.overscan ?? 0;
		mockState.estimatedSizes = Array.from(
			{ length: options.count },
			(_, index) => options.estimateSize(index),
		);
		let nextStart = 0;
		mockState.virtualItems = Array.from(
			{ length: Math.min(options.count, 7) },
			(_, index) => {
				const size = mockState.estimatedSizes[index];
				const start = nextStart;
				nextStart += size;
				return {
					key: index + 1,
					index,
					start,
					end: nextStart,
					size,
				};
			},
		);
		mockState.getTotalSize.mockImplementation(() =>
			mockState.estimatedSizes.reduce((total, size) => total + size, 0),
		);
		mockState.virtualizerInstance ??= {
			getVirtualItems: () => mockState.virtualItems,
			getTotalSize: mockState.getTotalSize,
			measure: mockState.measure,
			measureElement: mockState.measureElement,
			scrollToIndex: mockState.scrollToIndex,
		};
		return mockState.virtualizerInstance;
	},
}));

vi.mock("react-pdf", () => {
	const pdfjs = {
		GlobalWorkerOptions: {},
		version: "5.4.296",
	};

	return {
		Document: ({
			children,
			...props
		}: Record<string, unknown> & { children?: React.ReactNode }) => {
			mockState.documentProps = props;
			return <div data-testid="pdf-document">{children}</div>;
		},
		Page: (props: Record<string, unknown>) => {
			mockState.pageProps.push(props);
			return <div data-testid={`pdf-page-${props.pageNumber}`} />;
		},
		pdfjs,
	};
});

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		...props
	}: {
		children?: React.ReactNode;
		[key: string]: unknown;
	}) => (
		<button type="button" {...props}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: () => <span />,
}));

vi.mock("@/components/ui/input", () => ({
	Input: (props: Record<string, unknown>) => <input {...props} />,
}));

vi.mock("@/components/files/preview/shared/PreviewError", () => ({
	PreviewError: ({ onRetry }: { onRetry?: () => void }) => (
		<div>
			preview-error
			{onRetry ? (
				<button type="button" data-testid="preview-retry" onClick={onRetry}>
					retry
				</button>
			) : null}
		</div>
	),
}));

vi.mock("@/hooks/useBlobUrl", () => ({
	useBlobUrl: (...args: unknown[]) => mockState.useBlobUrl(...args),
}));

vi.mock("@/lib/authenticatedDownload", () => ({
	startAuthenticatedDownload: (...args: unknown[]) =>
		mockState.startAuthenticatedDownload(...args),
}));

const apiResource = derivedFileResource("/api/files/1/download", {
	deliveryMode: "blob_url",
	scope: "personal",
});
const workspaceResource = derivedFileResource("/files/1/download", {
	deliveryMode: "blob_url",
	scope: "personal",
});

interface MockPdfPageSize {
	width: number;
	height: number;
}

function createLoadedDocument(pageSizes: MockPdfPageSize[]) {
	return {
		numPages: pageSizes.length,
		getPage: vi.fn(async (pageNumber: number) => ({
			getViewport: vi.fn(() => pageSizes[pageNumber - 1]),
		})),
	};
}

async function loadDocument(pageSizes: MockPdfPageSize[]) {
	const onDocumentLoadSuccess = mockState.documentProps?.onLoadSuccess;
	if (typeof onDocumentLoadSuccess !== "function") {
		throw new Error("document load handler was not registered");
	}
	const loadedDocument = createLoadedDocument(pageSizes);
	await act(async () => {
		await onDocumentLoadSuccess(loadedDocument);
	});
	return loadedDocument;
}

describe("PdfPreview", () => {
	beforeEach(() => {
		mockState.documentProps = null;
		mockState.pageProps = [];
		mockState.startAuthenticatedDownload.mockReset();
		mockState.startAuthenticatedDownload.mockResolvedValue(undefined);
		mockState.useBlobUrl.mockReset();
		mockState.useBlobUrl.mockReturnValue({
			blob: mockState.documentBlob,
			blobUrl: "blob:/pdf",
			error: false,
			loading: false,
			retry: vi.fn(),
		});
		mockState.virtualCount = 0;
		mockState.virtualOverscan = 0;
		mockState.estimatedSizes = [];
		mockState.virtualItems = [];
		mockState.measureElement.mockClear();
		mockState.measure.mockClear();
		mockState.scrollToIndex.mockClear();
		mockState.getTotalSize.mockClear();
		mockState.getTotalSize.mockReturnValue(0);
		mockState.virtualizerInstance = null;
		vi.spyOn(window, "open").mockImplementation(() => null);
	});

	it("loads the PDF through a blob URL and passes streaming options to the document loader", () => {
		render(<PdfPreview resource={apiResource} fileName="manual.pdf" />);

		expect(screen.getByTestId("pdf-document")).toBeInTheDocument();
		expect(mockState.useBlobUrl).toHaveBeenCalledWith(apiResource, {
			lane: "preview",
		});
		expect(mockState.documentProps).toMatchObject({
			options: {
				cMapPacked: true,
				cMapUrl: "/pdfjs/5.4.296/cmaps/",
				disableRange: false,
				disableStream: false,
				withCredentials: true,
			},
		});
		expect(mockState.documentProps?.file).toBe(mockState.documentBlob);
	});

	it("uses ordinary workspace download paths as the blob fetch key", () => {
		render(<PdfPreview resource={workspaceResource} fileName="manual.pdf" />);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith(workspaceResource, {
			lane: "preview",
		});
		expect(mockState.documentProps?.file).toBe(mockState.documentBlob);
	});

	it("renders only the virtualized page window for long documents", async () => {
		render(<PdfPreview resource={apiResource} fileName="manual.pdf" />);

		const loadedDocument = await loadDocument(
			Array.from({ length: 100 }, () => ({ width: 600, height: 800 })),
		);

		expect(screen.getByTestId("pdf-page-1")).toBeInTheDocument();
		expect(screen.getByTestId("pdf-page-7")).toBeInTheDocument();
		expect(screen.queryByTestId("pdf-page-8")).not.toBeInTheDocument();
		expect(mockState.virtualCount).toBe(100);
		expect(mockState.virtualOverscan).toBe(3);
		expect(loadedDocument.getPage).toHaveBeenCalledTimes(100);
		expect(mockState.pageProps).toHaveLength(7);
		expect(mockState.pageProps[0]).toMatchObject({
			pageNumber: 1,
			width: 800,
		});
		expect(
			screen.getByTestId("pdf-page-1").parentElement?.parentElement,
		).toHaveStyle({
			minWidth: "800px",
		});
	});

	it("uses every page size before exposing the virtual scrollbar", async () => {
		render(<PdfPreview resource={apiResource} fileName="manual.pdf" />);

		await loadDocument([
			{ width: 600, height: 800 },
			{ width: 1200, height: 800 },
			{ width: 600, height: 800 },
		]);

		expect(mockState.estimatedSizes).toEqual([1079, 546, 1079]);
		expect(mockState.getTotalSize()).toBe(2704);
		expect(
			screen.getByTestId("pdf-page-2").parentElement?.parentElement,
		).toHaveStyle({ height: "546px" });
		expect(mockState.measureElement).not.toHaveBeenCalled();
	});

	it("keeps measured page sizes while the current page changes", async () => {
		render(<PdfPreview resource={apiResource} fileName="manual.pdf" />);

		await loadDocument(
			Array.from({ length: 11 }, (_, index) =>
				index === 0 || index === 10
					? { width: 600, height: 800 }
					: { width: 1200, height: 800 },
			),
		);
		const measureCallsAfterLoad = mockState.measure.mock.calls.length;

		fireEvent.click(screen.getByLabelText("pdf_next_page"));

		expect(mockState.scrollToIndex).toHaveBeenLastCalledWith(1, {
			align: "start",
			behavior: "smooth",
		});
		expect(mockState.measure).toHaveBeenCalledTimes(measureCallsAfterLoad);

		fireEvent.click(screen.getByLabelText("pdf_rotate_right"));

		expect(mockState.measure.mock.calls.length).toBeGreaterThan(
			measureCallsAfterLoad,
		);
	});

	it("opens and downloads the loaded blob URL", () => {
		const clickSpy = vi
			.spyOn(HTMLAnchorElement.prototype, "click")
			.mockImplementation(() => undefined);
		const createElementSpy = vi.spyOn(document, "createElement");
		render(<PdfPreview resource={workspaceResource} fileName="manual.pdf" />);

		fireEvent.click(screen.getByLabelText("pdf_open_new_tab"));
		expect(window.open).toHaveBeenCalledWith(
			"blob:/pdf",
			"_blank",
			"noopener,noreferrer",
		);

		fireEvent.click(screen.getByLabelText("pdf_download"));
		const createdLinks = createElementSpy.mock.results.flatMap((result) =>
			result.value instanceof HTMLAnchorElement ? [result.value] : [],
		);
		const downloadLink = createdLinks.find((link) =>
			link.href.endsWith("blob:/pdf"),
		);
		expect(downloadLink).toBeDefined();
		expect(downloadLink?.download).toBe("manual.pdf");
		expect(clickSpy).toHaveBeenCalled();
	});

	it("uses the authenticated download fallback before the blob is ready", () => {
		mockState.useBlobUrl.mockReturnValue({
			blob: null,
			blobUrl: null,
			error: false,
			loading: true,
			retry: vi.fn(),
		});
		render(<PdfPreview resource={workspaceResource} fileName="manual.pdf" />);

		fireEvent.click(screen.getByLabelText("pdf_download"));

		expect(mockState.startAuthenticatedDownload).toHaveBeenCalledWith(
			"/files/1/download",
		);
	});

	it("refreshes the blob URL instead of reusing a failed PDF blob on retry", () => {
		let retried = false;
		const retry = vi.fn(() => {
			retried = true;
		});
		const freshBlob = new Blob(["%PDF fresh"]);
		mockState.useBlobUrl.mockImplementation(() => ({
			blob: retried ? freshBlob : mockState.documentBlob,
			blobUrl: retried ? "blob:/fresh-pdf" : "blob:/stale-pdf",
			error: false,
			loading: false,
			retry,
		}));
		const { rerender } = render(
			<PdfPreview resource={workspaceResource} fileName="manual.pdf" />,
		);

		const onLoadError = mockState.documentProps?.onLoadError;
		if (typeof onLoadError !== "function") {
			throw new Error("document error handler was not registered");
		}
		act(() => {
			onLoadError(new Error("stale blob"));
		});
		const callsBeforeRetry = mockState.useBlobUrl.mock.calls.length;

		fireEvent.click(screen.getByTestId("preview-retry"));
		rerender(<PdfPreview resource={workspaceResource} fileName="manual.pdf" />);

		expect(retry).toHaveBeenCalledTimes(1);
		expect(mockState.useBlobUrl.mock.calls.length).toBeGreaterThan(
			callsBeforeRetry,
		);
		expect(mockState.documentProps?.file).toBe(freshBlob);
	});

	it("shows a stable retry target when PDF loading fails", () => {
		const retry = vi.fn();
		mockState.useBlobUrl.mockReturnValue({
			blob: mockState.documentBlob,
			blobUrl: "blob:/stale-pdf",
			error: false,
			loading: false,
			retry,
		});
		render(<PdfPreview resource={workspaceResource} fileName="manual.pdf" />);

		const onLoadError = mockState.documentProps?.onLoadError;
		if (typeof onLoadError !== "function") {
			throw new Error("document error handler was not registered");
		}
		act(() => {
			onLoadError(new Error("stale blob"));
		});

		fireEvent.click(screen.getByTestId("preview-retry"));

		expect(retry).toHaveBeenCalledTimes(1);
	});
});
