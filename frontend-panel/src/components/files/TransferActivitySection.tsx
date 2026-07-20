import type { ReactNode } from "react";
import { Icon, type IconName } from "@/components/ui/icon";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";

type TransferActivityTone = "default" | "active" | "success" | "error";

interface TransferActivitySectionProps {
	actions?: ReactNode;
	children: ReactNode;
	expandedBodyClassName: string;
	icon: IconName;
	open: boolean;
	progress?: number | null;
	summary: string;
	title: string;
	toggleLabel: string;
	tone?: TransferActivityTone;
	onToggle: () => void;
}

export function TransferActivitySection({
	actions,
	children,
	expandedBodyClassName,
	icon,
	open,
	progress = null,
	summary,
	title,
	toggleLabel,
	tone = "default",
	onToggle,
}: TransferActivitySectionProps) {
	return (
		<section className="pointer-events-auto w-full bg-card/95 dark:bg-card/85">
			<div
				className={cn(
					"bg-card/80 px-3 py-2.5 transition-colors dark:bg-card/65",
					open && "border-b border-border/60",
				)}
			>
				<div className="flex items-start gap-1">
					<button
						type="button"
						className="flex min-w-0 flex-1 items-start gap-3 rounded-md px-1 py-0.5 text-left outline-none transition-colors hover:bg-muted/35 focus-visible:ring-2 focus-visible:ring-ring/50"
						onClick={onToggle}
						aria-expanded={open}
						aria-label={toggleLabel}
						title={toggleLabel}
					>
						<span
							className={cn(
								"flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted/45 text-muted-foreground dark:bg-muted/25",
								tone === "active" &&
									"bg-blue-500/10 text-blue-600 dark:text-blue-300",
								tone === "success" &&
									"bg-emerald-500/10 text-emerald-600 dark:text-emerald-300",
								tone === "error" && "bg-destructive/10 text-destructive",
							)}
						>
							<Icon name={icon} className="size-4" />
						</span>
						<span className="min-w-0 flex-1">
							<span className="flex items-center justify-between gap-3">
								<span className="truncate text-sm font-semibold text-foreground">
									{title}
								</span>
								{progress !== null ? (
									<span className="shrink-0 text-xs font-medium text-muted-foreground tabular-nums">
										{progress}%
									</span>
								) : null}
							</span>
							<span className="mt-0.5 block truncate text-xs text-muted-foreground">
								{summary}
							</span>
						</span>
						<Icon
							name={open ? "CaretDown" : "CaretUp"}
							className="mt-2 size-3 shrink-0 text-muted-foreground"
						/>
					</button>
					{actions ? (
						<div className="flex shrink-0 items-center gap-1 pt-1">
							{actions}
						</div>
					) : null}
				</div>
				{progress !== null ? (
					<Progress
						value={progress}
						className={cn(
							"mt-2.5 h-1.5",
							tone === "success" &&
								"[&_[data-slot=progress-indicator]]:bg-emerald-500",
							tone === "error" &&
								"[&_[data-slot=progress-indicator]]:bg-destructive",
						)}
					/>
				) : null}
			</div>
			<div
				aria-hidden={!open}
				data-state={open ? "open" : "closed"}
				inert={open ? undefined : true}
				className={cn(
					"min-h-0 overflow-hidden transition-[height,opacity] duration-200 ease-out motion-reduce:transition-none",
					open ? `${expandedBodyClassName} opacity-100` : "h-0 opacity-0",
				)}
			>
				{children}
			</div>
		</section>
	);
}

interface TransferTaskItemProps {
	actions?: ReactNode;
	detail: ReactNode;
	error?: ReactNode;
	icon: IconName;
	progress?: number | null;
	progressLabel?: string;
	title: string;
	tone?: TransferActivityTone;
	warning?: ReactNode;
}

export function TransferTaskItem({
	actions,
	detail,
	error,
	icon,
	progress = null,
	progressLabel,
	title,
	tone = "default",
	warning,
}: TransferTaskItemProps) {
	return (
		<div
			className={cn(
				"h-full w-full space-y-2 border-b border-border/65 bg-card/45 px-4 py-2.5 transition-colors hover:bg-card/70 dark:border-border/50 dark:bg-card/25 dark:hover:bg-card/45",
				tone === "success" && "text-foreground/75 hover:bg-muted/30",
				tone === "error" &&
					"bg-destructive/5 hover:bg-destructive/10 dark:bg-destructive/10",
			)}
		>
			<div className="flex items-start gap-2">
				<span
					className={cn(
						"mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full bg-muted/55 text-muted-foreground",
						tone === "active" && "bg-primary/10 text-primary",
						tone === "success" &&
							"bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
						tone === "error" && "bg-destructive/10 text-destructive",
					)}
				>
					<Icon
						name={icon}
						className={cn("size-3.5", icon === "Spinner" && "animate-spin")}
					/>
				</span>
				<div className="min-w-0 flex-1 space-y-0.5">
					<div className="truncate text-sm font-medium text-foreground">
						{title}
					</div>
					<div className="truncate text-xs text-muted-foreground">{detail}</div>
				</div>
				{progressLabel ? (
					<span className="shrink-0 pt-0.5 text-xs text-muted-foreground tabular-nums">
						{progressLabel}
					</span>
				) : null}
				{actions ? (
					<div className="flex shrink-0 items-center gap-1">{actions}</div>
				) : null}
			</div>
			{progress !== null ? (
				<Progress value={progress} className="h-1.5" />
			) : null}
			{warning ? (
				<div className="break-words text-xs text-amber-700 dark:text-amber-300">
					{warning}
				</div>
			) : null}
			{error ? (
				<div className="break-words text-xs text-destructive">{error}</div>
			) : null}
		</div>
	);
}
