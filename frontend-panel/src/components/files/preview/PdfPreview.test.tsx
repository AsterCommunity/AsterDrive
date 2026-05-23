import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PdfPreview } from "@/components/files/preview/PdfPreview";

const mockState = vi.hoisted(() => ({
	documentProps: null as Record<string, unknown> | null,
	pageProps: [] as Record<string, unknown>[],
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
		t: (key: string) => key,
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
	PreviewError: () => <div>preview-error</div>,
}));

describe("PdfPreview", () => {
	beforeEach(() => {
		mockState.documentProps = null;
		mockState.pageProps = [];
		mockState.virtualCount = 0;
		mockState.virtualOverscan = 0;
		mockState.virtualItems = [];
		mockState.measureElement.mockClear();
		mockState.scrollToIndex.mockClear();
		mockState.getTotalSize.mockClear();
		mockState.getTotalSize.mockReturnValue(0);
		vi.spyOn(window, "open").mockImplementation(() => null);
	});

	it("passes a credentialed URL source and streaming options to the document loader", () => {
		render(<PdfPreview path="/api/files/1/download" fileName="manual.pdf" />);

		expect(screen.getByTestId("pdf-document")).toBeInTheDocument();
		expect(mockState.documentProps).toMatchObject({
			file: {
				url: "/api/files/1/download",
			},
			options: {
				cMapPacked: true,
				cMapUrl: "/pdfjs/5.4.296/cmaps/",
				disableRange: false,
				disableStream: false,
				withCredentials: true,
			},
		});
	});

	it("joins ordinary workspace download paths with the configured API base URL", () => {
		render(<PdfPreview path="/files/1/download" fileName="manual.pdf" />);

		expect(mockState.documentProps).toMatchObject({
			file: {
				url: "/api/v1/files/1/download",
			},
		});
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

	it("opens and downloads the direct URL without a preloaded blob URL", () => {
		const clickSpy = vi
			.spyOn(HTMLAnchorElement.prototype, "click")
			.mockImplementation(() => undefined);
		const createElementSpy = vi.spyOn(document, "createElement");
		render(<PdfPreview path="/files/1/download" fileName="manual.pdf" />);

		fireEvent.click(screen.getByLabelText("pdf_open_new_tab"));
		expect(window.open).toHaveBeenCalledWith(
			"/api/v1/files/1/download",
			"_blank",
			"noopener,noreferrer",
		);

		fireEvent.click(screen.getByLabelText("pdf_download"));
		const createdLinks = createElementSpy.mock.results.flatMap((result) =>
			result.value instanceof HTMLAnchorElement ? [result.value] : [],
		);
		const downloadLink = createdLinks.find((link) =>
			link.href.endsWith("/api/v1/files/1/download"),
		);
		expect(downloadLink).toBeDefined();
		expect(downloadLink?.download).toBe("manual.pdf");
		expect(clickSpy).toHaveBeenCalled();
	});
});
