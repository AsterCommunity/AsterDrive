import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";

interface UploadTaskAction {
	label: string;
	icon: "X" | "ArrowsClockwise" | "Upload";
	onClick: () => void;
	variant?: "outline" | "ghost";
}

interface UploadTaskItemProps {
	title: string;
	status: string;
	mode: string;
	progress: number;
	detail?: string;
	speed?: string;
	completed?: boolean;
	actions?: UploadTaskAction[];
}

export function UploadTaskItem({
	title,
	status,
	mode,
	progress,
	detail,
	speed,
	completed = false,
	actions = [],
}: UploadTaskItemProps) {
	const failed =
		!completed && actions.some((action) => action.icon === "ArrowsClockwise");
	const waitingForFile =
		!completed && actions.some((action) => action.icon === "Upload");
	const showProgress =
		!completed && !failed && !waitingForFile && progress < 100;

	return (
		<div
			className={cn(
				"h-full w-full space-y-2 border-b border-border/65 px-4 py-2.5 transition-colors dark:border-border/50",
				completed
					? "bg-card/35 text-foreground/75 hover:bg-muted/30 dark:bg-card/20"
					: "bg-card/55 hover:bg-card/75 dark:bg-card/35 dark:hover:bg-card/50",
				failed &&
					"bg-destructive/5 hover:bg-destructive/10 dark:bg-destructive/10",
			)}
		>
			<div className="flex items-start gap-2">
				<div
					className={cn(
						"mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full",
						completed && "bg-emerald-500/10 text-emerald-600",
						failed && "bg-destructive/10 text-destructive",
						waitingForFile && "bg-primary/10 text-primary",
						showProgress && "bg-primary/10 text-primary",
					)}
				>
					<Icon
						name={
							completed
								? "Check"
								: failed
									? "CircleAlert"
									: waitingForFile
										? "Upload"
										: "Spinner"
						}
						className={cn("h-3.5 w-3.5", showProgress && "animate-spin")}
					/>
				</div>
				<div className="min-w-0 flex-1 space-y-0.5">
					<div className="truncate text-sm font-medium">{title}</div>
					<div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
						<span className="shrink-0">{mode}</span>
						<span className="text-muted-foreground/60">·</span>
						<span className="truncate">{detail ?? status}</span>
						{speed ? (
							<>
								<span className="shrink-0 text-muted-foreground/60">·</span>
								<span className="shrink-0 tabular-nums">{speed}</span>
							</>
						) : null}
					</div>
				</div>
				{showProgress ? (
					<span className="shrink-0 pt-0.5 text-xs text-muted-foreground tabular-nums">
						{progress}%
					</span>
				) : null}
				{actions.length > 0 && (
					<div className="flex items-center gap-1">
						{actions.map((action) => (
							<Button
								key={`${action.icon}-${action.label}`}
								variant={action.variant ?? "ghost"}
								size="icon-xs"
								onClick={action.onClick}
								title={action.label}
							>
								<Icon name={action.icon} className="h-3 w-3" />
							</Button>
						))}
					</div>
				)}
			</div>
			{showProgress ? <Progress value={progress} className="h-1.5" /> : null}
		</div>
	);
}
