import { FileHoverPreview } from "@/components/files/FileHoverPreview";
import { FileItemStatusIndicators } from "@/components/files/FileItemStatusIndicators";
import { FileThumbnail } from "@/components/files/FileThumbnail";
import { TagChips } from "@/components/files/TagChips";
import {
	type GridItemDragData,
	useGridItemDragDrop,
} from "@/components/files/useGridItemDragDrop";
import { ItemCheckbox } from "@/components/ui/item-checkbox";
import { useDelayedHoverPreview } from "@/hooks/useDelayedHoverPreview";
import { formatBytes } from "@/lib/format";
import { isResourceLocked } from "@/lib/resourceLock";
import { cn } from "@/lib/utils";
import type { FileListItem } from "@/types/api";

interface FileCardProps {
	item: FileListItem;
	selected: boolean;
	onSelect?: () => void;
	/** 点击事件（携带修饰键状态，Finder 式 Cmd/Ctrl/Shift 选择由调用方处理） */
	onClick: (modifiers: {
		metaKey: boolean;
		ctrlKey: boolean;
		shiftKey: boolean;
	}) => void;
	onDoubleClick?: () => void;
	/** IDs to drag when this item is part of a selection */
	dragData?: GridItemDragData;
	resolveDragData?: () => GridItemDragData;
	targetPathIds?: number[];
	fading?: boolean;
	draggable?: boolean;
	selectable?: boolean;
	selectionActive?: boolean;
	thumbnailPath?: string;
	actionMenu?: React.ReactNode;
	alwaysShowActionMenu?: boolean;
	/** 覆盖默认副标题（大小），回收站模式用来展示过期时间 */
	subtitle?: React.ReactNode;
}

/**
 * 网格视图文件项。文件夹网格项见 FolderGridItem——
 * D9 起两者同为无容器形态：缩略图/徽章本身就是内容，
 * 容器透明，hover/selected 才浮现色垫（Finder 式"内容即界面"）。
 */
export function FileCard({
	item,
	selected,
	onSelect,
	onClick,
	onDoubleClick,
	dragData,
	resolveDragData,
	targetPathIds,
	fading,
	draggable = true,
	selectable = true,
	selectionActive = false,
	thumbnailPath,
	actionMenu,
	alwaysShowActionMenu = false,
	subtitle,
}: FileCardProps) {
	const { dragProps } = useGridItemDragDrop({
		itemId: item.id,
		isFolder: false,
		draggable,
		dragData,
		resolveDragData,
		targetPathIds,
	});
	// 悬停意向预览：只在缩略图媒体区上计时；cover 模式大图原位盖住缩略图向上展开
	const hoverPreview = useDelayedHoverPreview<HTMLDivElement>();

	return (
		// biome-ignore lint/a11y/useSemanticElements: card with nested interactive checkbox cannot be a button
		<div
			data-drag-preview-root
			className={cn(
				"group relative flex min-h-[166px] select-none flex-col rounded-xl px-2.5 py-2.5 transition-[background-color,box-shadow,opacity,transform] duration-150 ease-out outline-none hover:bg-muted/45 focus-visible:ring-2 focus-visible:ring-ring/60 dark:hover:bg-muted/25",
				selected &&
					"bg-accent/60 text-accent-foreground ring-2 ring-primary/60 dark:bg-accent/40",
				fading && "opacity-0",
			)}
			{...dragProps}
			data-file-list-item
			onClick={(e) => onClick(e)}
			onDoubleClick={onDoubleClick}
			onKeyDown={(e) => {
				if (e.key !== "Enter") return;
				e.preventDefault();
				// KeyboardEvent 与 MouseEvent 同样携带修饰键状态
				(onDoubleClick ?? onClick)(e);
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
						"absolute top-2 left-2 z-10 transition-opacity",
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
					"absolute top-2 z-10 flex-col items-end gap-1",
					actionMenu ? "right-11 sm:right-2" : "right-2",
				)}
			/>
			{actionMenu ? (
				// biome-ignore lint/a11y/noStaticElementInteractions: non-interactive boundary prevents menu events from opening the parent card
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
				ref={hoverPreview.triggerRef}
				data-drag-preview-media
				className="mb-2 flex h-20 w-full items-center justify-center overflow-hidden rounded-xl"
			>
				<FileThumbnail
					file={item}
					size="lg"
					thumbnailPath={thumbnailPath}
					iconClassName="size-11"
					imageClassName="h-full w-full object-cover"
				/>
			</div>

			<div className="min-w-0 flex-1 space-y-1">
				<span
					data-drag-preview-name
					className="block w-full line-clamp-2 break-words text-sm leading-tight font-medium"
					title={item.name}
				>
					{item.name}
				</span>
				<div className="truncate text-xs text-muted-foreground">
					{subtitle ?? formatBytes(item.size ?? 0)}
				</div>
			</div>
			<TagChips
				tags={item.tags}
				maxVisible={2}
				className="mt-2 max-h-5 w-full justify-start overflow-hidden"
			/>
			<FileHoverPreview
				anchor={hoverPreview.triggerEl}
				file={item}
				open={hoverPreview.open}
				onClose={hoverPreview.close}
				thumbnailPath={thumbnailPath}
				cover
			/>
		</div>
	);
}
