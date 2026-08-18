import { useState } from "react";
import { DRAG_SOURCE_MIME } from "@/lib/constants";
import {
	getInvalidInternalDropReason,
	hasInternalDragData,
	readInternalDragData,
	setInternalDragPreview,
	writeInternalDragData,
} from "@/lib/dragDrop";

export interface GridItemDragData {
	fileIds: number[];
	folderIds: number[];
}

interface UseGridItemDragDropOptions {
	itemId: number;
	isFolder: boolean;
	draggable?: boolean;
	dragData?: GridItemDragData;
	resolveDragData?: () => GridItemDragData;
	onDrop?: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number,
		targetPathIds: number[],
	) => void;
	targetPathIds?: number[];
}

/**
 * 网格项（文件卡片 / 文件夹项）共享的内部拖拽与文件夹落点逻辑。
 * 只处理应用内 drag data，外部文件拖入由上传层负责。
 */
export function useGridItemDragDrop({
	itemId,
	isFolder,
	draggable = true,
	dragData,
	resolveDragData,
	onDrop,
	targetPathIds = [],
}: UseGridItemDragDropOptions) {
	const [dragOver, setDragOver] = useState(false);

	const handleDragStart = (e: React.DragEvent) => {
		const data =
			resolveDragData?.() ??
			(dragData &&
			(dragData.fileIds.length > 0 || dragData.folderIds.length > 0)
				? dragData
				: isFolder
					? { fileIds: [], folderIds: [itemId] }
					: { fileIds: [itemId], folderIds: [] });
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
		if (getInvalidInternalDropReason(data, itemId, targetPathIds) !== null) {
			return;
		}
		onDrop?.(data.fileIds, data.folderIds, itemId, targetPathIds);
	};

	return {
		dragOver,
		dragProps: {
			draggable,
			onDragStart: draggable ? handleDragStart : undefined,
			onDragOver: draggable ? handleDragOver : undefined,
			onDragLeave: draggable ? handleDragLeave : undefined,
			onDrop: draggable ? handleDrop : undefined,
		},
	};
}
