import { useVirtualizer } from "@tanstack/react-virtual";
import { memo, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useFileBrowserContext } from "@/components/files/FileBrowserContext";
import {
	FileBrowserItemActionMenu,
	FileBrowserItemContextMenu,
} from "@/components/files/FileBrowserItemContextMenu";
import { FileCard } from "@/components/files/FileCard";
import { getCurrentSelectionDragData } from "@/components/files/selectionDragData";
import type { BrowserOpenMode } from "@/stores/fileStore";
import { useFileStore } from "@/stores/fileStore";
import type { FileListItem, FolderListItem } from "@/types/api";

interface FileGridProps {
	scrollElement?: HTMLDivElement | null;
}

const GRID_CLASSES =
	"grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-3";
const GRID_SECTION_HEADER_CLASSES =
	"flex items-center gap-2 px-1 text-xs font-semibold uppercase text-muted-foreground";
const GRID_HEADER_BOTTOM_GAP = 8;
const GRID_SECTION_TOP_GAP = 16;
const GRID_ROW_GAP = 12;
const GRID_HEADER_ESTIMATE = 28;
const GRID_ITEM_ROW_ESTIMATE = 176;

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

function getGridColumnCount(viewportWidth: number) {
	if (viewportWidth >= 1280) return 6;
	if (viewportWidth >= 1024) return 5;
	if (viewportWidth >= 768) return 4;
	if (viewportWidth >= 640) return 3;
	return 2;
}

interface BaseGridCardProps {
	browserOpenMode: BrowserOpenMode;
}

interface FolderGridCardProps extends BaseGridCardProps {
	breadcrumbPathIds: number[];
	fading: boolean;
	folder: FolderListItem;
	selectionActive: boolean;
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
	selectionActive,
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
	const actionMenu = useMemo(
		() => <FileBrowserItemActionMenu item={folder} isFolder />,
		[folder],
	);

	return (
		<FileBrowserItemContextMenu item={folder} isFolder>
			<FileCard
				item={folder}
				isFolder
				selected={selected}
				selectionActive={selectionActive}
				onSelect={() => toggleFolderSelection(folder.id)}
				onClick={() => {
					if (browserOpenMode === "double_click") {
						selectOnlyFolder(folder.id);
						return;
					}
					onFolderOpen(folder.id, folder.name);
				}}
				onDoubleClick={
					browserOpenMode === "double_click"
						? () => onFolderOpen(folder.id, folder.name)
						: undefined
				}
				resolveDragData={() => getCurrentSelectionDragData(folder.id, true)}
				onDrop={onMoveToFolder}
				targetPathIds={targetPathIds}
				fading={fading}
				actionMenu={actionMenu}
			/>
		</FileBrowserItemContextMenu>
	);
});

interface FileGridCardProps extends BaseGridCardProps {
	fading: boolean;
	file: FileListItem;
	selectionActive: boolean;
	onFileClick: (file: FileListItem) => void;
}

const FileGridCard = memo(function FileGridCard({
	browserOpenMode,
	fading,
	file,
	selectionActive,
	onFileClick,
}: FileGridCardProps) {
	const selected = useFileStore((s) => s.selectedFileIds.has(file.id));
	const selectOnlyFile = useFileStore((s) => s.selectOnlyFile);
	const toggleFileSelection = useFileStore((s) => s.toggleFileSelection);
	const actionMenu = useMemo(
		() => <FileBrowserItemActionMenu item={file} isFolder={false} />,
		[file],
	);

	return (
		<FileBrowserItemContextMenu item={file} isFolder={false}>
			<FileCard
				item={file}
				isFolder={false}
				selected={selected}
				selectionActive={selectionActive}
				onSelect={() => toggleFileSelection(file.id)}
				onClick={() => {
					if (browserOpenMode === "double_click") {
						selectOnlyFile(file.id);
						return;
					}
					onFileClick(file);
				}}
				onDoubleClick={
					browserOpenMode === "double_click"
						? () => onFileClick(file)
						: undefined
				}
				resolveDragData={() => getCurrentSelectionDragData(file.id, false)}
				fading={fading}
				actionMenu={actionMenu}
			/>
		</FileBrowserItemContextMenu>
	);
});

function FileGridComponent({ scrollElement }: FileGridProps) {
	const { t } = useTranslation("files");
	const {
		breadcrumbPathIds,
		browserOpenMode,
		fadingFileIds,
		fadingFolderIds,
		files,
		folders,
		onFileClick,
		onFolderOpen,
		onMoveToFolder,
	} = useFileBrowserContext();
	const selectionActive = useFileStore(
		(s) => s.selectedFileIds.size + s.selectedFolderIds.size > 0,
	);
	const [viewportWidth, setViewportWidth] = useState(() =>
		typeof window === "undefined" ? 1280 : window.innerWidth,
	);

	const hasBoth = folders.length > 0 && files.length > 0;

	useEffect(() => {
		if (typeof window === "undefined") return;

		const updateViewportWidth = () => {
			setViewportWidth(window.innerWidth);
		};

		updateViewportWidth();
		window.addEventListener("resize", updateViewportWidth);
		return () => window.removeEventListener("resize", updateViewportWidth);
	}, []);

	const renderFolderCard = (folder: FolderListItem) => (
		<FolderGridCard
			key={`folder-${folder.id}`}
			breadcrumbPathIds={breadcrumbPathIds}
			browserOpenMode={browserOpenMode}
			fading={fadingFolderIds?.has(folder.id) ?? false}
			folder={folder}
			selectionActive={selectionActive}
			onFolderOpen={onFolderOpen}
			onMoveToFolder={onMoveToFolder}
		/>
	);

	const renderFileCard = (file: FileListItem) => (
		<FileGridCard
			key={`file-${file.id}`}
			browserOpenMode={browserOpenMode}
			fading={fadingFileIds?.has(file.id) ?? false}
			file={file}
			selectionActive={selectionActive}
			onFileClick={onFileClick}
		/>
	);

	const columnCount = getGridColumnCount(viewportWidth);
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

	const virtualizer = useVirtualizer({
		count: scrollElement ? gridRows.length : 0,
		getScrollElement: () => scrollElement ?? null,
		estimateSize: (index) =>
			gridRows[index]?.type === "section-header"
				? GRID_HEADER_ESTIMATE
				: GRID_ITEM_ROW_ESTIMATE,
		overscan: 4,
	});

	useEffect(() => {
		if (!scrollElement) return;
		virtualizer.measure();
	}, [scrollElement, virtualizer]);

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
			<div className="px-4 py-3 md:p-5">
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
							style={{ paddingBottom: row.paddingBottom }}
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
		<div className="space-y-4 px-4 py-3 md:p-5">
			{folders.length > 0 && (
				<div className="space-y-2">
					{hasBoth && (
						<h3 className={GRID_SECTION_HEADER_CLASSES}>
							{t("folders_section")}
						</h3>
					)}
					<div className={GRID_CLASSES}>{folders.map(renderFolderCard)}</div>
				</div>
			)}

			{files.length > 0 && (
				<div className="space-y-2">
					{hasBoth && (
						<h3 className={GRID_SECTION_HEADER_CLASSES}>
							{t("files_section")}
						</h3>
					)}
					<div className={GRID_CLASSES}>{files.map(renderFileCard)}</div>
				</div>
			)}
		</div>
	);
}

export const FileGrid = memo(FileGridComponent);
