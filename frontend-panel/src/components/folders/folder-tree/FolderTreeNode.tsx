import type React from "react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	DRAG_SOURCE_MIME,
	FOLDER_TREE_INDENT_PX,
	FOLDER_TREE_ROW_OFFSET_PX,
} from "@/lib/constants";
import {
	getInvalidInternalDropReason,
	hasInternalDragData,
	readInternalDragData,
	setInternalDragPreview,
	writeInternalDragData,
} from "@/lib/dragDrop";
import { folderTreeRowClass } from "@/lib/utils";
import { AnimatedTreeGroup } from "./AnimatedTreeGroup";
import { FolderTreeItemContent } from "./FolderTreeItemContent";
import type { TreeNodeProps } from "./types";

export function FolderTreeNodeRow({
	currentFolderId,
	depth,
	expandedIds,
	loadedIds,
	loadingIds,
	nodeId,
	nodeMap,
	onDragHoverEnd,
	onDragHoverStart,
	onDrop,
	onNavigate,
	onToggle,
	children,
}: TreeNodeProps) {
	const { t } = useTranslation("files");
	const node = nodeMap.get(nodeId);
	const [dragOver, setDragOver] = useState(false);
	const rowRef = useRef<HTMLDivElement | null>(null);

	if (!node) return null;

	const isActive = currentFolderId === node.folder.id;
	const isExpanded = expandedIds.has(node.folder.id);
	const isLoading = loadingIds.has(node.folder.id);
	const isLoaded = loadedIds.has(node.folder.id);
	const showToggle = isLoading || !isLoaded || node.childIds.length > 0;

	const handleDragStart = (e: React.DragEvent) => {
		writeInternalDragData(e.dataTransfer, {
			fileIds: [],
			folderIds: [node.folder.id],
		});
		e.dataTransfer.setData(DRAG_SOURCE_MIME, "tree");
		setInternalDragPreview(e, { itemCount: 1 });
	};

	const handleDragOver = (e: React.DragEvent) => {
		if (!hasInternalDragData(e.dataTransfer)) return;
		e.preventDefault();
		e.dataTransfer.dropEffect = "move";
		setDragOver(true);
		onDragHoverStart(node.folder.id);
	};

	const handleDragLeave = (e: React.DragEvent) => {
		const nextTarget = e.relatedTarget;
		if (nextTarget instanceof Node && rowRef.current?.contains(nextTarget)) {
			return;
		}
		setDragOver(false);
		onDragHoverEnd(node.folder.id);
	};

	const handleDrop = (e: React.DragEvent) => {
		setDragOver(false);
		onDragHoverEnd(node.folder.id);
		e.preventDefault();
		const data = readInternalDragData(e.dataTransfer);
		if (!data) return;
		const targetPathIds = (() => {
			const pathIds: number[] = [];
			let cursor: number | null = node.folder.id;

			while (cursor !== null) {
				pathIds.unshift(cursor);
				cursor = nodeMap.get(cursor)?.parentId ?? null;
			}

			return pathIds;
		})();
		if (
			getInvalidInternalDropReason(data, node.folder.id, targetPathIds) !== null
		) {
			return;
		}
		onDrop(data.fileIds, data.folderIds, node.folder.id, targetPathIds);
	};

	return (
		<div>
			{/* biome-ignore lint/a11y/noStaticElementInteractions: row is a drag/drop target that contains semantic child buttons for actions */}
			<div
				ref={rowRef}
				draggable
				className={folderTreeRowClass(
					isActive,
					dragOver && "ring-2 ring-primary bg-accent/30",
				)}
				data-folder-tree-row={node.folder.id}
				style={{
					paddingLeft: `${depth * FOLDER_TREE_INDENT_PX + FOLDER_TREE_ROW_OFFSET_PX}px`,
				}}
				onDragStart={handleDragStart}
				onDragOver={handleDragOver}
				onDragLeave={handleDragLeave}
				onDrop={handleDrop}
			>
				<FolderTreeItemContent
					expanded={isExpanded}
					label={node.folder.name}
					loading={isLoading}
					showToggle={showToggle}
					toggleLabel={t(isExpanded ? "collapse_tree" : "expand_tree")}
					onNavigate={() => onNavigate(node.folder.id, node.folder.name)}
					onToggle={() => onToggle(node.folder.id)}
				/>
			</div>
			<AnimatedTreeGroup open={isExpanded && node.childIds.length > 0}>
				{children}
			</AnimatedTreeGroup>
		</div>
	);
}
