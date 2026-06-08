import { useTranslation } from "react-i18next";
import { FileThumbnail } from "@/components/files/FileThumbnail";
import { TagChips } from "@/components/files/TagChips";
import { Icon } from "@/components/ui/icon";
import { formatBytes, formatDate } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { SearchEntry } from "./types";

interface GlobalSearchResultRowProps {
	active: boolean;
	entry: SearchEntry;
	index: number;
	onHover: (index: number) => void;
	onSelect: (entry: SearchEntry) => void;
	opening: boolean;
}

export function GlobalSearchResultRow({
	active,
	entry,
	index,
	onHover,
	onSelect,
	opening,
}: GlobalSearchResultRowProps) {
	const { t, i18n } = useTranslation(["files", "search"]);

	return (
		<button
			type="button"
			data-search-result-index={index}
			className={cn(
				"flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition-colors",
				active
					? "bg-primary/10 text-foreground ring-1 ring-primary/20"
					: "hover:bg-muted/60",
			)}
			onMouseEnter={() => onHover(index)}
			onClick={() => onSelect(entry)}
		>
			<div
				className={cn(
					"flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-border/60 bg-background/80",
					active && "border-primary/25 bg-primary/10 text-primary",
				)}
			>
				{entry.kind === "folder" ? (
					<Icon name="FolderOpen" className="size-4 text-amber-500" />
				) : (
					<FileThumbnail file={entry.item} size="md" />
				)}
			</div>
			<div className="min-w-0 flex-1 space-y-1">
				<div className="flex min-w-0 items-center gap-2">
					<span className="truncate text-sm font-medium">
						{entry.item.name}
					</span>
					<TagChips
						tags={entry.item.tags}
						maxVisible={2}
						className="hidden min-w-0 flex-nowrap overflow-hidden sm:flex"
					/>
					{entry.item.is_locked ? (
						<Icon
							name="Lock"
							className="size-3.5 shrink-0 text-muted-foreground"
						/>
					) : null}
				</div>
				<div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
					<span>
						{entry.kind === "folder"
							? t("files:folders_section")
							: t("files:files_section")}
					</span>
					{entry.kind === "file" ? (
						<>
							<span className="size-1 rounded-full bg-border" />
							<span>{formatBytes(entry.item.size)}</span>
						</>
					) : null}
					<span className="size-1 rounded-full bg-border" />
					<span>{formatDate(entry.item.updated_at, i18n)}</span>
				</div>
			</div>
			<div className="flex shrink-0 items-center gap-2 text-muted-foreground">
				{opening ? (
					<Icon name="Spinner" className="size-4 animate-spin" />
				) : (
					<Icon name="ArrowSquareOut" className="size-4" />
				)}
			</div>
		</button>
	);
}
