import { useTranslation } from "react-i18next";
import { FileItemStatusIndicators } from "@/components/files/FileItemStatusIndicators";
import { FileThumbnail } from "@/components/files/FileThumbnail";
import { TagChips } from "@/components/files/TagChips";
import { Icon } from "@/components/ui/icon";
import { TableCell } from "@/components/ui/table";
import { formatBytes, formatDate } from "@/lib/format";
import type { FileListItem, FolderListItem } from "@/types/api";

export function FileNameCell({
	file,
	thumbnailPath,
}: {
	file: FileListItem;
	thumbnailPath?: string;
}) {
	return (
		<TableCell className="pl-1 pr-2">
			<div className="flex min-w-0 items-center gap-3">
				<FileThumbnail file={file} size="sm" thumbnailPath={thumbnailPath} />
				<div className="flex min-w-0 flex-1 items-center gap-2">
					<div className="flex min-w-0 flex-1 items-center gap-2">
						<span
							className="min-w-0 truncate font-medium text-foreground"
							title={file.name}
						>
							{file.name}
						</span>
						<TagChips
							tags={file.tags}
							maxVisible={2}
							className="hidden min-w-0 flex-nowrap overflow-hidden sm:flex"
						/>
					</div>
					<FileItemStatusIndicators
						isShared={file.is_shared}
						isLocked={file.is_locked}
						compact
						className="ml-auto"
					/>
				</div>
			</div>
		</TableCell>
	);
}

export function FolderNameCell({ folder }: { folder: FolderListItem }) {
	return (
		<TableCell className="pl-1 pr-2">
			<div className="flex min-w-0 items-center gap-3">
				<div className="flex size-6 shrink-0 items-center justify-center overflow-hidden rounded-md border border-border/50 bg-amber-500/10 text-amber-500 shadow-xs dark:shadow-none">
					<Icon name="Folder" className="size-4" />
				</div>
				<div className="flex min-w-0 flex-1 items-center gap-2">
					<div className="flex min-w-0 flex-1 items-center gap-2">
						<span
							className="min-w-0 truncate font-medium text-foreground"
							title={folder.name}
						>
							{folder.name}
						</span>
						<TagChips
							tags={folder.tags}
							maxVisible={2}
							className="hidden min-w-0 flex-nowrap overflow-hidden sm:flex"
						/>
					</div>
					<FileItemStatusIndicators
						isShared={folder.is_shared}
						isLocked={folder.is_locked}
						compact
						className="ml-auto"
					/>
				</div>
			</div>
		</TableCell>
	);
}

export function FileSizeCell({ size }: { size: number }) {
	return (
		<TableCell className="text-muted-foreground">{formatBytes(size)}</TableCell>
	);
}

export function FolderSizeCell() {
	return <TableCell className="text-muted-foreground">---</TableCell>;
}

export function UpdatedAtCell({ updatedAt }: { updatedAt: string }) {
	const { i18n } = useTranslation("core");

	return (
		<TableCell className="text-muted-foreground">
			{formatDate(updatedAt, i18n)}
		</TableCell>
	);
}
