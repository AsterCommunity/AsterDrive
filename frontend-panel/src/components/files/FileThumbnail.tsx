import { useEffect } from "react";
import { useBlobUrl } from "@/hooks/useBlobUrl";
import { useEnteredViewport } from "@/hooks/useEnteredViewport";
import { fileService } from "@/services/fileService";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";
import type { FileInfo, FileListItem } from "@/types/api";
import { Icon } from "../ui/icon";
import { FileTypeIcon } from "./FileTypeIcon";

interface FileThumbnailProps {
	file: FileInfo | FileListItem;
	size?: "sm" | "md" | "lg";
	thumbnailPath?: string;
}

function getThumbnailExtension(fileName: string) {
	const trimmed = fileName.trim().toLowerCase();
	const dot = trimmed.lastIndexOf(".");
	if (dot <= 0 || dot === trimmed.length - 1) {
		return "";
	}
	return trimmed.slice(dot + 1);
}

function supportsThumbnailExtension(
	fileName: string,
	extensions: string[] | undefined,
) {
	const extension = getThumbnailExtension(fileName);
	if (!extension || !extensions?.length) {
		return false;
	}

	return extensions.some(
		(candidate) =>
			candidate.trim().replace(/^\./, "").toLowerCase() === extension,
	);
}

export function FileThumbnail({
	file,
	size = "sm",
	thumbnailPath,
}: FileThumbnailProps) {
	const thumbnailSupport = useThumbnailSupportStore((state) => state.config);
	const thumbnailSupportLoaded = useThumbnailSupportStore(
		(state) => state.isLoaded,
	);
	const loadThumbnailSupport = useThumbnailSupportStore((state) => state.load);
	const canRequestThumbnail =
		thumbnailSupportLoaded &&
		supportsThumbnailExtension(file.name, thumbnailSupport?.extensions);

	useEffect(() => {
		if (!thumbnailSupportLoaded) {
			void loadThumbnailSupport();
		}
	}, [loadThumbnailSupport, thumbnailSupportLoaded]);

	const { ref, hasEnteredViewport } = useEnteredViewport<HTMLDivElement>({
		enabled: canRequestThumbnail,
	});
	const blobPath =
		canRequestThumbnail && hasEnteredViewport
			? (thumbnailPath ?? fileService.thumbnailPath(file.id))
			: null;
	const { blobUrl, error, loading } = useBlobUrl(blobPath, {
		lane: "thumbnail",
	});

	if (size === "sm") {
		return (
			<div
				ref={ref}
				className="flex h-6 w-6 shrink-0 items-center justify-center overflow-hidden rounded-md border border-border/50 bg-muted/35 shadow-xs dark:bg-muted/25 dark:shadow-none"
			>
				{canRequestThumbnail && loading && !error && !blobUrl ? (
					<Icon
						name="Spinner"
						className="h-3.5 w-3.5 animate-spin text-muted-foreground"
						data-testid="thumbnail-loading"
					/>
				) : !canRequestThumbnail || error || !blobUrl ? (
					<FileTypeIcon
						mimeType={file.mime_type}
						fileName={file.name}
						fileCategory={file.file_category}
						className="h-4 w-4"
					/>
				) : (
					<img
						src={blobUrl}
						alt=""
						loading="lazy"
						decoding="async"
						draggable={false}
						className="h-full w-full object-cover"
					/>
				)}
			</div>
		);
	}

	if (size === "md") {
		if (canRequestThumbnail && loading && !error && !blobUrl) {
			return (
				<div
					ref={ref}
					className="flex h-full w-full items-center justify-center text-muted-foreground"
				>
					<Icon
						name="Spinner"
						className="h-4 w-4 animate-spin"
						data-testid="thumbnail-loading"
					/>
				</div>
			);
		}

		if (!canRequestThumbnail || error || !blobUrl) {
			return (
				<div
					ref={ref}
					className="flex h-full w-full items-center justify-center"
				>
					<FileTypeIcon
						mimeType={file.mime_type}
						fileName={file.name}
						fileCategory={file.file_category}
						className="h-5 w-5"
					/>
				</div>
			);
		}

		return (
			<div ref={ref} className="flex h-full w-full items-center justify-center">
				<img
					src={blobUrl}
					alt=""
					loading="lazy"
					decoding="async"
					draggable={false}
					className="h-full w-full object-cover"
				/>
			</div>
		);
	}

	if (canRequestThumbnail && loading && !error && !blobUrl) {
		return (
			<div
				ref={ref}
				className="flex h-full w-full items-center justify-center text-muted-foreground"
			>
				<Icon
					name="Spinner"
					className="h-5 w-5 animate-spin"
					data-testid="thumbnail-loading"
				/>
			</div>
		);
	}

	if (!canRequestThumbnail || error || !blobUrl) {
		return (
			<div ref={ref} className="flex h-full w-full items-center justify-center">
				<FileTypeIcon
					mimeType={file.mime_type}
					fileName={file.name}
					fileCategory={file.file_category}
					className="h-12 w-12"
				/>
			</div>
		);
	}

	return (
		<div ref={ref} className="flex h-full w-full items-center justify-center">
			<img
				src={blobUrl}
				alt=""
				loading="lazy"
				decoding="async"
				draggable={false}
				className="h-full w-auto shrink-0 max-w-none"
			/>
		</div>
	);
}
