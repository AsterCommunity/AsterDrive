import { useVirtualizer } from "@tanstack/react-virtual";
import type React from "react";
import { memo, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useFileBrowserContext } from "@/components/files/FileBrowserContext";
import { FileBrowserItemContextMenu } from "@/components/files/FileBrowserItemContextMenu";
import {
	FileNameCell,
	FileSizeCell,
	FolderNameCell,
	FolderSizeCell,
	UpdatedAtCell,
} from "@/components/files/FileTableCells";
import { getCurrentSelectionDragData } from "@/components/files/selectionDragData";
import { Icon } from "@/components/ui/icon";
import { ItemCheckbox } from "@/components/ui/item-checkbox";
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";
import { DRAG_SOURCE_MIME } from "@/lib/constants";
import {
	getInvalidInternalDropReason,
	hasInternalDragData,
	readInternalDragData,
	setInternalDragPreview,
	writeInternalDragData,
} from "@/lib/dragDrop";
import { cn } from "@/lib/utils";
import type { BrowserOpenMode, SortBy } from "@/stores/fileStore";
import { useFileStore } from "@/stores/fileStore";
import type { FileListItem, FolderListItem } from "@/types/api";

interface FileTableProps {
	scrollElement?: HTMLDivElement | null;
}

type TableRowItem =
	| { type: "folder"; item: FolderListItem }
	| { type: "file"; item: FileListItem };

const TABLE_COLUMN_COUNT = 4;
const TABLE_ROW_ESTIMATE = 52;

function SortIcon({
	column,
	current,
	order,
}: {
	column: SortBy;
	current: SortBy;
	order: "asc" | "desc";
}) {
	if (column !== current) return null;
	return order === "asc" ? (
		<Icon name="SortAscending" className="h-3 w-3 ml-1" />
	) : (
		<Icon name="SortDescending" className="h-3 w-3 ml-1" />
	);
}

interface BaseTableRowProps {
	browserOpenMode: BrowserOpenMode;
}

interface FolderTableDataRowProps extends BaseTableRowProps {
	breadcrumbPathIds: number[];
	fading: boolean;
	folder: FolderListItem;
	onFolderOpen: (id: number, name: string) => void;
	onMoveToFolder?: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => void | Promise<void>;
}

const FolderTableDataRow = memo(function FolderTableDataRow({
	breadcrumbPathIds,
	browserOpenMode,
	fading,
	folder,
	onFolderOpen,
	onMoveToFolder,
}: FolderTableDataRowProps) {
	const selected = useFileStore((s) => s.selectedFolderIds.has(folder.id));
	const selectOnlyFolder = useFileStore((s) => s.selectOnlyFolder);
	const toggleFolderSelection = useFileStore((s) => s.toggleFolderSelection);
	const [dragOver, setDragOver] = useState(false);
	const targetPathIds = useMemo(
		() => [...breadcrumbPathIds, folder.id],
		[breadcrumbPathIds, folder.id],
	);

	const handleDragStart = (e: React.DragEvent<HTMLTableRowElement>) => {
		const data = getCurrentSelectionDragData(folder.id, true);
		writeInternalDragData(e.dataTransfer, data);
		setInternalDragPreview(e, {
			variant: "list-row",
			itemCount: data.fileIds.length + data.folderIds.length,
		});
	};

	const handleDragOver = (e: React.DragEvent<HTMLTableRowElement>) => {
		if (
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

	const handleDrop = (e: React.DragEvent<HTMLTableRowElement>) => {
		setDragOver(false);
		if (e.dataTransfer.types.includes(DRAG_SOURCE_MIME)) {
			return;
		}
		e.preventDefault();
		e.stopPropagation();
		const data = readInternalDragData(e.dataTransfer);
		if (!data) return;
		if (getInvalidInternalDropReason(data, folder.id, targetPathIds) !== null) {
			return;
		}
		void onMoveToFolder?.(data.fileIds, data.folderIds, folder.id);
	};

	return (
		<FileBrowserItemContextMenu renderTrigger item={folder} isFolder>
			<TableRow
				data-folder-drop-target="true"
				className={cn(
					"cursor-pointer transition-[background-color,box-shadow,opacity] duration-150 ease-out",
					dragOver && "ring-2 ring-primary bg-accent/30",
					fading && "opacity-0",
				)}
				draggable
				onDragStart={handleDragStart}
				onDragOver={handleDragOver}
				onDragLeave={() => setDragOver(false)}
				onDrop={handleDrop}
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
			>
				<TableCell
					className="w-12 pr-0 first:pl-3 md:first:pl-3"
					onClick={(e) => e.stopPropagation()}
				>
					<div className="flex justify-center">
						<ItemCheckbox
							checked={selected}
							onChange={() => toggleFolderSelection(folder.id)}
						/>
					</div>
				</TableCell>
				<FolderNameCell folder={folder} />
				<FolderSizeCell />
				<UpdatedAtCell updatedAt={folder.updated_at} />
			</TableRow>
		</FileBrowserItemContextMenu>
	);
});

interface FileTableDataRowProps extends BaseTableRowProps {
	fading: boolean;
	file: FileListItem;
	onFileClick: (file: FileListItem) => void;
}

const FileTableDataRow = memo(function FileTableDataRow({
	browserOpenMode,
	fading,
	file,
	onFileClick,
}: FileTableDataRowProps) {
	const selected = useFileStore((s) => s.selectedFileIds.has(file.id));
	const selectOnlyFile = useFileStore((s) => s.selectOnlyFile);
	const toggleFileSelection = useFileStore((s) => s.toggleFileSelection);

	const handleDragStart = (e: React.DragEvent<HTMLTableRowElement>) => {
		const data = getCurrentSelectionDragData(file.id, false);
		writeInternalDragData(e.dataTransfer, data);
		setInternalDragPreview(e, {
			variant: "list-row",
			itemCount: data.fileIds.length + data.folderIds.length,
		});
	};

	return (
		<FileBrowserItemContextMenu renderTrigger item={file} isFolder={false}>
			<TableRow
				className={cn(
					"cursor-pointer transition-[background-color,box-shadow,opacity] duration-150 ease-out",
					fading && "opacity-0",
				)}
				draggable
				onDragStart={handleDragStart}
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
			>
				<TableCell
					className="w-12 pr-0 first:pl-3 md:first:pl-3"
					onClick={(e) => e.stopPropagation()}
				>
					<div className="flex justify-center">
						<ItemCheckbox
							checked={selected}
							onChange={() => toggleFileSelection(file.id)}
						/>
					</div>
				</TableCell>
				<FileNameCell file={file} />
				<FileSizeCell size={file.size} />
				<UpdatedAtCell updatedAt={file.updated_at} />
			</TableRow>
		</FileBrowserItemContextMenu>
	);
});

function FileTableComponent({ scrollElement }: FileTableProps) {
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
	const selectedFileIds = useFileStore((s) => s.selectedFileIds);
	const selectedFolderIds = useFileStore((s) => s.selectedFolderIds);
	const selectItems = useFileStore((s) => s.selectItems);
	const clearSelection = useFileStore((s) => s.clearSelection);
	const sortBy = useFileStore((s) => s.sortBy);
	const sortOrder = useFileStore((s) => s.sortOrder);
	const setSortBy = useFileStore((s) => s.setSortBy);
	const setSortOrder = useFileStore((s) => s.setSortOrder);

	const allSelected =
		folders.length + files.length > 0 &&
		files.every((file) => selectedFileIds.has(file.id)) &&
		folders.every((folder) => selectedFolderIds.has(folder.id)) &&
		selectedFileIds.size + selectedFolderIds.size ===
			files.length + folders.length;

	const handleSort = (col: SortBy) => {
		if (sortBy === col) {
			setSortOrder(sortOrder === "asc" ? "desc" : "asc");
		} else {
			setSortBy(col);
		}
	};

	const handleSelectAll = () => {
		if (allSelected) clearSelection();
		else
			selectItems(
				files.map((file) => file.id),
				folders.map((folder) => folder.id),
			);
	};

	const renderFolderRow = (folder: FolderListItem) => (
		<FolderTableDataRow
			key={`folder-${folder.id}`}
			breadcrumbPathIds={breadcrumbPathIds}
			browserOpenMode={browserOpenMode}
			fading={fadingFolderIds?.has(folder.id) ?? false}
			folder={folder}
			onFolderOpen={onFolderOpen}
			onMoveToFolder={onMoveToFolder}
		/>
	);

	const renderFileRow = (file: FileListItem) => (
		<FileTableDataRow
			key={`file-${file.id}`}
			browserOpenMode={browserOpenMode}
			fading={fadingFileIds?.has(file.id) ?? false}
			file={file}
			onFileClick={onFileClick}
		/>
	);

	const tableRows = useMemo<TableRowItem[]>(
		() => [
			...folders.map((item) => ({ type: "folder", item }) as const),
			...files.map((item) => ({ type: "file", item }) as const),
		],
		[files, folders],
	);

	const virtualizer = useVirtualizer({
		count: scrollElement ? tableRows.length : 0,
		getScrollElement: () => scrollElement ?? null,
		estimateSize: () => TABLE_ROW_ESTIMATE,
		overscan: 10,
	});

	useEffect(() => {
		if (!scrollElement) return;
		virtualizer.measure();
	}, [scrollElement, virtualizer]);

	const renderSpacerRow = (key: string, height: number) => (
		<TableRow key={key} aria-hidden className="border-0 hover:bg-transparent">
			<TableCell
				colSpan={TABLE_COLUMN_COUNT}
				className="p-0 first:pl-0 last:pr-0 md:first:pl-0 md:last:pr-0"
				style={{ height }}
			/>
		</TableRow>
	);

	const renderRows = () => {
		if (!scrollElement) {
			return (
				<>
					{folders.map(renderFolderRow)}
					{files.map(renderFileRow)}
				</>
			);
		}

		const virtualRows = virtualizer.getVirtualItems();
		const firstVirtualRow = virtualRows[0];
		const lastVirtualRow = virtualRows[virtualRows.length - 1];
		const paddingTop = firstVirtualRow?.start ?? 0;
		const paddingBottom = Math.max(
			0,
			virtualizer.getTotalSize() - (lastVirtualRow?.end ?? 0),
		);

		return (
			<>
				{paddingTop > 0 && renderSpacerRow("spacer-top", paddingTop)}
				{virtualRows.map((virtualRow) => {
					const row = tableRows[virtualRow.index];
					if (!row) return null;
					return row.type === "folder"
						? renderFolderRow(row.item)
						: renderFileRow(row.item);
				})}
				{paddingBottom > 0 && renderSpacerRow("spacer-bottom", paddingBottom)}
			</>
		);
	};

	return (
		<Table>
			<TableHeader>
				<TableRow>
					<TableHead className="w-12 pr-0 first:pl-3 md:first:pl-3">
						<div className="flex justify-center">
							<ItemCheckbox checked={allSelected} onChange={handleSelectAll} />
						</div>
					</TableHead>
					<TableHead
						className="cursor-pointer select-none"
						onClick={() => handleSort("name")}
					>
						<div className="flex items-center">
							{t("core:name")}
							<SortIcon column="name" current={sortBy} order={sortOrder} />
						</div>
					</TableHead>
					<TableHead
						className="w-[100px] cursor-pointer select-none"
						onClick={() => handleSort("size")}
					>
						<div className="flex items-center">
							{t("core:size")}
							<SortIcon column="size" current={sortBy} order={sortOrder} />
						</div>
					</TableHead>
					<TableHead
						className="cursor-pointer select-none"
						onClick={() => handleSort("created_at")}
					>
						<div className="flex items-center">
							{t("core:date")}
							<SortIcon
								column="created_at"
								current={sortBy}
								order={sortOrder}
							/>
						</div>
					</TableHead>
				</TableRow>
			</TableHeader>
			<TableBody>{renderRows()}</TableBody>
		</Table>
	);
}

export const FileTable = memo(FileTableComponent);
