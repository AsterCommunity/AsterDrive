import type React from "react";
import { useRef, useState } from "react";
import { Icon } from "@/components/ui/icon";
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
				{showToggle ? (
					<button
						type="button"
						onDragEnter={(e) => e.preventDefault()}
						className="shrink-0 rounded p-0.5 text-muted-foreground hover:bg-accent-foreground/10 hover:text-foreground disabled:cursor-default disabled:hover:bg-transparent"
						onClick={(e) => {
							e.stopPropagation();
							onToggle(node.folder.id);
						}}
						disabled={isLoading}
					>
						{isLoading ? (
							<div className="size-3 animate-spin rounded-full border-2 border-t-muted-foreground border-muted-foreground/30" />
						) : (
							<Icon
								name="CaretRight"
								className={`size-3 text-muted-foreground transition-transform duration-200 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none ${
									isExpanded ? "rotate-90" : "rotate-0"
								}`}
							/>
						)}
					</button>
				) : (
					<span className="size-4 shrink-0" aria-hidden="true" />
				)}
				<button
					type="button"
					aria-label={node.folder.name}
					aria-expanded={showToggle ? isExpanded : undefined}
					className="flex min-w-0 flex-1 items-center gap-2 rounded-sm px-1 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
					onClick={() => onNavigate(node.folder.id, node.folder.name)}
				>
					{isExpanded ? (
						<Icon
							name="FolderOpen"
							aria-hidden="true"
							className="size-4 shrink-0 text-muted-foreground"
						/>
					) : (
						<Icon
							name="Folder"
							aria-hidden="true"
							className="size-4 shrink-0 text-muted-foreground"
						/>
					)}
					<span className="truncate">{node.folder.name}</span>
				</button>
			</div>
			<AnimatedTreeGroup open={isExpanded && node.childIds.length > 0}>
				{children}
			</AnimatedTreeGroup>
		</div>
	);
}
