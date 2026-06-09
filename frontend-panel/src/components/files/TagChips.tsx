import type { ReactNode } from "react";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import type { TagSummary } from "@/types/api";
import { safeTagColor } from "./tagColors";

export function TagChip({
	className,
	onRemove,
	tag,
	removeLabel,
}: {
	className?: string;
	onRemove?: () => void;
	removeLabel?: string;
	tag: TagSummary;
}) {
	return (
		<span
			className={cn(
				"inline-flex h-5 max-w-32 shrink-0 items-center gap-1.5 rounded-md border border-border/65 bg-muted/45 px-1.5 text-[11px] font-medium leading-none text-muted-foreground",
				className,
			)}
			title={tag.name}
		>
			<span
				className="size-2 shrink-0 rounded-full ring-1 ring-black/10"
				style={{ backgroundColor: safeTagColor(tag.color) }}
				aria-hidden
			/>
			<span className="min-w-0 truncate">{tag.name}</span>
			{onRemove ? (
				<button
					type="button"
					className="-mr-0.5 flex size-3.5 shrink-0 items-center justify-center rounded-sm text-muted-foreground/75 transition-colors hover:bg-background/80 hover:text-foreground"
					onClick={onRemove}
					aria-label={removeLabel || "Remove tag"}
				>
					<Icon name="X" className="size-3" />
				</button>
			) : null}
		</span>
	);
}

export function TagChips({
	className,
	empty,
	maxVisible = 3,
	tags,
}: {
	className?: string;
	empty?: ReactNode;
	maxVisible?: number;
	tags?: TagSummary[] | null;
}) {
	const renderedTags = tags ?? [];
	const safeMaxVisible = Math.max(0, Math.floor(Number(maxVisible) || 0));

	if (renderedTags.length === 0) {
		return empty ?? null;
	}

	const visibleTags = renderedTags.slice(0, safeMaxVisible);
	const hiddenCount = renderedTags.length - visibleTags.length;

	return (
		<div className={cn("flex min-w-0 flex-wrap items-center gap-1", className)}>
			{visibleTags.map((tag) => (
				<TagChip key={tag.id} tag={tag} />
			))}
			{hiddenCount > 0 ? (
				<span className="inline-flex h-5 shrink-0 items-center rounded-md border border-border/60 bg-muted/35 px-1.5 text-[11px] font-medium leading-none text-muted-foreground">
					+{hiddenCount}
				</span>
			) : null}
		</div>
	);
}
