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
		/* biome-ignore lint/a11y/noStaticElementInteractions: row is a drag/drop target that contains semantic child buttons for actions */
		<div
			className={folderTreeRowClass(
				active,
				dragOver && "ring-2 ring-primary bg-accent/30",
			)}
			data-folder-tree-root-row="true"
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
