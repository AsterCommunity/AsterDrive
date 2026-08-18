import { useTranslation } from "react-i18next";
import { FileItemStatusIndicators } from "@/components/files/FileItemStatusIndicators";
import { FolderGlyph } from "@/components/files/FolderGlyph";
import { TagChips } from "@/components/files/TagChips";
import {
	type GridItemDragData,
	useGridItemDragDrop,
} from "@/components/files/useGridItemDragDrop";
import { ItemCheckbox } from "@/components/ui/item-checkbox";
import { isResourceLocked } from "@/lib/resourceLock";
import { cn } from "@/lib/utils";
import type { FolderListItem } from "@/types/api";

interface FolderGridItemProps {
	item: FolderListItem;
	selected: boolean;
	onSelect?: () => void;
	onClick: () => void;
	onDoubleClick?: () => void;
	/** IDs to drag when this item is part of a selection */
	dragData?: GridItemDragData;
	resolveDragData?: () => GridItemDragData;
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
	actionMenu?: React.ReactNode;
	alwaysShowActionMenu?: boolean;
}

/**
 * 网格视图文件夹项。与文件卡片（FileCard）刻意不同形：
 * 无卡片边框、无媒体区底色，靠填充式文件夹图形本身承载识别，
 * hover/selected/dragOver 状态全部落在容器背景与 ring 上。
 */
export function FolderGridItem({
	item,
	selected,
	onSelect,
	onClick,
	onDoubleClick,
	dragData,
	resolveDragData,
	onDrop,
	targetPathIds,
	fading,
	draggable = true,
	selectable = true,
	selectionActive = false,
	actionMenu,
	alwaysShowActionMenu = false,
}: FolderGridItemProps) {
	const { t } = useTranslation("core");
	const { dragOver, dragProps } = useGridItemDragDrop({
		itemId: item.id,
		isFolder: true,
		draggable,
		dragData,
		resolveDragData,
		onDrop,
		targetPathIds,
	});

	return (
		// biome-ignore lint/a11y/useSemanticElements: grid item with nested interactive checkbox cannot be a button
		<div
			data-drag-preview-root
			data-folder-drop-target="true"
			className={cn(
				"group relative flex min-h-[132px] select-none flex-col items-center rounded-xl px-2.5 pt-3 pb-2.5 transition-[background-color,box-shadow,opacity] duration-150 ease-out hover:bg-muted/45 dark:hover:bg-muted/25",
				selected && "bg-accent/60 dark:bg-accent/40",
				draggable && dragOver && "bg-accent/40 ring-2 ring-primary",
				fading && "opacity-0",
			)}
			{...dragProps}
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
					onChange={onSelect ?? (() => {})}
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
				isLocked={isResourceLocked(item.lock_state)}
				compact
				className={cn(
					"absolute top-2 flex-col items-end gap-1",
					actionMenu ? "right-11 sm:right-2" : "right-2",
				)}
			/>
			{actionMenu ? (
				// biome-ignore lint/a11y/noStaticElementInteractions: non-interactive boundary prevents menu events from opening the parent item
				<div
					data-file-card-action-menu
					role="presentation"
					className={cn(
						"absolute top-2 right-2 z-10",
						selectable && !alwaysShowActionMenu && "sm:hidden",
					)}
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
				className="mb-1.5 flex h-20 w-full items-center justify-center"
			>
				<FolderGlyph className="size-16 drop-shadow-sm transition-transform duration-150 ease-out group-hover:scale-[1.04] motion-reduce:transition-none motion-reduce:group-hover:transform-none" />
			</div>

			<div className="min-w-0 flex-1 space-y-1 text-center">
				<span
					data-drag-preview-name
					className="block w-full line-clamp-2 text-sm leading-tight font-medium"
					title={item.name}
				>
					{item.name}
				</span>
				<div className="truncate text-xs text-muted-foreground">
					{t("folder")}
				</div>
			</div>
			<TagChips
				tags={item.tags}
				maxVisible={2}
				className="mt-1.5 max-h-5 w-full justify-center overflow-hidden"
			/>
		</div>
	);
}
