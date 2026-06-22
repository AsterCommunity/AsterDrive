import {
	type MouseEvent,
	type PointerEvent,
	useCallback,
	useEffect,
	useReducer,
	useRef,
	useState,
} from "react";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { useBlobUrl } from "@/hooks/useBlobUrl";
import { useFileContentResource } from "@/hooks/useFileResource";
import { canBrowserRenderImage } from "@/lib/browserImageSupport";
import { formatBytes } from "@/lib/format";
import { shouldIgnoreKeyboardTarget } from "@/lib/keyboard";
import type { ResourcePath } from "@/lib/resourceRequest";
import { cn } from "@/lib/utils";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import type { FileInfo, FileListItem } from "@/types/api";
import {
	BlobImagePreview,
	type ImagePreviewSource,
	type ShowOriginalState,
} from "./BlobImagePreview";
import type { FilePreviewResources } from "./filePreviewResources";
import { useImagePreviewTransform } from "./useImagePreviewTransform";

const ORIGINAL_BUTTON_EXIT_MS = 220;
const CLICK_MOVE_THRESHOLD = 4;
const IMAGE_PREVIEW_KEY_SEPARATOR = "\u0000";
const ORIGINAL_BUTTON_SUCCESS_HOLD_MS = 650;
const IMAGE_NAVIGATION_KEYDOWN_OPTIONS = { capture: true } as const;
const IMAGE_PREVIEW_INTERACTIVE_SELECTOR = [
	"button",
	"a[href]",
	"input",
	"textarea",
	"select",
	"[role='button']",
	"[role='link']",
	"[contenteditable='true']",
	"[contenteditable='plaintext-only']",
].join(",");

type OriginalButtonAnimationPhase =
	| "hidden"
	| "visible"
	| "exiting"
	| "dismissed";

interface OriginalButtonAnimationState {
	phase: OriginalButtonAnimationPhase;
}

type OriginalButtonAnimationAction =
	| { type: "show" }
	| { type: "startSuccessExit" }
	| { type: "dismissSuccess" }
	| { type: "hide" };

const initialOriginalButtonAnimationState: OriginalButtonAnimationState = {
	phase: "hidden",
};

function originalButtonAnimationReducer(
	state: OriginalButtonAnimationState,
	action: OriginalButtonAnimationAction,
): OriginalButtonAnimationState {
	switch (action.type) {
		case "show":
			return state.phase === "visible" ? state : { phase: "visible" };
		case "startSuccessExit":
			return state.phase === "exiting" || state.phase === "dismissed"
				? state
				: { phase: "exiting" };
		case "dismissSuccess":
			return state.phase === "dismissed" ? state : { phase: "dismissed" };
		case "hide":
			return state.phase === "hidden" ? state : { phase: "hidden" };
	}
}

function isImagePreviewInteractiveTarget(target: EventTarget | null) {
	if (!(target instanceof Element)) return false;

	return (
		(target instanceof HTMLElement && target.isContentEditable) ||
		target.closest(IMAGE_PREVIEW_INTERACTIVE_SELECTOR) !== null
	);
}

interface ImagePreviewPanelProps {
	file: FileInfo | FileListItem;
	allOptionsCount: number;
	resources: FilePreviewResources;
	nextImageFile?: FileInfo | FileListItem;
	onChooseOpenMethod: () => void;
	onClose: () => void;
	onNavigateImage?: (file: FileInfo | FileListItem) => void;
	previousImageFile?: FileInfo | FileListItem;
	chooseOpenMethodLabel: string;
	closeLabel: string;
	fitToWindowLabel: string;
	nextImageLabel: string;
	previousImageLabel: string;
	previewSourceLabel: string;
	originalSourceLabel: string;
	rotateRightLabel: string;
	zoomInLabel: string;
	zoomOutLabel: string;
}

export function ImagePreviewPanel({
	file,
	allOptionsCount,
	resources,
	nextImageFile,
	onChooseOpenMethod,
	onClose,
	onNavigateImage,
	previousImageFile,
	chooseOpenMethodLabel,
	closeLabel,
	fitToWindowLabel,
	nextImageLabel,
	previousImageLabel,
	previewSourceLabel,
	originalSourceLabel,
	rotateRightLabel,
	zoomInLabel,
	zoomOutLabel,
}: ImagePreviewPanelProps) {
	const imagePreviewPreference = useFrontendConfigStore(
		(state) => state.imagePreviewPreference,
	);
	const originalIsBrowserRenderable = canBrowserRenderImage(file);
	const hasBackendPreview = resources.paths.imagePreview != null;
	const initialSource =
		hasBackendPreview &&
		(imagePreviewPreference === "preview_first" || !originalIsBrowserRenderable)
			? "backend_preview"
			: "original";
	const canRequestOriginal =
		hasBackendPreview &&
		imagePreviewPreference === "preview_first" &&
		originalIsBrowserRenderable;
	const [originalButtonAnimation, dispatchOriginalButtonAnimation] = useReducer(
		originalButtonAnimationReducer,
		initialOriginalButtonAnimationState,
	);
	const [requestedOriginalKey, setRequestedOriginalKey] = useState<
		string | null
	>(null);
	const [renderedOriginalKey, setRenderedOriginalKey] = useState<string | null>(
		null,
	);
	const [failedOriginalKey, setFailedOriginalKey] = useState<string | null>(
		null,
	);
	const [loadedImageRenderKey, setLoadedImageRenderKey] = useState<
		string | null
	>(null);
	const previewKey = [
		file.name,
		file.mime_type,
		resources.paths.download,
		resources.paths.imagePreview ?? "",
		imagePreviewPreference,
	].join(IMAGE_PREVIEW_KEY_SEPARATOR);
	const requestedOriginal = requestedOriginalKey === previewKey;
	const shouldResolveOriginalResource =
		initialSource === "original" || (canRequestOriginal && requestedOriginal);
	const originalDownloadPath = useFileContentResource({
		deliveryMode: "blob_url",
		downloadPath: resources.paths.download,
		enabled: shouldResolveOriginalResource,
		fileId: file.id,
		mimeType: file.mime_type,
		open: true,
		representation: "original",
		resolveResourceHandle: resources.resolve,
	});
	const backendPreviewResource = useFileContentResource({
		deliveryMode: "blob_url",
		downloadPath: resources.paths.download,
		enabled: hasBackendPreview,
		fileId: file.id,
		mimeType: file.mime_type,
		open: true,
		representation: "image_preview",
		resolveResourceHandle: resources.resolve,
	});
	const {
		blobUrl: originalBlobUrl,
		error: originalError,
		loading: originalLoading,
		retry: retryOriginal,
	} = useBlobUrl(
		canRequestOriginal && requestedOriginal ? originalDownloadPath : null,
		{ lane: "default" },
	);
	const originalReady =
		requestedOriginal && originalBlobUrl && !originalLoading && !originalError;
	const originalRenderedSuccessfully = renderedOriginalKey === previewKey;
	const originalRenderFailed = failedOriginalKey === previewKey;
	const shouldFallbackToBackendPreview =
		originalRenderFailed && hasBackendPreview;
	const effectiveSource: ImagePreviewSource = shouldFallbackToBackendPreview
		? "backend_preview"
		: originalReady && !originalRenderFailed
			? "original"
			: initialSource;
	const effectiveShowOriginalState: ShowOriginalState = canRequestOriginal
		? originalRenderedSuccessfully
			? "success"
			: requestedOriginal && !originalError && !originalRenderFailed
				? "loading"
				: "available"
		: "hidden";
	const activeImageRenderKey = [previewKey, effectiveSource].join(
		IMAGE_PREVIEW_KEY_SEPARATOR,
	);
	const imageGesturesEnabled = loadedImageRenderKey === activeImageRenderKey;

	const sourceLabel =
		effectiveSource === "backend_preview"
			? previewSourceLabel
			: originalSourceLabel;

	const showOriginal = useCallback(() => {
		setFailedOriginalKey(null);
		if (requestedOriginal && originalError) {
			retryOriginal();
			return;
		}
		setRequestedOriginalKey(previewKey);
	}, [originalError, previewKey, requestedOriginal, retryOriginal]);

	const handleImageLoad = useCallback(
		(renderedSource: ImagePreviewSource) => {
			setLoadedImageRenderKey(
				[previewKey, renderedSource].join(IMAGE_PREVIEW_KEY_SEPARATOR),
			);
			if (renderedSource !== "original") return;
			setFailedOriginalKey(null);
			setRenderedOriginalKey(previewKey);
		},
		[previewKey],
	);

	const handleImageRenderError = useCallback(
		(failedSource: ImagePreviewSource) => {
			setLoadedImageRenderKey(null);
			if (failedSource !== "original") return;
			setFailedOriginalKey(previewKey);
			setRenderedOriginalKey(null);
			setRequestedOriginalKey(null);
		},
		[previewKey],
	);

	useEffect(() => {
		if (
			effectiveShowOriginalState === "success" &&
			originalButtonAnimation.phase === "exiting"
		) {
			const timer = window.setTimeout(() => {
				dispatchOriginalButtonAnimation({ type: "dismissSuccess" });
			}, ORIGINAL_BUTTON_EXIT_MS);

			return () => {
				window.clearTimeout(timer);
			};
		}

		if (
			effectiveShowOriginalState === "available" ||
			effectiveShowOriginalState === "loading"
		) {
			dispatchOriginalButtonAnimation({ type: "show" });
			return;
		}

		if (effectiveShowOriginalState === "success") {
			if (originalButtonAnimation.phase === "dismissed") {
				return;
			}

			const successTimer = window.setTimeout(() => {
				dispatchOriginalButtonAnimation({ type: "startSuccessExit" });
			}, ORIGINAL_BUTTON_SUCCESS_HOLD_MS);
			return () => {
				window.clearTimeout(successTimer);
			};
		}

		const timer = window.setTimeout(() => {
			dispatchOriginalButtonAnimation({ type: "hide" });
		}, ORIGINAL_BUTTON_EXIT_MS);

		return () => {
			window.clearTimeout(timer);
		};
	}, [effectiveShowOriginalState, originalButtonAnimation.phase]);

	useEffect(() => {
		if (!onNavigateImage || (!previousImageFile && !nextImageFile)) return;

		const handleKeyDown = (event: KeyboardEvent) => {
			if (event.defaultPrevented) return;
			if (shouldIgnoreKeyboardTarget(event.target)) return;
			if (event.key === "ArrowLeft" && previousImageFile) {
				event.preventDefault();
				onNavigateImage(previousImageFile);
				return;
			}
			if (event.key === "ArrowRight" && nextImageFile) {
				event.preventDefault();
				onNavigateImage(nextImageFile);
			}
		};

		window.addEventListener(
			"keydown",
			handleKeyDown,
			IMAGE_NAVIGATION_KEYDOWN_OPTIONS,
		);
		return () =>
			window.removeEventListener(
				"keydown",
				handleKeyDown,
				IMAGE_NAVIGATION_KEYDOWN_OPTIONS,
			);
	}, [nextImageFile, onNavigateImage, previousImageFile]);

	const originalButtonVisible =
		effectiveShowOriginalState === "available" ||
		effectiveShowOriginalState === "loading" ||
		(effectiveShowOriginalState === "success" &&
			originalButtonAnimation.phase === "visible");
	const renderOriginalButton =
		originalButtonAnimation.phase === "visible" ||
		originalButtonAnimation.phase === "exiting";
	const originalButtonDisabled =
		effectiveShowOriginalState === "loading" ||
		effectiveShowOriginalState === "success";
	const originalButtonIcon =
		effectiveShowOriginalState === "loading"
			? "Spinner"
			: effectiveShowOriginalState === "success"
				? "Check"
				: "Eye";

	return (
		<div className="relative flex h-full min-h-0 flex-col overflow-hidden bg-zinc-950 text-white">
			<ImagePreviewTopChrome
				file={file}
				allOptionsCount={allOptionsCount}
				sourceLabel={sourceLabel}
				chooseOpenMethodLabel={chooseOpenMethodLabel}
				closeLabel={closeLabel}
				onChooseOpenMethod={onChooseOpenMethod}
				onClose={onClose}
			/>

			<ImagePreviewTransformLayer
				key={activeImageRenderKey}
				originalDownloadPath={originalDownloadPath}
				backendPreviewResource={backendPreviewResource}
				effectiveShowOriginalState={effectiveShowOriginalState}
				effectiveSource={effectiveSource}
				file={file}
				fitToWindowLabel={fitToWindowLabel}
				imageGesturesEnabled={imageGesturesEnabled}
				originalButtonDisabled={originalButtonDisabled}
				originalButtonIcon={originalButtonIcon}
				originalButtonVisible={originalButtonVisible}
				originalSourceLabel={originalSourceLabel}
				previewKey={previewKey}
				renderOriginalButton={renderOriginalButton}
				rotateRightLabel={rotateRightLabel}
				showOriginal={showOriginal}
				zoomInLabel={zoomInLabel}
				zoomOutLabel={zoomOutLabel}
				onClose={onClose}
				onImageLoad={handleImageLoad}
				onImageRenderError={handleImageRenderError}
			/>

			<ImagePreviewSideNavigation
				nextFile={nextImageFile}
				nextLabel={nextImageLabel}
				previousFile={previousImageFile}
				previousLabel={previousImageLabel}
				onNavigate={onNavigateImage}
			/>
		</div>
	);
}

function ImagePreviewTransformLayer({
	originalDownloadPath,
	backendPreviewResource,
	effectiveShowOriginalState,
	effectiveSource,
	file,
	fitToWindowLabel,
	imageGesturesEnabled,
	originalButtonDisabled,
	originalButtonIcon,
	originalButtonVisible,
	originalSourceLabel,
	previewKey,
	renderOriginalButton,
	rotateRightLabel,
	showOriginal,
	zoomInLabel,
	zoomOutLabel,
	onClose,
	onImageLoad,
	onImageRenderError,
}: {
	originalDownloadPath: ResourcePath | null;
	backendPreviewResource: ResourcePath | null;
	effectiveShowOriginalState: ShowOriginalState;
	effectiveSource: ImagePreviewSource;
	file: FileInfo | FileListItem;
	fitToWindowLabel: string;
	imageGesturesEnabled: boolean;
	originalButtonDisabled: boolean;
	originalButtonIcon: "Check" | "Eye" | "Spinner";
	originalButtonVisible: boolean;
	originalSourceLabel: string;
	previewKey: string;
	renderOriginalButton: boolean;
	rotateRightLabel: string;
	showOriginal: () => void;
	zoomInLabel: string;
	zoomOutLabel: string;
	onClose: () => void;
	onImageLoad: (renderedSource: ImagePreviewSource) => void;
	onImageRenderError: (failedSource: ImagePreviewSource) => void;
}) {
	const gestureSurfaceRef = useRef<HTMLDivElement | null>(null);
	const imageRef = useRef<HTMLImageElement | null>(null);
	const surfacePointerStartRef = useRef<{
		targetIsInteractive: boolean;
		targetIsImage: boolean;
		x: number;
		y: number;
	} | null>(null);
	const viewportRef = useRef<HTMLDivElement | null>(null);
	const {
		canZoomIn,
		canZoomOut,
		handlePointerDown,
		handlePointerEnd,
		handlePointerMove,
		imageStyle,
		resetImageTransform,
		rotateRight,
		zoom,
		zoomIn,
		zoomOut,
		zoomPercent,
	} = useImagePreviewTransform({
		gestureSurfaceRef,
		gesturesEnabled: imageGesturesEnabled,
		imageRef,
		viewportRef,
	});
	const handleSurfacePointerDown = useCallback(
		(event: PointerEvent<HTMLDivElement>) => {
			handlePointerDown(event);
			surfacePointerStartRef.current = {
				targetIsInteractive: isImagePreviewInteractiveTarget(event.target),
				targetIsImage: event.target === imageRef.current,
				x: event.clientX,
				y: event.clientY,
			};
		},
		[handlePointerDown],
	);
	const handleSurfaceClick = useCallback(
		(event: MouseEvent<HTMLDivElement>) => {
			const pointerStart = surfacePointerStartRef.current;
			surfacePointerStartRef.current = null;

			if (
				isImagePreviewInteractiveTarget(event.target) ||
				pointerStart?.targetIsInteractive
			) {
				return;
			}

			if (event.target === imageRef.current || pointerStart?.targetIsImage) {
				return;
			}

			if (
				pointerStart &&
				Math.hypot(
					event.clientX - pointerStart.x,
					event.clientY - pointerStart.y,
				) > CLICK_MOVE_THRESHOLD
			) {
				return;
			}

			onClose();
		},
		[onClose],
	);

	return (
		<>
			<div className="min-h-0 flex-1 scale-[0.985] overflow-hidden opacity-0 transition-[opacity,transform] duration-200 ease-out group-data-open/image-preview:scale-100 group-data-open/image-preview:opacity-100 group-data-closed/image-preview:scale-[0.985] group-data-closed/image-preview:opacity-0">
				{/* biome-ignore lint/a11y/noStaticElementInteractions: This is an image gesture surface; keyboard users use the explicit close button. */}
				<div
					ref={gestureSurfaceRef}
					role="presentation"
					className={cn(
						"h-full min-h-0 w-full touch-none select-none overflow-hidden",
						zoom > 1 ? "cursor-grab active:cursor-grabbing" : "cursor-default",
					)}
					onPointerDown={handleSurfacePointerDown}
					onPointerMove={handlePointerMove}
					onPointerUp={handlePointerEnd}
					onPointerCancel={handlePointerEnd}
					onClick={handleSurfaceClick}
				>
					<BlobImagePreview
						file={file}
						fillContainer
						resource={originalDownloadPath}
						fallbackResource={backendPreviewResource}
						imageRef={imageRef}
						viewportRef={viewportRef}
						source={effectiveSource}
						showOriginalButtonPlacement="none"
						onImageLoad={onImageLoad}
						onImageRenderError={onImageRenderError}
						viewportClassName="flex h-full min-h-0 w-full items-center justify-center overflow-hidden px-4 py-16 sm:px-8"
						imageClassName="block max-h-full max-w-full min-w-0 touch-none select-none object-contain"
						imageStyle={imageStyle}
						key={previewKey}
					/>
				</div>
			</div>

			<ImagePreviewBottomChrome
				canZoomIn={canZoomIn}
				canZoomOut={canZoomOut}
				fitToWindowLabel={fitToWindowLabel}
				originalButtonDisabled={originalButtonDisabled}
				originalButtonIcon={originalButtonIcon}
				originalButtonVisible={originalButtonVisible}
				originalSourceLabel={originalSourceLabel}
				renderOriginalButton={renderOriginalButton}
				showOriginalState={effectiveShowOriginalState}
				rotateRightLabel={rotateRightLabel}
				zoomInLabel={zoomInLabel}
				zoomOutLabel={zoomOutLabel}
				zoomPercent={zoomPercent}
				onRotateRight={rotateRight}
				onShowOriginal={showOriginal}
				onZoomIn={zoomIn}
				onZoomOut={zoomOut}
				onFitToWindow={resetImageTransform}
			/>
		</>
	);
}

function ImagePreviewTopChrome({
	file,
	allOptionsCount,
	sourceLabel,
	chooseOpenMethodLabel,
	closeLabel,
	onChooseOpenMethod,
	onClose,
}: {
	file: FileInfo | FileListItem;
	allOptionsCount: number;
	sourceLabel: string;
	chooseOpenMethodLabel: string;
	closeLabel: string;
	onChooseOpenMethod: () => void;
	onClose: () => void;
}) {
	return (
		<div className="absolute inset-x-0 top-0 z-10 bg-linear-to-b from-black/78 via-black/36 to-transparent px-3 pt-3 pb-8 opacity-0 transition-opacity duration-200 ease-out group-data-open/image-preview:opacity-100 group-data-closed/image-preview:opacity-0 sm:px-4">
			<div className="flex min-w-0 items-start gap-3">
				<div className="min-w-0 flex-1">
					<div className="flex min-w-0 items-center gap-2">
						<h2 className="min-w-0 truncate text-sm font-medium leading-6 text-white">
							{file.name}
						</h2>
						<span className="shrink-0 rounded-full border border-white/12 bg-white/10 px-2 py-0.5 text-[0.7rem] font-medium text-white/80">
							{sourceLabel}
						</span>
					</div>
					<p className="mt-0.5 truncate text-xs text-white/56">
						{formatBytes(file.size)}
						{file.mime_type ? ` · ${file.mime_type}` : ""}
					</p>
				</div>

				<div className="flex shrink-0 items-center gap-1 rounded-xl border border-white/10 bg-black/32 p-1 shadow-lg shadow-black/20 backdrop-blur-md">
					{allOptionsCount > 1 ? (
						<ToolbarButton
							label={chooseOpenMethodLabel}
							onClick={onChooseOpenMethod}
							icon="DotsThree"
						/>
					) : null}
					<ToolbarButton label={closeLabel} onClick={onClose} icon="X" />
				</div>
			</div>
		</div>
	);
}

function ImagePreviewSideNavigation({
	nextFile,
	nextLabel,
	onNavigate,
	previousFile,
	previousLabel,
}: {
	nextFile?: FileInfo | FileListItem;
	nextLabel: string;
	onNavigate?: (file: FileInfo | FileListItem) => void;
	previousFile?: FileInfo | FileListItem;
	previousLabel: string;
}) {
	if (!onNavigate || (!previousFile && !nextFile)) return null;

	return (
		<div className="pointer-events-none absolute inset-x-0 top-16 bottom-16 z-10 flex items-center justify-between px-2 opacity-0 transition-opacity duration-200 ease-out group-data-open/image-preview:opacity-100 group-data-closed/image-preview:opacity-0 sm:px-4">
			{previousFile ? (
				<ImageNavigationButton
					file={previousFile}
					icon="CaretLeft"
					label={previousLabel}
					onNavigate={onNavigate}
				/>
			) : (
				<span className="size-11" aria-hidden="true" />
			)}
			{nextFile ? (
				<ImageNavigationButton
					file={nextFile}
					icon="CaretRight"
					label={nextLabel}
					onNavigate={onNavigate}
				/>
			) : (
				<span className="size-11" aria-hidden="true" />
			)}
		</div>
	);
}

function ImageNavigationButton({
	file,
	icon,
	label,
	onNavigate,
}: {
	file: FileInfo | FileListItem;
	icon: "CaretLeft" | "CaretRight";
	label: string;
	onNavigate: (file: FileInfo | FileListItem) => void;
}) {
	return (
		<Button
			type="button"
			variant="ghost"
			size="icon"
			aria-label={label}
			title={file.name}
			className="pointer-events-auto size-11 rounded-full border border-white/10 bg-black/38 text-white/82 shadow-lg shadow-black/25 backdrop-blur-md hover:bg-white/14 hover:text-white focus-visible:ring-white/40"
			onClick={() => onNavigate(file)}
		>
			<Icon name={icon} className="size-5" />
			<span className="sr-only">{label}</span>
		</Button>
	);
}

function ImagePreviewBottomChrome({
	canZoomIn,
	canZoomOut,
	fitToWindowLabel,
	originalButtonDisabled,
	originalButtonIcon,
	originalButtonVisible,
	originalSourceLabel,
	renderOriginalButton,
	showOriginalState,
	rotateRightLabel,
	zoomInLabel,
	zoomOutLabel,
	zoomPercent,
	onRotateRight,
	onShowOriginal,
	onZoomIn,
	onZoomOut,
	onFitToWindow,
}: {
	canZoomIn: boolean;
	canZoomOut: boolean;
	fitToWindowLabel: string;
	originalButtonDisabled: boolean;
	originalButtonIcon: "Check" | "Eye" | "Spinner";
	originalButtonVisible: boolean;
	originalSourceLabel: string;
	renderOriginalButton: boolean;
	showOriginalState: ShowOriginalState;
	rotateRightLabel: string;
	zoomInLabel: string;
	zoomOutLabel: string;
	zoomPercent: number;
	onRotateRight: () => void;
	onShowOriginal: () => void;
	onZoomIn: () => void;
	onZoomOut: () => void;
	onFitToWindow: () => void;
}) {
	return (
		<div className="pointer-events-none absolute inset-x-0 bottom-0 z-10 flex justify-center bg-linear-to-t from-black/72 via-black/28 to-transparent px-3 pt-10 pb-4 opacity-0 transition-opacity duration-200 ease-out group-data-open/image-preview:opacity-100 group-data-closed/image-preview:opacity-0">
			<div className="pointer-events-auto flex items-center gap-1 rounded-xl border border-white/10 bg-black/40 p-1 text-white shadow-lg shadow-black/25 backdrop-blur-md">
				{renderOriginalButton ? (
					<OriginalImageButton
						disabled={originalButtonDisabled}
						icon={originalButtonIcon}
						label={originalSourceLabel}
						state={showOriginalState}
						visible={originalButtonVisible}
						onClick={onShowOriginal}
					/>
				) : null}
				<ToolbarButton
					label={zoomOutLabel}
					onClick={onZoomOut}
					icon="MagnifyingGlassMinus"
					disabled={!canZoomOut}
				/>
				<button
					type="button"
					aria-label={fitToWindowLabel}
					className="h-8 min-w-15 rounded-lg px-2 text-xs font-medium text-white/82 transition-colors hover:bg-white/10 hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/45"
					onClick={onFitToWindow}
					title={fitToWindowLabel}
				>
					{zoomPercent}%
				</button>
				<ToolbarButton
					label={zoomInLabel}
					onClick={onZoomIn}
					icon="MagnifyingGlassPlus"
					disabled={!canZoomIn}
				/>
				<div className="mx-1 h-5 w-px bg-white/14" />
				<ToolbarButton
					label={rotateRightLabel}
					onClick={onRotateRight}
					icon="ArrowClockwise"
				/>
			</div>
		</div>
	);
}

function OriginalImageButton({
	disabled,
	icon,
	label,
	state,
	visible,
	onClick,
}: {
	disabled: boolean;
	icon: "Check" | "Eye" | "Spinner";
	label: string;
	state: ShowOriginalState;
	visible: boolean;
	onClick: () => void;
}) {
	return (
		<div
			className={cn(
				"flex origin-left items-center overflow-hidden transition-[max-width,opacity,transform] duration-[260ms] ease-out",
				visible
					? "max-w-40 translate-x-0 opacity-100"
					: "max-w-0 translate-x-2 opacity-0",
			)}
		>
			<div className="flex shrink-0 items-center gap-1">
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className={cn(
						"text-white/82 shadow-none transition-[background-color,color,transform] duration-200 hover:bg-white/12 hover:text-white focus-visible:ring-white/35",
						visible ? "scale-100" : "scale-95",
						state === "success" &&
							"bg-emerald-400/15 text-emerald-100 hover:bg-emerald-400/15 hover:text-emerald-100",
					)}
					onClick={onClick}
					disabled={disabled}
				>
					<Icon
						name={icon}
						className={cn("size-4", state === "loading" && "animate-spin")}
					/>
					<span
						className={
							state === "loading" || state === "success" ? "sr-only" : undefined
						}
					>
						{label}
					</span>
				</Button>
				<div className="mx-1 h-5 w-px bg-white/14" />
			</div>
		</div>
	);
}

function ToolbarButton({
	disabled,
	icon,
	label,
	onClick,
}: {
	disabled?: boolean;
	icon:
		| "ArrowClockwise"
		| "ArrowsInCardinal"
		| "ArrowsOutCardinal"
		| "DotsThree"
		| "MagnifyingGlassMinus"
		| "MagnifyingGlassPlus"
		| "X";
	label: string;
	onClick: () => void;
}) {
	return (
		<Button
			type="button"
			variant="ghost"
			size="icon-sm"
			disabled={disabled}
			onClick={onClick}
			aria-label={label}
			title={label}
			className="text-white/78 shadow-none hover:bg-white/12 hover:text-white focus-visible:ring-white/35 disabled:text-white/25"
		>
			<Icon name={icon} className="size-4" />
			<span className="sr-only">{label}</span>
		</Button>
	);
}
