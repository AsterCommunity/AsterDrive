import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useBlobUrl } from "@/hooks/useBlobUrl";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import type { PublicImagePreviewPreference } from "@/types/api";
import { PreviewError } from "./PreviewError";
import { PreviewLoadingState } from "./PreviewLoadingState";
import type { PreviewableFileLike } from "./types";

interface BlobImagePreviewProps {
	file: PreviewableFileLike;
	fallbackPath?: string;
	fillContainer?: boolean;
	path: string;
}

function isSvgPreview(file: PreviewableFileLike) {
	return (
		file.mime_type.toLowerCase() === "image/svg+xml" ||
		file.name.toLowerCase().endsWith(".svg")
	);
}

function isHeifPreview(file: PreviewableFileLike) {
	const lowerName = file.name.trim().toLowerCase();
	const mime = file.mime_type.trim().toLowerCase();
	return (
		mime === "image/heic" ||
		mime === "image/heif" ||
		lowerName.endsWith(".heic") ||
		lowerName.endsWith(".heif")
	);
}

type ImagePreviewSource = "original" | "backend_preview";

function chooseInitialPreviewSource(
	file: PreviewableFileLike,
	fallbackPath: string | undefined,
	preference: PublicImagePreviewPreference,
): ImagePreviewSource {
	if (!fallbackPath) return "original";
	if (isHeifPreview(file)) return "backend_preview";
	return preference === "preview_first" ? "backend_preview" : "original";
}

function otherPreviewSource(
	source: ImagePreviewSource,
	fallbackPath: string | undefined,
): ImagePreviewSource | null {
	if (!fallbackPath) return null;
	return source === "original" ? "backend_preview" : "original";
}

export function BlobImagePreview({
	file,
	fallbackPath,
	fillContainer = false,
	path,
}: BlobImagePreviewProps) {
	const { t } = useTranslation("files");
	const imagePreviewPreference = useFrontendConfigStore(
		(state) => state.imagePreviewPreference,
	);
	const previewKey = `${file.name}\u0000${file.mime_type}\u0000${path}\u0000${
		fallbackPath ?? ""
	}\u0000${imagePreviewPreference}`;
	const initialSource = chooseInitialPreviewSource(
		file,
		fallbackPath,
		imagePreviewPreference,
	);
	const [fallbackAttemptKey, setFallbackAttemptKey] = useState<string | null>(
		null,
	);
	const shouldUseAlternateSource = fallbackAttemptKey === previewKey;
	const alternateSource = otherPreviewSource(initialSource, fallbackPath);
	const activeSource =
		shouldUseAlternateSource && alternateSource
			? alternateSource
			: initialSource;
	const activePath =
		activeSource === "backend_preview" ? (fallbackPath ?? path) : path;
	const { blobUrl, error, loading, retry } = useBlobUrl(activePath);
	const canTryAlternateSource =
		!shouldUseAlternateSource && alternateSource !== null;
	const [imageRenderFailedKey, setImageRenderFailedKey] = useState<
		string | null
	>(null);
	const imageRenderFailed =
		imageRenderFailedKey === `${previewKey}\u0000${activeSource ?? ""}`;

	useEffect(() => {
		if (!loading && (error || !blobUrl) && canTryAlternateSource) {
			setFallbackAttemptKey(previewKey);
		}
	}, [blobUrl, canTryAlternateSource, error, loading, previewKey]);

	const handleImageError = () => {
		if (canTryAlternateSource) {
			setImageRenderFailedKey(null);
			setFallbackAttemptKey(previewKey);
			return;
		}
		setImageRenderFailedKey(`${previewKey}\u0000${activeSource}`);
	};

	const handleRetry = () => {
		setImageRenderFailedKey(null);
		setFallbackAttemptKey(null);
		retry();
	};

	if (loading) {
		return (
			<PreviewLoadingState text={t("loading_preview")} className="h-full" />
		);
	}

	if (error || !blobUrl || imageRenderFailed) {
		if ((error || !blobUrl) && canTryAlternateSource) {
			return (
				<PreviewLoadingState text={t("loading_preview")} className="h-full" />
			);
		}
		return <PreviewError onRetry={handleRetry} />;
	}

	const isSvg = isSvgPreview(file);

	return (
		<div
			className={
				fillContainer
					? "flex h-full min-h-0 w-full items-center justify-center p-4"
					: isSvg
						? "flex w-full items-center justify-center p-4"
						: "mx-auto flex w-fit max-w-full min-w-0 items-center justify-center p-4"
			}
		>
			<img
				src={blobUrl}
				alt={file.name}
				onError={handleImageError}
				className={
					fillContainer
						? "block h-full w-full min-w-0 object-contain"
						: isSvg
							? "block h-auto w-full max-h-[min(70vh,48rem)] max-w-[min(70vw,48rem)] min-w-0 object-contain"
							: "block max-h-[min(70vh,48rem)] max-w-full min-w-0 object-contain"
				}
			/>
		</div>
	);
}
