import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useBlobUrl } from "@/hooks/useBlobUrl";
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

export function BlobImagePreview({
	file,
	fallbackPath,
	fillContainer = false,
	path,
}: BlobImagePreviewProps) {
	const { t } = useTranslation("files");
	const previewKey = `${file.name}\u0000${file.mime_type}\u0000${path}\u0000${
		fallbackPath ?? ""
	}`;
	const [imageRenderFailedKey, setImageRenderFailedKey] = useState<
		string | null
	>(null);
	const shouldPreferBackendPreview =
		Boolean(fallbackPath) && isHeifPreview(file);
	const imageRenderFailed = imageRenderFailedKey === previewKey;
	const activePath = shouldPreferBackendPreview ? (fallbackPath ?? path) : path;
	const { blobUrl, error, loading, retry } = useBlobUrl(activePath);

	const handleImageError = () => {
		setImageRenderFailedKey(previewKey);
	};

	const handleRetry = () => {
		setImageRenderFailedKey(null);
		retry();
	};

	if (loading) {
		return (
			<PreviewLoadingState text={t("loading_preview")} className="h-full" />
		);
	}

	if (error || !blobUrl || imageRenderFailed) {
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
