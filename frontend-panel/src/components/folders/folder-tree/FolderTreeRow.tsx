import type { ComponentProps, Ref } from "react";
import {
	FOLDER_TREE_INDENT_PX,
	FOLDER_TREE_ROW_OFFSET_PX,
} from "@/lib/constants";
import { folderTreeRowClass } from "@/lib/utils";
import { FolderTreeItemContent } from "./FolderTreeItemContent";

type FolderTreeRowProps = ComponentProps<typeof FolderTreeItemContent> & {
	active: boolean;
	depth?: number;
	folderId?: number;
	dragOver?: boolean;
	rowRef?: Ref<HTMLDivElement>;
	dragHandlers?: Pick<
		ComponentProps<"div">,
		"draggable" | "onDragStart" | "onDragOver" | "onDragLeave" | "onDrop"
	>;
};

/** Shared pointer hit area, indentation, active indicator and keyboard buttons. */
export function FolderTreeRow({
	active,
	depth = 0,
	folderId,
	dragOver = false,
	rowRef,
	dragHandlers,
	...content
}: FolderTreeRowProps) {
	return (
		/* biome-ignore lint/a11y/noStaticElementInteractions: 整行供指针导航，键盘与读屏使用内部导航和展开按钮。 */
		/* biome-ignore lint/a11y/useKeyWithClickEvents: 键盘交互由内部语义按钮承担。 */
		<div
			ref={rowRef}
			className={folderTreeRowClass(
				active,
				dragOver && "ring-2 ring-primary bg-accent/30",
				{ indicator: true },
			)}
			data-folder-tree-root-row={folderId == null ? "true" : undefined}
			data-folder-tree-row={folderId}
			style={
				folderId == null
					? undefined
					: {
							paddingLeft: `${depth * FOLDER_TREE_INDENT_PX + FOLDER_TREE_ROW_OFFSET_PX}px`,
						}
			}
			onClick={content.onNavigate}
			{...dragHandlers}
		>
			<FolderTreeItemContent {...content} />
		</div>
	);
}
