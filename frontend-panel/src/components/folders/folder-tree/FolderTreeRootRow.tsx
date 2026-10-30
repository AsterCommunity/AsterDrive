import type { DragEvent } from "react";
import { useTranslation } from "react-i18next";
import { FolderTreeRow } from "./FolderTreeRow";

interface FolderTreeRootRowProps {
	active: boolean;
	dragOver?: boolean;
	expanded: boolean;
	onClick: () => void;
	onDragLeave?: (event: DragEvent<HTMLDivElement>) => void;
	onDragOver?: (event: DragEvent<HTMLDivElement>) => void;
	onDrop?: (event: DragEvent<HTMLDivElement>) => void;
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
		<FolderTreeRow
			active={active}
			dragOver={dragOver}
			dragHandlers={{ onDragOver, onDragLeave, onDrop }}
			expanded={expanded}
			label={t("root")}
			showToggle
			toggleLabel={t(expanded ? "collapse_tree" : "expand_tree")}
			onNavigate={onClick}
			onToggle={onToggle}
		/>
	);
}
