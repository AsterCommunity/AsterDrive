import type { DragEvent } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "@/components/ui/icon";
import { folderTreeRowClass } from "@/lib/utils";

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
			<button
				type="button"
				aria-label={t(expanded ? "collapse_tree" : "expand_tree")}
				className="shrink-0 rounded p-0.5 text-muted-foreground hover:bg-accent-foreground/10 hover:text-foreground"
				onKeyDown={(event) => {
					if (event.key === "Enter" || event.key === " ") {
						event.stopPropagation();
					}
				}}
				onClick={(event) => {
					event.stopPropagation();
					onToggle();
				}}
			>
				<Icon
					name="CaretRight"
					className={`size-3 text-muted-foreground transition-transform duration-200 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none ${
						expanded ? "rotate-90" : "rotate-0"
					}`}
				/>
			</button>
			<button
				type="button"
				aria-label={t("root")}
				aria-expanded={expanded}
				className="flex min-w-0 flex-1 items-center gap-2 rounded-sm px-1 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
				onClick={onClick}
			>
				<Icon
					name={expanded ? "FolderOpen" : "Folder"}
					aria-hidden="true"
					className="size-4 shrink-0 text-muted-foreground"
				/>
				<span className="truncate">{t("root")}</span>
			</button>
		</div>
	);
}
