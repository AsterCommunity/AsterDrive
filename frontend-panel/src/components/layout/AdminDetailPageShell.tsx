import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import { AdminPageHeader } from "./AdminPageHeader";

interface AdminDetailPageShellProps {
	actions?: ReactNode;
	backLabel: string;
	children: ReactNode;
	className?: string;
	contentClassName?: string;
	description?: string;
	onBack: () => void;
	title: string;
}

export function AdminDetailPageShell({
	actions,
	backLabel,
	children,
	className,
	contentClassName,
	description,
	onBack,
	title,
}: AdminDetailPageShellProps) {
	return (
		<div className={cn("flex min-h-0 min-w-0 flex-1 flex-col", className)}>
			<div className="mb-2 animate-in fade-in slide-in-from-top-1 duration-200 fill-mode-backwards motion-reduce:animate-none">
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className="-ml-2 text-muted-foreground"
					onClick={onBack}
				>
					<Icon name="ArrowLeft" className="mr-1 size-4" />
					{backLabel}
				</Button>
			</div>
			<AdminPageHeader
				className="animate-in fade-in slide-in-from-top-1 duration-200 fill-mode-backwards motion-reduce:animate-none px-0 md:px-0"
				title={title}
				description={description}
				actions={actions}
			/>
			<div className={cn("min-h-0 min-w-0", contentClassName)}>{children}</div>
		</div>
	);
}
