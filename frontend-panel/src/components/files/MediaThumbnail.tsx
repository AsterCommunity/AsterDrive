import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import { FileThumbnail, type ThumbnailFileLike } from "./FileThumbnail";

interface MediaThumbnailProps {
	artworkUrl?: string | null;
	className?: string;
	file?: ThumbnailFileLike | null;
	iconClassName?: string;
	imageClassName?: string;
	size?: "sm" | "md" | "lg";
	thumbnailPath?: string | null;
}

export function MediaThumbnail({
	artworkUrl,
	className,
	file,
	iconClassName,
	imageClassName,
	size = "md",
	thumbnailPath,
}: MediaThumbnailProps) {
	if (file) {
		return (
			<FileThumbnail
				file={file}
				size={size}
				thumbnailPath={thumbnailPath ?? undefined}
				className={cn(
					"overflow-hidden rounded-lg border border-border/55 bg-muted/35 shadow-xs dark:bg-muted/25 dark:shadow-none",
					className,
				)}
				iconClassName={iconClassName}
				imageClassName={imageClassName}
			/>
		);
	}

	if (artworkUrl) {
		return (
			<img
				src={artworkUrl}
				alt=""
				className={cn("object-cover", imageClassName, className)}
			/>
		);
	}

	return (
		<div
			className={cn(
				"flex items-center justify-center overflow-hidden rounded-lg border border-border/55 bg-[linear-gradient(135deg,var(--color-muted),var(--color-background))] text-primary",
				className,
			)}
		>
			<Icon
				name="VinylRecord"
				className={cn("h-1/2 w-1/2 opacity-80", iconClassName)}
			/>
		</div>
	);
}
