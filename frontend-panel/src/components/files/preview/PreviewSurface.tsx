import type { ReactNode } from "react";
import { Icon, type IconName } from "@/components/ui/icon";
import { cn } from "@/lib/utils";

interface PreviewSurfaceProps {
	children: ReactNode;
	className?: string;
}

interface PreviewSurfaceToolbarProps {
	icon?: IconName;
	label: ReactNode;
	meta?: ReactNode;
	actions?: ReactNode;
	className?: string;
}

interface PreviewSurfaceContentProps {
	children: ReactNode;
	className?: string;
}

interface PreviewSurfaceMessageProps {
	children: ReactNode;
	role?: "alert" | "status";
	tone?: "muted" | "danger";
	className?: string;
}

export function PreviewSurface({ children, className }: PreviewSurfaceProps) {
	return (
		<div
			className={cn(
				"flex h-full min-h-0 w-full min-w-0 flex-col overflow-hidden rounded-lg border border-border/70 bg-card shadow-xs dark:shadow-none",
				className,
			)}
		>
			{children}
		</div>
	);
}

export function PreviewSurfaceToolbar({
	icon,
	label,
	meta,
	actions,
	className,
}: PreviewSurfaceToolbarProps) {
	return (
		<div
			className={cn(
				"flex min-h-10 flex-wrap items-center gap-x-3 gap-y-2 border-b border-border/60 bg-muted/25 px-4 py-2 text-xs dark:bg-muted/15",
				className,
			)}
		>
			<div className="flex min-w-0 flex-1 items-center gap-2 text-muted-foreground">
				{icon ? (
					<Icon name={icon} className="size-4 shrink-0 text-muted-foreground" />
				) : null}
				<span className="truncate font-medium text-foreground">{label}</span>
				{meta ? (
					<>
						<span className="shrink-0 text-muted-foreground/60">·</span>
						<span className="min-w-0 truncate">{meta}</span>
					</>
				) : null}
			</div>
			{actions ? (
				<div className="ml-auto flex shrink-0 items-center gap-2">
					{actions}
				</div>
			) : null}
		</div>
	);
}

export function PreviewSurfaceContent({
	children,
	className,
}: PreviewSurfaceContentProps) {
	return (
		<div
			className={cn(
				"min-h-0 w-full min-w-0 flex-1 overflow-hidden bg-background/80 dark:bg-background/25",
				className,
			)}
		>
			{children}
		</div>
	);
}

export function PreviewSurfaceMessage({
	children,
	role,
	tone = "muted",
	className,
}: PreviewSurfaceMessageProps) {
	const effectiveRole = role ?? (tone === "danger" ? "alert" : undefined);

	return (
		<div
			role={effectiveRole}
			className={cn(
				"flex min-h-[12rem] items-center justify-center px-6 py-8 text-center text-sm",
				tone === "danger" ? "text-destructive" : "text-muted-foreground",
				className,
			)}
		>
			{children}
		</div>
	);
}
