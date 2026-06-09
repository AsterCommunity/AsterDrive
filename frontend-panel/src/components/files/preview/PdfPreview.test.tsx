import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PdfPreview } from "@/components/files/preview/PdfPreview";

const mockState = vi.hoisted(() => ({
	documentBlob: new Blob(["%PDF"]),
	documentProps: null as Record<string, unknown> | null,
	pageProps: [] as Record<string, unknown>[],
	startAuthenticatedDownload: vi.fn(),
	useBlobUrl: vi.fn(),
	virtualCount: 0,
	virtualOverscan: 0,
	virtualItems: [] as {
		key: number;
		index: number;
		start: number;
		end: number;
		size: number;
	}[],
	measureElement: vi.fn(),
	scrollToIndex: vi.fn(),
	getTotalSize: vi.fn(() => 0),
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
		estimateSize: () => number;
	}) => {
		mockState.virtualCount = options.count;
		mockState.virtualOverscan = options.overscan ?? 0;
		mockState.virtualItems = Array.from(
			{ length: Math.min(options.count, 7) },
			(_, index) => {
				const size = options.estimateSize();
				return {
					key: index + 1,
					index,
					start: index * size,
					end: (index + 1) * size,
					size,
				};
			},
		);
		mockState.getTotalSize.mockImplementation(
			() => options.count * options.estimateSize(),
		);
		return {
			getVirtualItems: () => mockState.virtualItems,
			getTotalSize: mockState.getTotalSize,
			measure: vi.fn(),
			measureElement: mockState.measureElement,
			scrollToIndex: mockState.scrollToIndex,
		};
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

vi.mock("@/components/files/preview/PreviewError", () => ({
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
		mockState.virtualItems = [];
		mockState.measureElement.mockClear();
		mockState.scrollToIndex.mockClear();
		mockState.getTotalSize.mockClear();
		mockState.getTotalSize.mockReturnValue(0);
		vi.spyOn(window, "open").mockImplementation(() => null);
	});

	it("loads the PDF through a blob URL and passes streaming options to the document loader", () => {
		render(<PdfPreview path="/api/files/1/download" fileName="manual.pdf" />);

		expect(screen.getByTestId("pdf-document")).toBeInTheDocument();
		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/api/files/1/download", {
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
		render(<PdfPreview path="/files/1/download" fileName="manual.pdf" />);

		expect(mockState.useBlobUrl).toHaveBeenCalledWith("/files/1/download", {
			lane: "preview",
		});
		expect(mockState.documentProps?.file).toBe(mockState.documentBlob);
	});

	it("renders only the virtualized page window for long documents", () => {
		render(<PdfPreview path="/api/files/1/download" fileName="manual.pdf" />);

		const onDocumentLoadSuccess = mockState.documentProps?.onLoadSuccess;
		if (typeof onDocumentLoadSuccess !== "function") {
			throw new Error("document load handler was not registered");
		}
		act(() => {
			onDocumentLoadSuccess({ numPages: 100 });
		});

		expect(screen.getByTestId("pdf-page-1")).toBeInTheDocument();
		expect(screen.getByTestId("pdf-page-7")).toBeInTheDocument();
		expect(screen.queryByTestId("pdf-page-8")).not.toBeInTheDocument();
		expect(mockState.virtualCount).toBe(100);
		expect(mockState.virtualOverscan).toBe(3);
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

	it("opens and downloads the loaded blob URL", () => {
		const clickSpy = vi
			.spyOn(HTMLAnchorElement.prototype, "click")
			.mockImplementation(() => undefined);
		const createElementSpy = vi.spyOn(document, "createElement");
		render(<PdfPreview path="/files/1/download" fileName="manual.pdf" />);

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
		render(<PdfPreview path="/files/1/download" fileName="manual.pdf" />);

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
			<PdfPreview path="/files/1/download" fileName="manual.pdf" />,
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
		rerender(<PdfPreview path="/files/1/download" fileName="manual.pdf" />);

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
		render(<PdfPreview path="/files/1/download" fileName="manual.pdf" />);

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
