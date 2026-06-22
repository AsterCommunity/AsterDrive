import { useVirtualizer } from "@tanstack/react-virtual";
import {
	type ComponentProps,
	type KeyboardEvent,
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { Document, Page, pdfjs } from "react-pdf";
import "react-pdf/dist/Page/AnnotationLayer.css";
import "react-pdf/dist/Page/TextLayer.css";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { useBlobUrl } from "@/hooks/useBlobUrl";
import { startAuthenticatedDownload } from "@/lib/authenticatedDownload";
import { isImeComposingKeyEvent } from "@/lib/keyboard";
import { type ResourcePath, resourceRequestPath } from "@/lib/resourceRequest";
import { PreviewError } from "./PreviewError";
import { PreviewLoadingState } from "./PreviewLoadingState";
import { PreviewSurface, PreviewSurfaceContent } from "./PreviewSurface";

pdfjs.GlobalWorkerOptions.workerSrc = new URL(
	"pdfjs-dist/build/pdf.worker.min.mjs",
	import.meta.url,
).toString();

const pdfDocumentOptions = {
	cMapUrl: `${import.meta.env.BASE_URL}pdfjs/${pdfjs.version}/cmaps/`,
	cMapPacked: true,
	disableRange: false,
	disableStream: false,
	withCredentials: true,
} satisfies NonNullable<ComponentProps<typeof Document>["options"]>;

const MIN_ZOOM = 50;
const MAX_ZOOM = 250;
const ZOOM_STEP = 25;
const VIEWER_HORIZONTAL_PADDING = 24;
const MIN_PAGE_WIDTH = 240;
const DEFAULT_PAGE_WIDTH = 800;
const DEFAULT_PAGE_HEIGHT = 1100;
const PAGE_GAP = 12;
const VIRTUAL_PAGE_OVERSCAN = 3;

type LoadedDocument = Parameters<
	NonNullable<ComponentProps<typeof Document>["onLoadSuccess"]>
>[0];
type LoadedPage = Parameters<
	NonNullable<ComponentProps<typeof Page>["onLoadSuccess"]>
>[0];

interface PdfPreviewProps {
	resource: ResourcePath;
	fileName?: string;
}

export function PdfPreview({ resource, fileName }: PdfPreviewProps) {
	const { t } = useTranslation("files");
	const {
		blob: documentBlob,
		blobUrl: documentUrl,
		error: documentLoadError,
		loading: documentLoading,
		retry: retryDocumentLoad,
	} = useBlobUrl(resource, { lane: "preview" });
	const downloadPath = resourceRequestPath(resource);
	const documentFile = useMemo(
		() => documentBlob ?? (documentUrl ? { url: documentUrl } : null),
		[documentBlob, documentUrl],
	);
	const [reloadKey, setReloadKey] = useState(0);
	const [numPages, setNumPages] = useState<number | null>(null);
	const [pdfError, setPdfError] = useState(false);
	const [currentPage, setCurrentPage] = useState(1);
	const [pageInputValue, setPageInputValue] = useState("1");
	const [zoomPercent, setZoomPercent] = useState(100);
	const [fitWidth, setFitWidth] = useState(true);
	const [rotation, setRotation] = useState(0);
	const [pageSize, setPageSize] = useState<{
		width: number;
		height: number;
	} | null>(null);
	const [viewerWidth, setViewerWidth] = useState(0);
	const pageInputComposingRef = useRef(false);
	const pageInputCompositionEndAtRef = useRef(0);
	const scrollContainerRef = useRef<HTMLDivElement | null>(null);
	const scrollFrameRef = useRef<number | null>(null);

	const clampPageNumber = useCallback(
		(pageNumber: number) => {
			if (!numPages) return 1;
			return Math.min(Math.max(pageNumber, 1), numPages);
		},
		[numPages],
	);

	const clampZoom = useCallback((value: number) => {
		return Math.min(Math.max(value, MIN_ZOOM), MAX_ZOOM);
	}, []);

	const basePageWidth = useMemo(() => {
		if (!pageSize) return null;
		return rotation % 180 === 0 ? pageSize.width : pageSize.height;
	}, [pageSize, rotation]);
	const basePageHeight = useMemo(() => {
		if (!pageSize) return null;
		return rotation % 180 === 0 ? pageSize.height : pageSize.width;
	}, [pageSize, rotation]);

	const renderedPageWidth = useMemo(() => {
		if (fitWidth) {
			if (viewerWidth <= 0) return DEFAULT_PAGE_WIDTH;
			return Math.max(
				Math.floor(viewerWidth - VIEWER_HORIZONTAL_PADDING),
				MIN_PAGE_WIDTH,
			);
		}
		if (!basePageWidth) {
			return DEFAULT_PAGE_WIDTH;
		}
		return Math.max(
			Math.round((basePageWidth * clampZoom(zoomPercent)) / 100),
			MIN_PAGE_WIDTH,
		);
	}, [basePageWidth, clampZoom, fitWidth, viewerWidth, zoomPercent]);

	const effectiveZoomPercent = useMemo(() => {
		if (!basePageWidth) return clampZoom(zoomPercent);
		return clampZoom(Math.round((renderedPageWidth / basePageWidth) * 100));
	}, [basePageWidth, clampZoom, renderedPageWidth, zoomPercent]);
	const viewerLayoutVersion = `${renderedPageWidth}:${rotation}`;
	const estimatedPageHeight = useMemo(() => {
		const pageWidth = basePageWidth ?? DEFAULT_PAGE_WIDTH;
		const pageHeight = basePageHeight ?? DEFAULT_PAGE_HEIGHT;
		return Math.ceil((pageHeight * renderedPageWidth) / pageWidth) + PAGE_GAP;
	}, [basePageHeight, basePageWidth, renderedPageWidth]);

	const virtualizer = useVirtualizer({
		count: numPages ?? 0,
		getScrollElement: () => scrollContainerRef.current,
		estimateSize: () => estimatedPageHeight,
		getItemKey: (index) => index + 1,
		overscan: VIRTUAL_PAGE_OVERSCAN,
	});

	const onDocumentLoadSuccess = useCallback(
		({ numPages: n }: LoadedDocument) => {
			setNumPages(n);
			setPdfError(false);
			setCurrentPage(1);
			setPageInputValue("1");
			if (scrollContainerRef.current) {
				scrollContainerRef.current.scrollTop = 0;
			}
			virtualizer.scrollToIndex(0, { align: "start" });
		},
		[virtualizer],
	);

	const onDocumentLoadError = useCallback(() => {
		setNumPages(null);
		setPdfError(true);
	}, []);

	const handlePdfRetry = useCallback(() => {
		setPdfError(false);
		setReloadKey((currentKey) => currentKey + 1);
		retryDocumentLoad();
	}, [retryDocumentLoad]);

	const onPageLoadSuccess = useCallback((page: LoadedPage) => {
		setPageSize((currentSize) => {
			if (currentSize) return currentSize;
			const viewport = page.getViewport({ scale: 1 });
			return {
				width: viewport.width,
				height: viewport.height,
			};
		});
	}, []);

	const syncCurrentPageFromScroll = useCallback(() => {
		const container = scrollContainerRef.current;
		if (!container || !numPages) return;

		const virtualPages = virtualizer.getVirtualItems();
		if (virtualPages.length === 0) return;

		const viewportMidpoint = container.scrollTop + container.clientHeight / 2;
		let closestPage = currentPage;
		let closestDistance = Number.POSITIVE_INFINITY;

		for (const virtualPage of virtualPages) {
			const pageMidpoint = virtualPage.start + virtualPage.size / 2;
			const distance = Math.abs(pageMidpoint - viewportMidpoint);
			if (distance < closestDistance) {
				closestDistance = distance;
				closestPage = virtualPage.index + 1;
			}
		}

		setCurrentPage((previousPage) =>
			previousPage === closestPage ? previousPage : closestPage,
		);
	}, [currentPage, numPages, virtualizer]);

	const schedulePageSync = useCallback(() => {
		if (scrollFrameRef.current !== null) return;
		scrollFrameRef.current = window.requestAnimationFrame(() => {
			scrollFrameRef.current = null;
			syncCurrentPageFromScroll();
		});
	}, [syncCurrentPageFromScroll]);

	const scrollToPage = useCallback(
		(pageNumber: number, behavior: ScrollBehavior = "smooth") => {
			virtualizer.scrollToIndex(pageNumber - 1, {
				align: "start",
				behavior,
			});
			setCurrentPage(pageNumber);
			setPageInputValue(String(pageNumber));
		},
		[virtualizer],
	);

	const commitPageInput = useCallback(() => {
		if (!numPages) {
			setPageInputValue("1");
			return;
		}
		const parsedPage = Number.parseInt(pageInputValue, 10);
		if (!Number.isFinite(parsedPage)) {
			setPageInputValue(String(currentPage));
			return;
		}
		scrollToPage(clampPageNumber(parsedPage));
	}, [clampPageNumber, currentPage, numPages, pageInputValue, scrollToPage]);

	const handlePageInputKeyDown = useCallback(
		(event: KeyboardEvent<HTMLInputElement>) => {
			if (
				pageInputComposingRef.current ||
				isImeComposingKeyEvent(event, {
					lastCompositionEndAt: pageInputCompositionEndAtRef.current,
				})
			) {
				return;
			}

			if (event.key !== "Enter") return;
			event.preventDefault();
			commitPageInput();
		},
		[commitPageInput],
	);

	const setManualZoom = useCallback(
		(nextZoom: number) => {
			setFitWidth(false);
			setZoomPercent(clampZoom(nextZoom));
		},
		[clampZoom],
	);

	const handleZoomOut = useCallback(() => {
		setManualZoom(effectiveZoomPercent - ZOOM_STEP);
	}, [effectiveZoomPercent, setManualZoom]);

	const handleZoomIn = useCallback(() => {
		setManualZoom(effectiveZoomPercent + ZOOM_STEP);
	}, [effectiveZoomPercent, setManualZoom]);

	const handleResetZoom = useCallback(() => {
		setFitWidth(false);
		setZoomPercent(100);
	}, []);

	const handleRotateLeft = useCallback(() => {
		setRotation((currentRotation) => (currentRotation + 270) % 360);
	}, []);

	const handleRotateRight = useCallback(() => {
		setRotation((currentRotation) => (currentRotation + 90) % 360);
	}, []);

	const handleOpenInNewTab = useCallback(() => {
		if (!documentUrl) return;
		window.open(documentUrl, "_blank", "noopener,noreferrer");
	}, [documentUrl]);

	const handleDownload = useCallback(() => {
		if (!documentUrl) {
			void startAuthenticatedDownload(downloadPath);
			return;
		}
		const link = document.createElement("a");
		link.href = documentUrl;
		link.download = fileName ?? "document.pdf";
		link.click();
	}, [documentUrl, downloadPath, fileName]);

	// biome-ignore lint/correctness/useExhaustiveDependencies: documentUrl intentionally resets viewer state when the PDF source changes
	useEffect(() => {
		setNumPages(null);
		setPdfError(false);
		setCurrentPage(1);
		setPageInputValue("1");
		setZoomPercent(100);
		setFitWidth(true);
		setRotation(0);
		setPageSize(null);
		setReloadKey(0);
		if (scrollContainerRef.current) {
			setViewerWidth(scrollContainerRef.current.clientWidth);
			scrollContainerRef.current.scrollTop = 0;
		}
	}, [documentUrl]);

	useEffect(() => {
		const container = scrollContainerRef.current;
		if (!container) return;

		const updateViewerWidth = () => {
			setViewerWidth(container.clientWidth);
		};

		updateViewerWidth();

		if (typeof ResizeObserver === "undefined") return;

		const resizeObserver = new ResizeObserver(() => {
			updateViewerWidth();
		});
		resizeObserver.observe(container);

		return () => {
			resizeObserver.disconnect();
		};
	}, []);

	useEffect(() => {
		setPageInputValue(String(currentPage));
	}, [currentPage]);

	useEffect(() => {
		if (!numPages) return;
		virtualizer.measure();
		const frame = window.requestAnimationFrame(() => {
			void viewerLayoutVersion;
			syncCurrentPageFromScroll();
		});
		return () => {
			window.cancelAnimationFrame(frame);
		};
	}, [numPages, syncCurrentPageFromScroll, viewerLayoutVersion, virtualizer]);

	useEffect(() => {
		const scrollFrame = scrollFrameRef;
		return () => {
			if (scrollFrame.current !== null) {
				window.cancelAnimationFrame(scrollFrame.current);
			}
		};
	}, []);

	if (pdfError) {
		return (
			<PreviewSurface>
				<PreviewSurfaceContent>
					<PreviewError onRetry={handlePdfRetry} />
				</PreviewSurfaceContent>
			</PreviewSurface>
		);
	}

	const virtualPages = numPages !== null ? virtualizer.getVirtualItems() : [];
	const firstVirtualPage = virtualPages[0];
	const lastVirtualPage = virtualPages[virtualPages.length - 1];
	const paddingTop = firstVirtualPage?.start ?? 0;
	const paddingBottom = Math.max(
		0,
		virtualizer.getTotalSize() - (lastVirtualPage?.end ?? 0),
	);

	return (
		<PreviewSurface>
			<div className="border-b border-border/60 bg-muted/15 px-2 py-1.5 dark:bg-muted/10 md:px-2.5 md:py-2">
				<div className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-1.5 md:flex md:flex-wrap md:gap-2">
					<div className="flex items-center gap-1.5 rounded-lg bg-background/70 p-0.5 ring-1 ring-border/50 dark:bg-background/20 dark:ring-border/60">
						<Button
							variant="ghost"
							size="icon-xs"
							onClick={() => scrollToPage(clampPageNumber(currentPage - 1))}
							disabled={numPages === null || currentPage <= 1}
							title={t("pdf_previous_page")}
							aria-label={t("pdf_previous_page")}
						>
							<Icon name="CaretLeft" className="size-4" />
						</Button>
						<Input
							value={pageInputValue}
							onChange={(event) => {
								const nextValue = event.target.value.replace(/\D+/g, "");
								setPageInputValue(nextValue);
							}}
							onCompositionStart={() => {
								pageInputComposingRef.current = true;
							}}
							onCompositionEnd={(event) => {
								pageInputComposingRef.current = false;
								pageInputCompositionEndAtRef.current = Date.now();
								const nextValue = event.currentTarget.value.replace(/\D+/g, "");
								setPageInputValue(nextValue);
							}}
							onBlur={() => {
								pageInputComposingRef.current = false;
								commitPageInput();
							}}
							onKeyDown={handlePageInputKeyDown}
							inputMode="numeric"
							className="h-6 w-12 rounded-md px-1 text-center text-xs tabular-nums"
							aria-label={t("pdf_page_input_label")}
						/>
						<span className="min-w-[3rem] text-center text-[11px] text-muted-foreground tabular-nums">
							/ {numPages ?? "?"}
						</span>
						<Button
							variant="ghost"
							size="icon-xs"
							onClick={() => scrollToPage(clampPageNumber(currentPage + 1))}
							disabled={
								numPages === null ||
								(numPages !== null && currentPage >= numPages)
							}
							title={t("pdf_next_page")}
							aria-label={t("pdf_next_page")}
						>
							<Icon name="CaretRight" className="size-4" />
						</Button>
					</div>

					<div className="flex min-w-0 items-center justify-center gap-1 rounded-lg bg-background/70 p-0.5 ring-1 ring-border/50 dark:bg-background/20 dark:ring-border/60 md:gap-1.5">
						<Button
							variant="ghost"
							size="icon-xs"
							onClick={handleZoomOut}
							disabled={effectiveZoomPercent <= MIN_ZOOM}
							title={t("pdf_zoom_out")}
							aria-label={t("pdf_zoom_out")}
						>
							<Icon name="Minus" className="size-4" />
						</Button>
						<Button
							variant="ghost"
							size="xs"
							onClick={handleResetZoom}
							title={t("pdf_zoom_reset")}
							aria-label={t("pdf_zoom_reset")}
							className="min-w-[3.25rem] justify-center px-1.5 tabular-nums md:min-w-[4rem] md:px-2"
						>
							{t("pdf_zoom_percent", { zoom: effectiveZoomPercent })}
						</Button>
						<Button
							variant="ghost"
							size="icon-xs"
							onClick={handleZoomIn}
							disabled={effectiveZoomPercent >= MAX_ZOOM}
							title={t("pdf_zoom_in")}
							aria-label={t("pdf_zoom_in")}
						>
							<Icon name="Plus" className="size-4" />
						</Button>
						<Button
							variant={fitWidth ? "secondary" : "ghost"}
							size="xs"
							onClick={() => setFitWidth(true)}
							className="hidden md:inline-flex"
						>
							{t("pdf_fit_width")}
						</Button>
					</div>

					<div className="hidden items-center gap-1.5 rounded-lg bg-background/70 p-0.5 ring-1 ring-border/50 dark:bg-background/20 dark:ring-border/60 md:flex">
						<Button
							variant="ghost"
							size="icon-xs"
							onClick={handleRotateLeft}
							title={t("pdf_rotate_left")}
							aria-label={t("pdf_rotate_left")}
						>
							<Icon name="ArrowCounterClockwise" className="size-4" />
						</Button>
						<Button
							variant="ghost"
							size="icon-xs"
							onClick={handleRotateRight}
							title={t("pdf_rotate_right")}
							aria-label={t("pdf_rotate_right")}
						>
							<Icon name="ArrowClockwise" className="size-4" />
						</Button>
					</div>

					<div className="hidden items-center gap-1.5 rounded-lg bg-background/70 p-0.5 ring-1 ring-border/50 dark:bg-background/20 dark:ring-border/60 md:ml-auto md:flex">
						<Button
							variant="ghost"
							size="icon-xs"
							onClick={handleOpenInNewTab}
							disabled={!documentUrl}
							title={t("pdf_open_new_tab")}
							aria-label={t("pdf_open_new_tab")}
						>
							<Icon name="ArrowSquareOut" className="size-4" />
						</Button>
						<Button
							variant="ghost"
							size="icon-xs"
							onClick={handleDownload}
							title={t("pdf_download")}
							aria-label={t("pdf_download")}
						>
							<Icon name="Download" className="size-4" />
						</Button>
					</div>
					<div className="flex items-center rounded-lg bg-background/70 p-0.5 ring-1 ring-border/50 dark:bg-background/20 dark:ring-border/60 md:hidden">
						<DropdownMenu>
							<DropdownMenuTrigger
								render={
									<Button
										variant="ghost"
										size="icon-xs"
										title={t("pdf_more_actions")}
										aria-label={t("pdf_more_actions")}
									>
										<Icon name="DotsThree" className="size-4" />
									</Button>
								}
							/>
							<DropdownMenuContent align="end" className="w-44">
								<DropdownMenuItem onClick={() => setFitWidth(true)}>
									<Icon name="ArrowsOutCardinal" className="size-4" />
									{t("pdf_fit_width")}
								</DropdownMenuItem>
								<DropdownMenuItem onClick={handleRotateLeft}>
									<Icon name="ArrowCounterClockwise" className="size-4" />
									{t("pdf_rotate_left")}
								</DropdownMenuItem>
								<DropdownMenuItem onClick={handleRotateRight}>
									<Icon name="ArrowClockwise" className="size-4" />
									{t("pdf_rotate_right")}
								</DropdownMenuItem>
								<DropdownMenuItem
									onClick={handleOpenInNewTab}
									disabled={!documentUrl}
								>
									<Icon name="ArrowSquareOut" className="size-4" />
									{t("pdf_open_new_tab")}
								</DropdownMenuItem>
								<DropdownMenuItem onClick={handleDownload}>
									<Icon name="Download" className="size-4" />
									{t("pdf_download")}
								</DropdownMenuItem>
							</DropdownMenuContent>
						</DropdownMenu>
					</div>
				</div>
			</div>
			<PreviewSurfaceContent>
				<div
					ref={scrollContainerRef}
					onScroll={schedulePageSync}
					className="h-full min-h-0 touch-pan-x touch-pan-y overflow-auto bg-background/80 p-2 dark:bg-background/25 md:p-3"
				>
					{documentLoadError ? (
						<PreviewError onRetry={retryDocumentLoad} />
					) : documentLoading || !documentFile ? (
						<PreviewLoadingState
							text={t("loading_preview")}
							className="h-full"
						/>
					) : (
						<Document
							key={`${documentUrl}:${reloadKey}`}
							file={documentFile}
							options={pdfDocumentOptions}
							onLoadSuccess={onDocumentLoadSuccess}
							onLoadError={onDocumentLoadError}
							loading={
								<div className="p-6 text-sm text-muted-foreground">
									{t("loading_preview")}
								</div>
							}
						>
							{numPages !== null && (
								<div className="w-full" style={{ minWidth: renderedPageWidth }}>
									{paddingTop > 0 && (
										<div aria-hidden style={{ height: paddingTop }} />
									)}
									{virtualPages.map((virtualPage) => {
										const pageNumber = virtualPage.index + 1;
										return (
											<div
												key={virtualPage.key}
												ref={(node) => {
													if (node) {
														virtualizer.measureElement(node);
													}
												}}
												data-index={virtualPage.index}
												className="flex justify-center pb-3"
												style={{ minWidth: renderedPageWidth }}
											>
												<div className="overflow-hidden rounded-lg bg-white ring-1 ring-black/5">
													<Page
														pageNumber={pageNumber}
														width={renderedPageWidth}
														rotate={rotation}
														onLoadSuccess={onPageLoadSuccess}
														loading={
															<div className="flex h-[250px] w-[200px] items-center justify-center bg-white">
																<span className="text-sm text-muted-foreground">
																	{t("loading_preview")}
																</span>
															</div>
														}
													/>
												</div>
											</div>
										);
									})}
									{paddingBottom > 0 && (
										<div aria-hidden style={{ height: paddingBottom }} />
									)}
								</div>
							)}
						</Document>
					)}
				</div>
			</PreviewSurfaceContent>
		</PreviewSurface>
	);
}
