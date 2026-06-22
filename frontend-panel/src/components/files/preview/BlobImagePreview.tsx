import type { CSSProperties, Ref } from "react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { useBlobUrl } from "@/hooks/useBlobUrl";
import { canBrowserRenderImage } from "@/lib/browserImageSupport";
import { type ResourcePath, resourceRequestPath } from "@/lib/resourceRequest";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import { PreviewError } from "./PreviewError";
import { PreviewLoadingState } from "./PreviewLoadingState";
import type { PreviewableFileLike } from "./types";

interface BlobImagePreviewProps {
	file: PreviewableFileLike;
	fallbackResource?: ResourcePath | null;
	fillContainer?: boolean;
	imageClassName?: string;
	imageRef?: Ref<HTMLImageElement>;
	imageStyle?: CSSProperties;
	onImageLoad?: (source: ImagePreviewSource) => void;
	onImageRenderError?: (source: ImagePreviewSource) => void;
	resource: ResourcePath | null;
	source?: ImagePreviewSource;
	showOriginalButtonPlacement?: "inline" | "none";
	viewportClassName?: string;
	viewportRef?: Ref<HTMLDivElement>;
}

function isSvgPreview(file: PreviewableFileLike) {
	return (
		file.mime_type.toLowerCase() === "image/svg+xml" ||
		file.name.toLowerCase().endsWith(".svg")
	);
}

export type ImagePreviewSource = "original" | "backend_preview";
export type ShowOriginalState = "hidden" | "available" | "loading" | "success";

export function BlobImagePreview({
	file,
	fallbackResource,
	fillContainer = false,
	imageClassName: imageClassNameProp,
	imageRef,
	imageStyle,
	onImageLoad,
	onImageRenderError,
	resource,
	source,
	showOriginalButtonPlacement = "inline",
	viewportClassName,
	viewportRef,
}: BlobImagePreviewProps) {
	const { t } = useTranslation("files");
	const imagePreviewPreference = useFrontendConfigStore(
		(state) => state.imagePreviewPreference,
	);
	const pathKey = resource ? resourceRequestPath(resource) : "";
	const fallbackResourceKey = fallbackResource
		? resourceRequestPath(fallbackResource)
		: "";
	const previewKey = `${file.name}\u0000${file.mime_type}\u0000${pathKey}\u0000${fallbackResourceKey}\u0000${imagePreviewPreference}`;
	const [requestedOriginalKey, setRequestedOriginalKey] = useState<
		string | null
	>(null);
	const [renderedOriginalKey, setRenderedOriginalKey] = useState<string | null>(
		null,
	);
	const [failedOriginalKey, setFailedOriginalKey] = useState<string | null>(
		null,
	);
	const requestedOriginal = requestedOriginalKey === previewKey;
	const originalRenderedSuccessfully = renderedOriginalKey === previewKey;
	const originalRenderFailed = failedOriginalKey === previewKey;
	const hasBackendPreview = fallbackResource != null;
	const originalIsBrowserRenderable = canBrowserRenderImage(file);
	const isControlledSource = source != null;
	const canShowOriginal =
		imagePreviewPreference === "preview_first" &&
		hasBackendPreview &&
		originalIsBrowserRenderable;
	const baseSource: ImagePreviewSource =
		source ??
		(hasBackendPreview &&
		(imagePreviewPreference === "preview_first" || !originalIsBrowserRenderable)
			? "backend_preview"
			: "original");
	const originalCanBeDisplayed =
		originalIsBrowserRenderable || isControlledSource;
	const shouldLoadOriginal =
		resource != null &&
		!isControlledSource &&
		canShowOriginal &&
		baseSource === "backend_preview" &&
		requestedOriginal;
	const {
		blobUrl: originalBlobUrl,
		error: originalError,
		loading: originalLoading,
		retry: retryOriginal,
	} = useBlobUrl(shouldLoadOriginal ? resource : null, {
		lane: "default",
	});
	const originalReady =
		shouldLoadOriginal && originalBlobUrl && !originalLoading && !originalError;
	const shouldFallbackOriginalRenderToPreview =
		resource != null &&
		!isControlledSource &&
		baseSource === "original" &&
		originalRenderFailed &&
		hasBackendPreview;
	const shouldPromoteReadyOriginal =
		resource != null &&
		!isControlledSource &&
		baseSource === "backend_preview" &&
		originalReady &&
		!originalRenderFailed;
	const displaySource: ImagePreviewSource =
		shouldFallbackOriginalRenderToPreview
			? "backend_preview"
			: shouldPromoteReadyOriginal
				? "original"
				: baseSource;
	const displayPath: ResourcePath | null =
		displaySource === "backend_preview"
			? (fallbackResource ?? null)
			: resource == null
				? null
				: originalCanBeDisplayed
					? resource
					: null;
	const originalUnsupported =
		displaySource === "original" && resource != null && !originalCanBeDisplayed;
	const { blobUrl, error, loading, retry } = useBlobUrl(displayPath, {
		lane: displaySource === "backend_preview" ? "preview" : "default",
	});
	const [imageRenderFailedKey, setImageRenderFailedKey] = useState<
		string | null
	>(null);
	const imageRenderKey = `${previewKey}\u0000${displaySource}`;
	const imageRenderFailed = imageRenderFailedKey === imageRenderKey;
	const canRequestOriginal =
		resource != null &&
		!isControlledSource &&
		canShowOriginal &&
		baseSource === "backend_preview" &&
		!originalRenderedSuccessfully;
	const showOriginalState: ShowOriginalState = canRequestOriginal
		? originalRenderedSuccessfully
			? "success"
			: requestedOriginal && !originalError
				? "loading"
				: "available"
		: canShowOriginal &&
				displaySource === "original" &&
				originalRenderedSuccessfully
			? "success"
			: "hidden";
	const imageContainerClass =
		viewportClassName ??
		(fillContainer
			? "flex h-full min-h-0 w-full items-center justify-center p-4"
			: isSvgPreview(file)
				? "flex w-full items-center justify-center p-4"
				: "mx-auto flex w-fit max-w-full min-w-0 items-center justify-center p-4");
	const imageClass =
		imageClassNameProp ??
		(fillContainer
			? "block h-full w-full min-w-0 object-contain"
			: isSvgPreview(file)
				? "block h-auto w-full max-h-[min(70vh,48rem)] max-w-[min(70vw,48rem)] min-w-0 object-contain"
				: "block max-h-[min(70vh,48rem)] max-w-full min-w-0 object-contain");

	const handleImageError = () => {
		onImageRenderError?.(displaySource);
		if (
			!isControlledSource &&
			displaySource === "original" &&
			hasBackendPreview
		) {
			setFailedOriginalKey(previewKey);
			setRequestedOriginalKey(null);
			return;
		}
		setImageRenderFailedKey(imageRenderKey);
	};

	const handleImageLoad = () => {
		onImageLoad?.(displaySource);
		if (!isControlledSource && displaySource === "original") {
			setRenderedOriginalKey(previewKey);
		}
	};

	const handleRetry = () => {
		setImageRenderFailedKey(null);
		retry();
	};

	const handleShowOriginal = () => {
		setImageRenderFailedKey(null);
		setFailedOriginalKey(null);
		if (requestedOriginal && originalError) {
			retryOriginal();
			return;
		}
		setRequestedOriginalKey(previewKey);
	};

	const originalButton =
		canRequestOriginal && showOriginalButtonPlacement === "inline" ? (
			<Button
				type="button"
				variant="outline"
				size="sm"
				className="shrink-0"
				onClick={handleShowOriginal}
				disabled={showOriginalState === "loading"}
			>
				<Icon
					name={showOriginalState === "loading" ? "Spinner" : "Eye"}
					className={`mr-1.5 size-4 ${showOriginalState === "loading" ? "animate-spin" : ""}`}
				/>
				{t("preview_show_original")}
			</Button>
		) : null;

	const readyBlobUrl =
		!loading && !error && !imageRenderFailed ? blobUrl : null;
	// Defensive fallback: readyBlobUrl should be null only while loading, after
	// error, or after imageRenderFailed, but keep a safe loading state if future
	// state combinations violate that invariant.
	const content =
		originalUnsupported || error || imageRenderFailed ? (
			<PreviewError appearance="dark" onRetry={handleRetry} />
		) : loading || !blobUrl ? (
			<PreviewLoadingState text={t("loading_preview")} className="h-full" />
		) : readyBlobUrl ? (
			<div ref={viewportRef} className={imageContainerClass}>
				<img
					ref={imageRef}
					src={readyBlobUrl}
					alt={file.name}
					draggable={false}
					onError={handleImageError}
					onLoad={handleImageLoad}
					className={imageClass}
					style={imageStyle}
				/>
			</div>
		) : (
			<PreviewLoadingState text={t("loading_preview")} className="h-full" />
		);

	if (!originalButton) {
		return content;
	}

	return (
		<div
			className={
				fillContainer
					? "flex h-full min-h-0 w-full flex-col"
					: "flex w-full flex-col"
			}
		>
			<div className="flex justify-end px-4 pt-4">{originalButton}</div>
			<div className={fillContainer ? "min-h-0 flex-1" : undefined}>
				{content}
			</div>
		</div>
	);
}
