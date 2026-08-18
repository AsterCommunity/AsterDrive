import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { USER_TOPBAR_HEIGHT_CLASS } from "@/lib/constants";

interface TopBarShellProps {
	onSidebarToggle?: () => void;
	sidebarOpen?: boolean;
	sidebarToggleLabels?: {
		open: string;
		close: string;
	};
	left: ReactNode;
	center?: ReactNode;
	right: ReactNode;
	heightClassName?: string;
}

export function TopBarShell({
	onSidebarToggle,
	sidebarOpen = false,
	sidebarToggleLabels,
	left,
	center,
	right,
	heightClassName = USER_TOPBAR_HEIGHT_CLASS,
}: TopBarShellProps) {
	const sidebarToggleLabel = sidebarOpen
		? (sidebarToggleLabels?.close ?? "Close sidebar")
		: (sidebarToggleLabels?.open ?? "Open sidebar");

	return (
		<div
			data-theme-surface="chrome"
			className="relative z-20 bg-muted/70 shadow-xs backdrop-blur supports-[backdrop-filter]:bg-muted/55 dark:bg-card/82 dark:shadow-none dark:supports-[backdrop-filter]:bg-card/72"
		>
			<div
				className={`flex items-center gap-3 px-4 shrink-0 ${heightClassName}`}
			>
				{onSidebarToggle && (
					<Button
						variant="ghost"
						size="icon"
						className="relative size-8 shrink-0 overflow-hidden md:hidden"
						onClick={onSidebarToggle}
						aria-label={sidebarToggleLabel}
						aria-expanded={sidebarOpen}
					>
						<span className="relative inline-flex size-4 items-center justify-center">
							<span
								className={`absolute inset-0 flex items-center justify-center transition-all duration-200 ease-out motion-reduce:transition-none ${
									sidebarOpen
										? "-rotate-90 scale-75 opacity-0"
										: "rotate-0 scale-100 opacity-100"
								}`}
							>
								<Icon name="List" className="size-4" />
							</span>
							<span
								className={`absolute inset-0 flex items-center justify-center transition-all duration-200 ease-out motion-reduce:transition-none ${
									sidebarOpen
										? "rotate-0 scale-100 opacity-100"
										: "rotate-90 scale-75 opacity-0"
								}`}
							>
								<Icon name="X" className="size-4" />
							</span>
						</span>
					</Button>
				)}
				<div className="min-w-0 shrink-0">{left}</div>
				{center ? (
					<div className="hidden min-w-0 flex-1 sm:flex">{center}</div>
				) : (
					<div className="flex-1" />
				)}
				<div className="flex-1" />
				{right}
			</div>
		</div>
	);
}
