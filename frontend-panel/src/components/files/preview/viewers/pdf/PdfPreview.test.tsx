import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { Suspense } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PdfPreview } from "@/components/files/preview/viewers/pdf/PdfPreview";
import { derivedFileResource } from "@/lib/fileResource";

const mockState = vi.hoisted(() => ({
	documentBlob: new Blob(["%PDF"]),
	documentLoadOnCommit: null as ReturnType<typeof createLoadedDocument> | null,
	documentRenderError: null as Error | null,
	documentProps: null as Record<string, unknown> | null,
	pageSuspends: new Set<number>(),
	pageSuspensePromise: new Promise<never>(() => undefined),
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
	scrollToOffset: vi.fn(),
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
			scrollToOffset: mockState.scrollToOffset,
		};
		return mockState.virtualizerInstance;
	},
}));

vi.mock("react-pdf", async () => {
	const { useLayoutEffect } =
		await vi.importActual<typeof import("react")>("react");
	const pdfjs = {
		GlobalWorkerOptions: {},
		version: "6.3.289",
	};

	return {
		Document: ({
			children,
			...props
		}: Record<string, unknown> & { children?: React.ReactNode }) => {
			mockState.documentProps = props;
			useLayoutEffect(() => {
				const loadedDocument = mockState.documentLoadOnCommit;
				const onLoadSuccess = props.onLoadSuccess;
				if (loadedDocument && typeof onLoadSuccess === "function") {
					void onLoadSuccess(loadedDocument);
				}
			}, [props.onLoadSuccess]);
			if (mockState.documentRenderError) {
				throw mockState.documentRenderError;
			}
			return <div data-testid="pdf-document">{children}</div>;
		},
		Page: (props: Record<string, unknown>) => {
			mockState.pageProps.push(props);
			if (mockState.pageSuspends.has(Number(props.pageNumber))) {
				throw mockState.pageSuspensePromise;
			}
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
		mockState.documentLoadOnCommit = null;
		mockState.documentRenderError = null;
		mockState.documentProps = null;
		mockState.pageSuspends.clear();
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
		mockState.scrollToOffset.mockClear();
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
			suspense: true,
			options: {
				cMapPacked: true,
				cMapUrl: "/pdfjs/6.3.289/cmaps/",
				disableRange: false,
				disableStream: false,
				withCredentials: true,
			},
		});
		expect(mockState.documentProps?.file).toBe(mockState.documentBlob);
	});

	it("keeps document state when Suspense resolves during the commit phase", async () => {
		mockState.documentLoadOnCommit = createLoadedDocument([
			{ width: 600, height: 800 },
		]);

		render(<PdfPreview resource={apiResource} fileName="manual.pdf" />);

		await waitFor(() => {
			expect(screen.getByText("/ 1")).toBeInTheDocument();
		});
		expect(screen.getByTestId("pdf-page-1")).toBeInTheDocument();
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
			suspense: true,
			width: 800,
		});
		expect(
			screen.getByTestId("pdf-page-1").parentElement?.parentElement,
		).toHaveStyle({
			minWidth: "800px",
		});
	});

	it("keeps the preview visible while an individual page suspends", async () => {
		mockState.pageSuspends.add(2);
		render(
			<Suspense fallback={<div data-testid="global-pdf-loading" />}>
				<PdfPreview resource={apiResource} fileName="manual.pdf" />
			</Suspense>,
		);

		await loadDocument([
			{ width: 600, height: 800 },
			{ width: 600, height: 800 },
			{ width: 600, height: 800 },
		]);

		expect(screen.queryByTestId("global-pdf-loading")).not.toBeInTheDocument();
		expect(screen.getByTestId("pdf-page-1")).toBeInTheDocument();
		expect(screen.queryByTestId("pdf-page-2")).not.toBeInTheDocument();
		expect(screen.getByText("loading_preview")).toBeInTheDocument();
		expect(screen.getByLabelText("pdf_download")).toBeInTheDocument();
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

	it("restores the same page offset after rotating in either direction", async () => {
		const requestAnimationFrame = vi
			.spyOn(window, "requestAnimationFrame")
			.mockImplementation((callback) => {
				callback(0);
				return 0;
			});
		render(<PdfPreview resource={apiResource} fileName="manual.pdf" />);
		await loadDocument(
			Array.from({ length: 11 }, (_, index) =>
				index === 0 || index === 10
					? { width: 600, height: 800 }
					: { width: 1200, height: 800 },
			),
		);

		const scrollContainer = screen.getByTestId("pdf-document")
			.parentElement as HTMLElement;
		const pageOffset = 75;
		const originalPageStart = mockState.estimatedSizes
			.slice(0, 4)
			.reduce((total, size) => total + size, 0);
		scrollContainer.scrollTop = originalPageStart + pageOffset;

		fireEvent.click(screen.getByLabelText("pdf_rotate_right"));
		const rotatedPageStart = mockState.estimatedSizes
			.slice(0, 4)
			.reduce((total, size) => total + size, 0);
		expect(mockState.scrollToOffset).toHaveBeenLastCalledWith(
			rotatedPageStart + pageOffset,
			{
				behavior: "auto",
			},
		);
		scrollContainer.scrollTop = rotatedPageStart + pageOffset;

		fireEvent.click(screen.getByLabelText("pdf_rotate_left"));
		expect(mockState.scrollToOffset).toHaveBeenLastCalledWith(
			originalPageStart + pageOffset,
			{ behavior: "auto" },
		);
		requestAnimationFrame.mockRestore();
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
			mockState.documentRenderError = null;
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
		mockState.documentRenderError = new Error("stale blob");
		rerender(<PdfPreview resource={workspaceResource} fileName="manual.pdf" />);
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
		mockState.documentRenderError = new Error("stale blob");
		render(<PdfPreview resource={workspaceResource} fileName="manual.pdf" />);

		fireEvent.click(screen.getByTestId("preview-retry"));

		expect(retry).toHaveBeenCalledTimes(1);
	});
});
