import { useState } from "react";
import { useTranslation } from "react-i18next";
import { FileItemStatusIndicators } from "@/components/files/FileItemStatusIndicators";
import { FileThumbnail } from "@/components/files/FileThumbnail";
import { TagChips } from "@/components/files/TagChips";
import { Icon } from "@/components/ui/icon";
import { ItemCheckbox } from "@/components/ui/item-checkbox";
import { DRAG_SOURCE_MIME } from "@/lib/constants";
import {
	getInvalidInternalDropReason,
	hasInternalDragData,
	readInternalDragData,
	setInternalDragPreview,
	writeInternalDragData,
} from "@/lib/dragDrop";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { FileListItem, FolderListItem } from "@/types/api";

const EMPTY_TARGET_PATH_IDS: number[] = [];

interface FileCardProps {
	item: FileListItem | FolderListItem;
	isFolder: boolean;
	selected: boolean;
	onSelect: () => void;
	onClick: () => void;
	onDoubleClick?: () => void;
	/** IDs to drag when this item is part of a selection */
	dragData?: { fileIds: number[]; folderIds: number[] };
	resolveDragData?: () => { fileIds: number[]; folderIds: number[] };
	onDrop?: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number,
		targetPathIds: number[],
	) => void;
	targetPathIds?: number[];
	fading?: boolean;
	draggable?: boolean;
	selectable?: boolean;
	selectionActive?: boolean;
	thumbnailPath?: string;
	actionMenu?: React.ReactNode;
}

export function FileCard({
	item,
	isFolder,
	selected,
	onSelect,
	onClick,
	onDoubleClick,
	dragData,
	resolveDragData,
	onDrop,
	targetPathIds = EMPTY_TARGET_PATH_IDS,
	fading,
	draggable = true,
	selectable = true,
	selectionActive = false,
	thumbnailPath,
	actionMenu,
}: FileCardProps) {
	const { t } = useTranslation("core");
	const [dragOver, setDragOver] = useState(false);
	const metaText = isFolder
		? t("folder")
		: formatBytes((item as FileListItem).size ?? 0);

	const handleDragStart = (e: React.DragEvent) => {
		const data =
			resolveDragData?.() ??
			(dragData &&
			(dragData.fileIds.length > 0 || dragData.folderIds.length > 0)
				? dragData
				: isFolder
					? { fileIds: [], folderIds: [item.id] }
					: { fileIds: [item.id], folderIds: [] });
		writeInternalDragData(e.dataTransfer, data);
		setInternalDragPreview(e, {
			variant: "grid-card",
			itemCount: data.fileIds.length + data.folderIds.length,
		});
	};

	const handleDragOver = (e: React.DragEvent) => {
		if (
			!isFolder ||
			!hasInternalDragData(e.dataTransfer) ||
			e.dataTransfer.types.includes(DRAG_SOURCE_MIME)
		) {
			return;
		}
		e.preventDefault();
		e.stopPropagation();
		e.dataTransfer.dropEffect = "move";
		setDragOver(true);
	};

	const handleDragLeave = () => setDragOver(false);

	const handleDrop = (e: React.DragEvent) => {
		setDragOver(false);
		if (isFolder && e.dataTransfer.types.includes(DRAG_SOURCE_MIME)) {
			return;
		}
		if (!isFolder) return;
		e.preventDefault();
		e.stopPropagation();
		const data = readInternalDragData(e.dataTransfer);
		if (!data) return;
		if (getInvalidInternalDropReason(data, item.id, targetPathIds) !== null) {
			return;
		}
		onDrop?.(data.fileIds, data.folderIds, item.id, targetPathIds);
	};

	return (
		// biome-ignore lint/a11y/useSemanticElements: card with nested interactive checkbox cannot be a button
		<div
			data-drag-preview-root
			data-folder-drop-target={isFolder ? "true" : undefined}
			className={cn(
				"group relative flex min-h-[166px] select-none flex-col rounded-lg border border-border/65 bg-background p-2.5 shadow-xs transition-[background-color,border-color,box-shadow,opacity,transform] duration-150 ease-out hover:border-primary/30 hover:bg-muted/20 hover:shadow-sm dark:shadow-none dark:hover:bg-muted/15",
				selected &&
					"border-primary bg-accent text-accent-foreground shadow-sm ring-1 ring-primary/25 dark:shadow-none",
				draggable && dragOver && "bg-accent/35 ring-2 ring-primary",
				fading && "opacity-0",
			)}
			draggable={draggable}
			onDragStart={draggable ? handleDragStart : undefined}
			onDragOver={draggable ? handleDragOver : undefined}
			onDragLeave={draggable ? handleDragLeave : undefined}
			onDrop={draggable ? handleDrop : undefined}
			onClick={onClick}
			onDoubleClick={onDoubleClick}
			onKeyDown={(e) => {
				if (e.key !== "Enter") return;
				e.preventDefault();
				(onDoubleClick ?? onClick)();
			}}
			role="button"
			tabIndex={0}
		>
			{selectable && (
				<ItemCheckbox
					data-drag-preview-hidden
					checked={selected}
					onChange={onSelect}
					className={cn(
						"absolute top-2 left-2 transition-opacity",
						selected || selectionActive
							? "opacity-100"
							: "opacity-100 sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100",
					)}
				/>
			)}

			<FileItemStatusIndicators
				isShared={item.is_shared}
				isLocked={item.is_locked}
				compact
				className={cn(
					"absolute top-2 flex-col items-end gap-1",
					actionMenu ? "right-11 sm:right-2" : "right-2",
				)}
			/>
			{actionMenu ? (
				// biome-ignore lint/a11y/noStaticElementInteractions: non-interactive boundary prevents menu events from opening the parent card
				<div
					data-file-card-action-menu
					role="presentation"
					className="absolute top-2 right-2 z-10 sm:hidden"
					onPointerDown={(event) => event.stopPropagation()}
					onClick={(event) => event.stopPropagation()}
					onDoubleClick={(event) => event.stopPropagation()}
					onKeyDown={(event) => {
						if (
							event.key === "Enter" ||
							event.key === " " ||
							event.key === "Spacebar"
						) {
							event.stopPropagation();
						}
					}}
				>
					{actionMenu}
				</div>
			) : null}

			<div
				data-drag-preview-media
				className="mb-2.5 flex h-20 w-full items-center justify-center overflow-hidden rounded-md border border-border/55 bg-muted/25 dark:border-border/60 dark:bg-muted/20"
			>
				{isFolder ? (
					<div className="flex size-14 items-center justify-center rounded-lg bg-amber-500/10 text-amber-500 ring-1 ring-amber-500/15">
						<Icon name="Folder" className="size-10" />
					</div>
				) : (
					<FileThumbnail
						file={item as FileListItem}
						size="lg"
						thumbnailPath={thumbnailPath}
						iconClassName="size-11"
						imageClassName="h-full w-full object-cover"
					/>
				)}
			</div>

			<div className="min-w-0 flex-1 space-y-1">
				<span
					data-drag-preview-name
					className="block w-full line-clamp-2 text-sm leading-tight font-medium"
					title={item.name}
				>
					{item.name}
				</span>
				<div className="truncate text-xs text-muted-foreground">{metaText}</div>
			</div>
			<TagChips
				tags={item.tags}
				maxVisible={2}
				className="mt-2 max-h-5 w-full justify-start overflow-hidden"
			/>
		</div>
	);
}
