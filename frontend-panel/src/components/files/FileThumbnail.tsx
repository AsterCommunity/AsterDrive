import { useEffect } from "react";
import { useBlobUrl } from "@/hooks/useBlobUrl";
import { useEnteredViewport } from "@/hooks/useEnteredViewport";
import { supportsGeneratedThumbnailFile } from "@/lib/thumbnailSupport";
import { cn } from "@/lib/utils";
import { fileService } from "@/services/fileService";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";
import type { FileCategory, FileInfo, FileListItem } from "@/types/api";
import { Icon } from "../ui/icon";
import { FileTypeIcon } from "./FileTypeIcon";

export interface ThumbnailFileLike {
	id: number;
	file_category?: FileCategory;
	mime_type: string;
	name: string;
}

interface FileThumbnailProps {
	className?: string;
	file: FileInfo | FileListItem | ThumbnailFileLike;
	iconClassName?: string;
	imageClassName?: string;
	size?: "sm" | "md" | "lg";
	thumbnailPath?: string;
}

export function FileThumbnail({
	className,
	file,
	iconClassName,
	imageClassName,
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
		supportsGeneratedThumbnailFile(file.name, thumbnailSupport);

	useEffect(() => {
		if (!thumbnailSupportLoaded) {
			void loadThumbnailSupport();
		}
	}, [loadThumbnailSupport, thumbnailSupportLoaded]);

	const { ref, isInViewport } = useEnteredViewport<HTMLDivElement>({
		enabled: canRequestThumbnail,
		trackVisibility: true,
	});
	const blobPath =
		canRequestThumbnail && isInViewport
			? (thumbnailPath ?? fileService.thumbnailPath(file.id))
			: null;
	const { blobUrl, error, loading } = useBlobUrl(blobPath, {
		lane: "thumbnail",
	});

	if (size === "sm") {
		return (
			<div
				ref={ref}
				className={cn(
					"flex size-6 shrink-0 items-center justify-center overflow-hidden rounded-md border border-border/50 bg-muted/35 shadow-xs dark:bg-muted/25 dark:shadow-none",
					className,
				)}
			>
				{canRequestThumbnail && loading && !error && !blobUrl ? (
					<Icon
						name="Spinner"
						className={cn(
							"size-3.5 animate-spin text-muted-foreground",
							iconClassName,
						)}
						data-testid="thumbnail-loading"
					/>
				) : !canRequestThumbnail || error || !blobUrl ? (
					<FileTypeIcon
						mimeType={file.mime_type}
						fileName={file.name}
						fileCategory={file.file_category}
						className={cn("size-4", iconClassName)}
					/>
				) : (
					<img
						src={blobUrl}
						alt=""
						loading="lazy"
						decoding="async"
						draggable={false}
						className={cn("h-full w-full object-cover", imageClassName)}
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
					className={cn(
						"flex h-full w-full items-center justify-center text-muted-foreground",
						className,
					)}
				>
					<Icon
						name="Spinner"
						className={cn("size-4 animate-spin", iconClassName)}
						data-testid="thumbnail-loading"
					/>
				</div>
			);
		}

		if (!canRequestThumbnail || error || !blobUrl) {
			return (
				<div
					ref={ref}
					className={cn(
						"flex h-full w-full items-center justify-center",
						className,
					)}
				>
					<FileTypeIcon
						mimeType={file.mime_type}
						fileName={file.name}
						fileCategory={file.file_category}
						className={cn("size-5", iconClassName)}
					/>
				</div>
			);
		}

		return (
			<div
				ref={ref}
				className={cn(
					"flex h-full w-full items-center justify-center",
					className,
				)}
			>
				<img
					src={blobUrl}
					alt=""
					loading="lazy"
					decoding="async"
					draggable={false}
					className={cn("h-full w-full object-cover", imageClassName)}
				/>
			</div>
		);
	}

	if (canRequestThumbnail && loading && !error && !blobUrl) {
		return (
			<div
				ref={ref}
				className={cn(
					"flex h-full w-full items-center justify-center text-muted-foreground",
					className,
				)}
			>
				<Icon
					name="Spinner"
					className={cn("size-5 animate-spin", iconClassName)}
					data-testid="thumbnail-loading"
				/>
			</div>
		);
	}

	if (!canRequestThumbnail || error || !blobUrl) {
		return (
			<div
				ref={ref}
				className={cn(
					"flex h-full w-full items-center justify-center",
					className,
				)}
			>
				<FileTypeIcon
					mimeType={file.mime_type}
					fileName={file.name}
					fileCategory={file.file_category}
					className={cn("size-12", iconClassName)}
				/>
			</div>
		);
	}

	return (
		<div
			ref={ref}
			className={cn(
				"flex h-full w-full items-center justify-center",
				className,
			)}
		>
			<img
				src={blobUrl}
				alt=""
				loading="lazy"
				decoding="async"
				draggable={false}
				className={cn("h-full w-auto shrink-0 max-w-none", imageClassName)}
			/>
		</div>
	);
}
