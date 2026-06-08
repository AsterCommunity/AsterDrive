import type { ReactNode } from "react";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

interface ManagerDialogShellProps {
	children: ReactNode;
	controls?: ReactNode;
	description?: ReactNode;
	footer?: ReactNode;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onOpenChangeComplete?: (open: boolean) => void;
	title: ReactNode;
	className?: string;
}

export function ManagerDialogShell({
	children,
	controls,
	description,
	footer,
	open,
	onOpenChange,
	onOpenChangeComplete,
	title,
	className,
}: ManagerDialogShellProps) {
	return (
		<Dialog
			open={open}
			onOpenChange={onOpenChange}
			onOpenChangeComplete={onOpenChangeComplete}
		>
			<DialogContent
				className={cn(
					"flex h-[min(92dvh,44rem)] max-w-[calc(100%-1rem)] flex-col gap-0 overflow-hidden p-0 sm:h-auto sm:max-h-[min(88vh,44rem)] sm:max-w-xl",
					className,
				)}
			>
				<DialogHeader className="shrink-0 border-b px-5 py-4">
					<DialogTitle>{title}</DialogTitle>
					{description ? (
						<DialogDescription>{description}</DialogDescription>
					) : null}
				</DialogHeader>
				{controls ? (
					<div className="shrink-0 border-b border-border/60 px-5 py-4">
						{controls}
					</div>
				) : null}
				<div className="min-h-0 flex-1">{children}</div>
				{footer}
			</DialogContent>
		</Dialog>
	);
}

export function ManagerDialogScrollableList({
	children,
	className,
}: {
	children: ReactNode;
	className?: string;
}) {
	return (
		<div className={cn("h-full overflow-y-auto px-5 py-4", className)}>
			{children}
		</div>
	);
}

export function FixedDialogFooter({
	children,
	className,
}: {
	children: ReactNode;
	className?: string;
}) {
	return (
		<div
			data-theme-surface="panel"
			className={cn(
				"shrink-0 border-t border-border/60 bg-muted/35 px-5 py-3",
				className,
			)}
		>
			{children}
		</div>
	);
}

export function InlineConfirm({
	children,
	className,
}: {
	children: ReactNode;
	className?: string;
}) {
	return (
		<div
			className={cn(
				"rounded-lg border border-destructive/25 bg-destructive/5 p-3",
				className,
			)}
		>
			{children}
		</div>
	);
}
