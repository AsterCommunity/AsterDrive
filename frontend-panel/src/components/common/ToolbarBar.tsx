import type { ReactNode } from "react";

interface ToolbarBarProps {
	left: ReactNode;
	right?: ReactNode;
}

export function ToolbarBar({ left, right }: ToolbarBarProps) {
	return (
		<div className="border-b border-border/60 bg-card/90 px-3 py-2 shadow-xs dark:bg-card/70 dark:shadow-none sm:px-4 sm:py-2.5">
			<div className="flex h-9 min-w-0 items-center gap-1.5 rounded-lg bg-background/70 px-2.5 shadow-xs ring-1 ring-border/55 transition-[background-color,box-shadow] dark:bg-background/25 dark:shadow-none dark:ring-border/60 sm:h-10 sm:gap-2 sm:px-3">
				<div className="flex min-w-0 flex-1 items-center gap-1.5 sm:gap-2">
					{left}
				</div>
				{right && (
					<div className="flex shrink-0 items-center gap-1 border-l border-border/55 pl-1.5 transition-[border-color] sm:gap-2 sm:pl-2">
						{right}
					</div>
				)}
			</div>
		</div>
	);
}
