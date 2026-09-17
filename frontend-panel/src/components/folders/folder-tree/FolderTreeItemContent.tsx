import { FolderIconRenderer } from "@/components/files/FolderIconRenderer";
import { Icon } from "@/components/ui/icon";
import type { FolderIcon } from "@/types/api";

interface FolderTreeItemContentProps {
	expanded: boolean;
	icon?: FolderIcon;
	label: string;
	loading?: boolean;
	showToggle: boolean;
	toggleLabel: string;
	onNavigate: () => void;
	onToggle: () => void;
}

export function FolderTreeItemContent({
	expanded,
	icon,
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
					className="flex size-6 shrink-0 cursor-pointer items-center justify-center rounded text-muted-foreground hover:bg-accent-foreground/10 hover:text-foreground disabled:cursor-default disabled:hover:bg-transparent"
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
				className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-sm px-1 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
				onClick={(event) => {
					// 行容器本身也承担整行点击导航，避免重复触发
					event.stopPropagation();
					onNavigate();
				}}
			>
				<FolderIconRenderer icon={icon} className="size-4 text-base" />
				<span className="truncate">{label}</span>
			</button>
		</>
	);
}
