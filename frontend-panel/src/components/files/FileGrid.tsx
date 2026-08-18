import { useVirtualizer } from "@tanstack/react-virtual";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { useTranslation } from "react-i18next";
import { useFileBrowserContext } from "@/components/files/FileBrowserContext";
import {
	FileBrowserItemActionMenu,
	FileBrowserItemContextMenu,
} from "@/components/files/FileBrowserItemContextMenu";
import { FileCard } from "@/components/files/FileCard";
import { FolderGridItem } from "@/components/files/FolderGridItem";
import {
	getGridColumnCount,
	getGridTemplateColumns,
} from "@/components/files/gridLayout";
import { applySelectionModifiers } from "@/components/files/selectionClick";
import { getCurrentSelectionDragData } from "@/components/files/selectionDragData";
import { useFileListKeyboardNavigation } from "@/components/files/useFileListKeyboardNavigation";
import { formatDateUntil } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { BrowserOpenMode } from "@/stores/fileStore";
import { useFileStore } from "@/stores/fileStore";
import type { SelectionItemKey } from "@/stores/fileStore/selectionRange";
import type { FileListItem, FolderListItem } from "@/types/api";

interface FileGridProps {
	scrollElement?: HTMLDivElement | null;
}

// 列数由 gridLayout 按容器宽度计算（卡片宽度到上限就裂列），
// grid 模板由 JS 直接生成，与虚拟化切行天然一致，无需 CSS auto-fill。
const GRID_CLASSES = "grid gap-3";
const GRID_SECTION_HEADER_CLASSES =
	"flex items-center gap-2 px-1 text-xs font-semibold uppercase text-muted-foreground";
const GRID_HEADER_BOTTOM_GAP = 8;
const GRID_SECTION_TOP_GAP = 16;
const GRID_ROW_GAP = 12;
const GRID_HEADER_ESTIMATE = 28;
const GRID_ITEM_ROW_ESTIMATE = 176;
const GRID_FOLDER_ROW_ESTIMATE = 148;

type GridItem =
	| { type: "folder"; item: FolderListItem }
	| { type: "file"; item: FileListItem };

type GridRow =
	| {
			type: "section-header";
			key: string;
			label: string;
			paddingTop: number;
	  }
	| {
			type: "items";
			key: string;
			items: GridItem[];
			paddingBottom: number;
	  };

interface BaseGridCardProps {
	browserOpenMode: BrowserOpenMode;
}

interface FolderGridCardProps extends BaseGridCardProps {
	breadcrumbPathIds: number[];
	fading: boolean;
	folder: FolderListItem;
	readOnly: boolean;
	selectionEnabled: boolean;
	selectionActive: boolean;
	trashSubtitle?: string;
	onFolderOpen: (id: number, name: string) => void;
	onMoveToFolder?: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => void | Promise<void>;
}

const FolderGridCard = memo(function FolderGridCard({
	browserOpenMode,
	breadcrumbPathIds,
	fading,
	folder,
	readOnly,
	selectionEnabled,
	selectionActive,
	trashSubtitle,
	onFolderOpen,
	onMoveToFolder,
}: FolderGridCardProps) {
	const selected = useFileStore((s) => s.selectedFolderIds.has(folder.id));
	const selectOnlyFolder = useFileStore((s) => s.selectOnlyFolder);
	const toggleFolderSelection = useFileStore((s) => s.toggleFolderSelection);
	const targetPathIds = useMemo(
		() => [...breadcrumbPathIds, folder.id],
		[breadcrumbPathIds, folder.id],
	);
	const actionMenu = useMemo(() => {
		// trashMode（trashSubtitle 存在）下 readOnly 也要保留恢复/删除菜单
		if (readOnly && trashSubtitle == null) return null;
		return <FileBrowserItemActionMenu item={folder} isFolder />;
	}, [folder, readOnly, trashSubtitle]);

	const card = (
		<FolderGridItem
			item={folder}
			selected={selectionEnabled ? selected : false}
			selectionActive={selectionEnabled ? selectionActive : false}
			onSelect={
				selectionEnabled ? () => toggleFolderSelection(folder.id) : undefined
			}
			onClick={(e) => {
				if (
					selectionEnabled &&
					applySelectionModifiers(e, { type: "folder", id: folder.id })
				) {
					return;
				}
				if (
					!readOnly &&
					browserOpenMode === "double_click" &&
					selectionEnabled
				) {
					selectOnlyFolder(folder.id);
					return;
				}
				onFolderOpen(folder.id, folder.name);
			}}
			onDoubleClick={
				!readOnly && browserOpenMode === "double_click"
					? () => onFolderOpen(folder.id, folder.name)
					: undefined
			}
			resolveDragData={() => getCurrentSelectionDragData(folder.id, true)}
			onDrop={readOnly ? undefined : onMoveToFolder}
			targetPathIds={targetPathIds}
			fading={fading}
			draggable={!readOnly}
			selectable={selectionEnabled}
			actionMenu={actionMenu}
			subtitle={trashSubtitle}
		/>
	);

	if (readOnly && !selectionEnabled) return card;

	return (
		<FileBrowserItemContextMenu item={folder} isFolder>
			{card}
		</FileBrowserItemContextMenu>
	);
});

interface FileGridCardProps extends BaseGridCardProps {
	fading: boolean;
	file: FileListItem;
	readOnly: boolean;
	selectionEnabled: boolean;
	selectionActive: boolean;
	thumbnailPath?: string;
	trashSubtitle?: string;
	onFileClick: (file: FileListItem) => void;
}

const FileGridCard = memo(function FileGridCard({
	browserOpenMode,
	fading,
	file,
	readOnly,
	selectionEnabled,
	selectionActive,
	thumbnailPath,
	trashSubtitle,
	onFileClick,
}: FileGridCardProps) {
	const selected = useFileStore((s) => s.selectedFileIds.has(file.id));
	const selectOnlyFile = useFileStore((s) => s.selectOnlyFile);
	const toggleFileSelection = useFileStore((s) => s.toggleFileSelection);
	const actionMenu = useMemo(() => {
		return <FileBrowserItemActionMenu item={file} isFolder={false} />;
	}, [file]);

	const card = (
		<FileCard
			item={file}
			selected={selectionEnabled ? selected : false}
			selectionActive={selectionEnabled ? selectionActive : false}
			onSelect={
				selectionEnabled ? () => toggleFileSelection(file.id) : undefined
			}
			onClick={(e) => {
				if (
					selectionEnabled &&
					applySelectionModifiers(e, { type: "file", id: file.id })
				) {
					return;
				}
				if (
					!readOnly &&
					browserOpenMode === "double_click" &&
					selectionEnabled
				) {
					selectOnlyFile(file.id);
					return;
				}
				onFileClick(file);
			}}
			onDoubleClick={
				!readOnly && browserOpenMode === "double_click"
					? () => onFileClick(file)
					: undefined
			}
			resolveDragData={() => getCurrentSelectionDragData(file.id, false)}
			fading={fading}
			draggable={!readOnly}
			selectable={selectionEnabled}
			thumbnailPath={thumbnailPath}
			actionMenu={actionMenu}
			alwaysShowActionMenu={readOnly}
			subtitle={trashSubtitle}
		/>
	);

	if (readOnly && !selectionEnabled) return card;

	return (
		<FileBrowserItemContextMenu item={file} isFolder={false}>
			{card}
		</FileBrowserItemContextMenu>
	);
});

function FileGridComponent({ scrollElement }: FileGridProps) {
	const { t, i18n } = useTranslation("files");
	const {
		breadcrumbPathIds,
		browserOpenMode,
		fadingFileIds,
		fadingFolderIds,
		files,
		folders,
		getThumbnailPath,
		getTrashMeta,
		onFileClick,
		onFolderOpen,
		onMoveToFolder,
		readOnly = false,
		selectionEnabled = !readOnly,
		trashMode = false,
	} = useFileBrowserContext();
	const selectionActive = useFileStore(
		(s) => s.selectedFileIds.size + s.selectedFolderIds.size > 0,
	);
	// 列数跟随容器实际宽度（侧边栏开合、窗口缩放都会改变它），
	// 而不是 window.innerWidth——断点式列数会让宽屏下单卡被拉得过宽。
	//
	// 注意不能做防抖：列数变化必须当帧同步到渲染，否则容器宽度与切行
	// 不一致期间会出现"超宽卡 + 行尾空缺"的错乱布局。这里只在列数
	// 实际变化时才 setState（宽度微调由 1fr 轨道自行吸收），
	// 相同值 React 自动 bailout，不会有多余重渲染。
	const [columnCount, setColumnCount] = useState(1);
	const resizeObserverRef = useRef<ResizeObserver | null>(null);
	const columnCountRef = useRef(1);
	const hasMeasuredRef = useRef(false);

	// scrollElement 就绪前后渲染的是不同的容器元素（虚拟化/非虚拟化分支），
	// 用 callback ref 保证 observer 始终挂在当前真实容器上。
	const containerRef = useCallback((element: HTMLDivElement | null) => {
		resizeObserverRef.current?.disconnect();
		resizeObserverRef.current = null;
		if (!element || typeof ResizeObserver === "undefined") return;
		const observer = new ResizeObserver((entries) => {
			const next = getGridColumnCount(entries[0]?.contentRect.width ?? 0);
			if (columnCountRef.current === next) return;
			columnCountRef.current = next;
			// 首次测量同步提交：ResizeObserver 回调处于渲染管线内（paint 之前），
			// flushSync 让 React 当场完成渲染，paint 时就是正确列数——
			// 否则 setState 走并发调度，首帧会先 paint 出 1 列再展开，造成闪烁。
			if (!hasMeasuredRef.current) {
				hasMeasuredRef.current = true;
				flushSync(() => setColumnCount(next));
				return;
			}
			// 裂列重排是硬跳变：用 View Transitions 对网格容器做一次快照
			// crossfade 缓冲（旧/新布局各一张快照交叉淡化，不重建 DOM、
			// 动画在合成层播放不阻塞交互）。不支持的浏览器与 reduced-motion
			// 直接更新，退化为无过渡。
			if (
				typeof document.startViewTransition === "function" &&
				!window.matchMedia("(prefers-reduced-motion: reduce)").matches
			) {
				document.startViewTransition(() => {
					flushSync(() => setColumnCount(next));
				});
				return;
			}
			setColumnCount(next);
		});
		observer.observe(element);
		resizeObserverRef.current = observer;
	}, []);

	const hasBoth = folders.length > 0 && files.length > 0;

	// D8 网格首次加载入场：内容从空变为非空（或挂载即非空）时播一次。
	// 类加上后不移除：CSS animation 不会因重渲染重播，且 fill-mode
	// backwards 播完回到自然样式，无 transform 残留。
	const hasContent = folders.length > 0 || files.length > 0;
	const [entering, setEntering] = useState(hasContent);

	useEffect(() => {
		// 已为 true 时 React 对相同值 setState 直接 bailout，不会重渲染
		if (hasContent) setEntering(true);
	}, [hasContent]);

	const renderFolderCard = (folder: FolderListItem) => {
		const trashMeta = trashMode
			? getTrashMeta?.("folder", folder.id)
			: undefined;
		return (
			<FolderGridCard
				key={`folder-${folder.id}`}
				breadcrumbPathIds={breadcrumbPathIds}
				browserOpenMode={browserOpenMode}
				fading={fadingFolderIds?.has(folder.id) ?? false}
				folder={folder}
				readOnly={readOnly}
				selectionEnabled={selectionEnabled}
				selectionActive={selectionActive}
				trashSubtitle={
					trashMeta ? formatDateUntil(trashMeta.expiresAt, i18n) : undefined
				}
				onFolderOpen={onFolderOpen}
				onMoveToFolder={onMoveToFolder}
			/>
		);
	};

	const renderFileCard = (file: FileListItem) => {
		const trashMeta = trashMode ? getTrashMeta?.("file", file.id) : undefined;
		return (
			<FileGridCard
				key={`file-${file.id}`}
				browserOpenMode={browserOpenMode}
				fading={fadingFileIds?.has(file.id) ?? false}
				file={file}
				readOnly={readOnly}
				selectionEnabled={selectionEnabled}
				selectionActive={selectionActive}
				thumbnailPath={getThumbnailPath?.(file)}
				trashSubtitle={
					trashMeta ? formatDateUntil(trashMeta.expiresAt, i18n) : undefined
				}
				onFileClick={onFileClick}
			/>
		);
	};

	const gridRows = useMemo(() => {
		const rows: GridRow[] = [];

		const appendSectionRows = (
			type: GridItem["type"],
			items: FolderListItem[] | FileListItem[],
			label: string,
		) => {
			if (items.length === 0) return;

			if (hasBoth) {
				rows.push({
					type: "section-header",
					key: `${type}-header`,
					label,
					paddingTop: rows.length === 0 ? 0 : GRID_SECTION_TOP_GAP,
				});
			}

			for (let index = 0; index < items.length; index += columnCount) {
				const slice = items.slice(index, index + columnCount);
				rows.push({
					type: "items",
					key: `${type}-row-${slice[0]?.id ?? index}`,
					items: slice.map((item) => ({ type, item })) as GridItem[],
					paddingBottom: index + columnCount < items.length ? GRID_ROW_GAP : 0,
				});
			}
		};

		appendSectionRows("folder", folders, t("folders_section"));
		appendSectionRows("file", files, t("files_section"));

		return rows;
	}, [columnCount, files, folders, hasBoth, t]);

	const gridTemplateColumns = getGridTemplateColumns(columnCount);

	const virtualizer = useVirtualizer({
		count: scrollElement ? gridRows.length : 0,
		getScrollElement: () => scrollElement ?? null,
		estimateSize: (index) => {
			const row = gridRows[index];
			if (!row) return GRID_ITEM_ROW_ESTIMATE;
			if (row.type === "section-header") return GRID_HEADER_ESTIMATE;
			return row.items[0]?.type === "folder"
				? GRID_FOLDER_ROW_ESTIMATE
				: GRID_ITEM_ROW_ESTIMATE;
		},
		overscan: 4,
	});

	useEffect(() => {
		if (!scrollElement) return;
		virtualizer.measure();
	}, [scrollElement, virtualizer]);

	// 键盘导航后让焦点项滚动进视口：从 gridRows 反查目标所在行
	// （section header 也占行，不能直接按下标换算）。
	const scrollToItem = useCallback(
		(key: SelectionItemKey) => {
			const rowIndex = gridRows.findIndex(
				(row) =>
					row.type === "items" &&
					row.items.some(
						(entry) => entry.type === key.type && entry.item.id === key.id,
					),
			);
			if (rowIndex >= 0) {
				virtualizer.scrollToIndex(rowIndex, { align: "auto" });
			}
		},
		[gridRows, virtualizer],
	);

	useFileListKeyboardNavigation({
		columnCount,
		enabled: selectionEnabled,
		scrollToItem: scrollElement ? scrollToItem : undefined,
		onOpenFocused: readOnly
			? undefined
			: (key) => {
					if (key.type === "folder") {
						const folder = folders.find((entry) => entry.id === key.id);
						if (folder) onFolderOpen(folder.id, folder.name);
					} else {
						const file = files.find((entry) => entry.id === key.id);
						if (file) onFileClick(file);
					}
				},
	});

	if (scrollElement) {
		const virtualRows = virtualizer.getVirtualItems();
		const firstVirtualRow = virtualRows[0];
		const lastVirtualRow = virtualRows[virtualRows.length - 1];
		const paddingTop = firstVirtualRow?.start ?? 0;
		const paddingBottom = Math.max(
			0,
			virtualizer.getTotalSize() - (lastVirtualRow?.end ?? 0),
		);

		return (
			<div
				ref={containerRef}
				className={cn(
					"file-grid-vt px-4 py-3 md:p-5",
					entering && "file-browser-enter",
				)}
			>
				{paddingTop > 0 && <div aria-hidden style={{ height: paddingTop }} />}
				{virtualRows.map((virtualRow) => {
					const row = gridRows[virtualRow.index];
					if (!row) return null;
					if (row.type === "section-header") {
						return (
							<h3
								key={row.key}
								ref={virtualizer.measureElement}
								data-index={virtualRow.index}
								className={GRID_SECTION_HEADER_CLASSES}
								style={{
									paddingTop: row.paddingTop,
									paddingBottom: GRID_HEADER_BOTTOM_GAP,
								}}
							>
								{row.label}
							</h3>
						);
					}

					return (
						<div
							key={row.key}
							ref={virtualizer.measureElement}
							data-index={virtualRow.index}
							className={GRID_CLASSES}
							style={{
								gridTemplateColumns,
								paddingBottom: row.paddingBottom,
							}}
						>
							{row.items.map((item) =>
								item.type === "folder"
									? renderFolderCard(item.item)
									: renderFileCard(item.item),
							)}
						</div>
					);
				})}
				{paddingBottom > 0 && (
					<div aria-hidden style={{ height: paddingBottom }} />
				)}
			</div>
		);
	}

	return (
		<div ref={containerRef} className="file-grid-vt space-y-4 px-4 py-3 md:p-5">
			{folders.length > 0 && (
				<div className={cn("space-y-2", entering && "file-browser-enter")}>
					{hasBoth && (
						<h3 className={GRID_SECTION_HEADER_CLASSES}>
							{t("folders_section")}
						</h3>
					)}
					<div className={GRID_CLASSES} style={{ gridTemplateColumns }}>
						{folders.map(renderFolderCard)}
					</div>
				</div>
			)}

			{files.length > 0 && (
				<div
					className={cn(
						"space-y-2",
						entering && "file-browser-enter",
						// 两区错开 80ms；只有一个区时不加延迟，单区内容直接入场
						entering && hasBoth && "file-browser-enter-delayed",
					)}
				>
					{hasBoth && (
						<h3 className={GRID_SECTION_HEADER_CLASSES}>
							{t("files_section")}
						</h3>
					)}
					<div className={GRID_CLASSES} style={{ gridTemplateColumns }}>
						{files.map(renderFileCard)}
					</div>
				</div>
			)}
		</div>
	);
}

export const FileGrid = memo(FileGridComponent);
