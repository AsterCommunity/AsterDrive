import { FileThumbnail } from "@/components/files/FileThumbnail";
import { DialogTitle } from "@/components/ui/dialog";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { FileInfo, FileListItem } from "@/types/api";

interface FilePreviewFileSummaryProps {
	file: FileInfo | FileListItem;
	title?: string;
	className?: string;
	fileNameAsDialogTitle?: boolean;
	iconClassName?: string;
	thumbnailPath?: string;
}

export function FilePreviewFileSummary({
	file,
	title,
	className,
	fileNameAsDialogTitle = false,
	iconClassName,
	thumbnailPath,
}: FilePreviewFileSummaryProps) {
	const fileName = fileNameAsDialogTitle ? (
		<DialogTitle className="truncate text-sm font-semibold leading-5">
			{file.name}
		</DialogTitle>
	) : (
		<div
			className={cn(
				"truncate text-sm text-muted-foreground",
				!title && "font-medium text-foreground",
			)}
			title={file.name}
		>
			{file.name}
		</div>
	);

	return (
		<div className={cn("flex min-w-0 items-center gap-3", className)}>
			<div
				className={cn(
					"flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-lg border border-border/55 bg-muted/35 text-muted-foreground",
					iconClassName,
				)}
			>
				<FileThumbnail
					file={file}
					size="md"
					thumbnailPath={thumbnailPath}
					className="h-full w-full"
					iconClassName="size-5"
					imageClassName="h-full w-full object-cover"
				/>
			</div>
			<div className="min-w-0 flex-1">
				{title ? (
					<div className="truncate text-sm font-semibold text-foreground">
						{title}
					</div>
				) : null}
				{fileName}
				<div className="truncate text-xs text-muted-foreground">
					{formatBytes(file.size)}
				</div>
			</div>
		</div>
	);
}
