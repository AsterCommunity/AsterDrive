import { Icon } from "@/components/ui/icon";

interface FolderTreeItemContentProps {
	expanded: boolean;
	label: string;
	loading?: boolean;
	showToggle: boolean;
	toggleLabel: string;
	onNavigate: () => void;
	onToggle: () => void;
}

export function FolderTreeItemContent({
	expanded,
	label,
	loading = false,
	showToggle,
	toggleLabel,
	onNavigate,
	onToggle,
}: FolderTreeItemContentProps) {
	return (
		<>
			{showToggle ? (
				<button
					type="button"
					aria-label={toggleLabel}
					aria-expanded={expanded}
					className="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-accent-foreground/10 hover:text-foreground disabled:cursor-default disabled:hover:bg-transparent"
					onKeyDown={(event) => {
						if (event.key === "Enter" || event.key === " ") {
							event.stopPropagation();
						}
					}}
					onClick={(event) => {
						event.stopPropagation();
						onToggle();
					}}
					disabled={loading}
				>
					{loading ? (
						<span className="block size-3 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-muted-foreground" />
					) : (
						<Icon
							name="CaretRight"
							className={`size-3 text-muted-foreground transition-transform duration-200 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none ${
								expanded ? "rotate-90" : "rotate-0"
							}`}
						/>
					)}
				</button>
			) : (
				<span className="size-4 shrink-0" aria-hidden="true" />
			)}
			<button
				type="button"
				aria-label={label}
				className="flex min-w-0 flex-1 items-center gap-2 rounded-sm px-1 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
				onClick={onNavigate}
			>
				<Icon
					name={expanded ? "FolderOpen" : "Folder"}
					aria-hidden="true"
					className="size-4 shrink-0 text-muted-foreground"
				/>
				<span className="truncate">{label}</span>
			</button>
		</>
	);
}
