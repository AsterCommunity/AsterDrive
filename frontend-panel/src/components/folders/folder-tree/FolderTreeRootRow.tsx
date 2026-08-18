import type { DragEvent } from "react";
import { useTranslation } from "react-i18next";
import { folderTreeRowClass } from "@/lib/utils";
import { FolderTreeItemContent } from "./FolderTreeItemContent";

interface FolderTreeRootRowProps {
	active: boolean;
	dragOver: boolean;
	expanded: boolean;
	onClick: () => void;
	onDragLeave: (event: DragEvent<HTMLDivElement>) => void;
	onDragOver: (event: DragEvent<HTMLDivElement>) => void;
	onDrop: (event: DragEvent<HTMLDivElement>) => void;
	onToggle: () => void;
}

export function FolderTreeRootRow({
	active,
	dragOver,
	expanded,
	onClick,
	onDragLeave,
	onDragOver,
	onDrop,
	onToggle,
}: FolderTreeRootRowProps) {
	const { t } = useTranslation("files");

	return (
		/* biome-ignore lint/a11y/noStaticElementInteractions: 行同时是拖拽目标和整行点击导航区；键盘与读屏经内部语义按钮完成导航/展开 */
		/* biome-ignore lint/a11y/useKeyWithClickEvents: 键盘交互由行内语义按钮承担，行 onClick 仅服务指针用户 */
		<div
			className={folderTreeRowClass(
				active,
				dragOver && "ring-2 ring-primary bg-accent/30",
				{ indicator: true },
			)}
			data-folder-tree-root-row="true"
			onClick={onClick}
			onDragOver={onDragOver}
			onDragLeave={onDragLeave}
			onDrop={onDrop}
		>
			<FolderTreeItemContent
				expanded={expanded}
				label={t("root")}
				showToggle
				toggleLabel={t(expanded ? "collapse_tree" : "expand_tree")}
				onNavigate={onClick}
				onToggle={onToggle}
			/>
		</div>
	);
}
