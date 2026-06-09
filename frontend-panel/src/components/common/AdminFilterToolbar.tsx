import { type ReactNode, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { cn } from "@/lib/utils";
import { Button } from "../ui/button";
import { Icon } from "../ui/icon";
import { AnimatedCollapsible } from "./AnimatedCollapsible";

interface AdminFilterToolbarProps {
	activeFilterCount: number;
	children: ReactNode;
	className?: string;
	contentClassName?: string;
	/** Initial uncontrolled open state. Later prop changes do not reset the panel. */
	defaultOpen?: boolean;
	inline?: boolean;
	onResetFilters?: () => void;
}

export function AdminFilterToolbar({
	activeFilterCount,
	children,
	className,
	contentClassName,
	defaultOpen = false,
	inline = false,
	onResetFilters,
}: AdminFilterToolbarProps) {
	const { t } = useTranslation("admin");
	const panelId = useId();
	const [open, setOpen] = useState(defaultOpen);
	const hasActiveFilters = activeFilterCount > 0;

	return (
		<div className={cn(inline ? "contents" : "w-full space-y-2", className)}>
			<div className="flex flex-wrap items-center gap-2">
				<Button
					type="button"
					variant={open || hasActiveFilters ? "default" : "outline"}
					size="sm"
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					aria-controls={panelId}
					aria-expanded={open}
					onClick={() => setOpen((value) => !value)}
				>
					<Icon name="MagnifyingGlassPlus" className="mr-1 size-4" />
					{open ? t("hide_filters") : t("show_filters")}
					{hasActiveFilters ? (
						<span className="ml-1 rounded-full bg-background/90 px-1.5 py-0.5 text-[11px] font-medium text-foreground shadow-xs">
							{activeFilterCount}
						</span>
					) : null}
				</Button>
				{hasActiveFilters ? (
					<span className="text-xs text-muted-foreground">
						{t("filters_active")}
					</span>
				) : null}
				{hasActiveFilters && onResetFilters ? (
					<Button
						type="button"
						variant="ghost"
						size="sm"
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						onClick={onResetFilters}
					>
						{t("clear_filters")}
					</Button>
				) : null}
			</div>
			<AnimatedCollapsible
				open={open}
				className={inline ? "basis-full" : undefined}
				contentClassName="pt-1"
			>
				<div
					id={panelId}
					className={cn(
						"flex w-full flex-wrap items-center gap-2 rounded-lg border border-border/70 bg-muted/20 p-2.5",
						contentClassName,
					)}
				>
					{children}
				</div>
			</AnimatedCollapsible>
		</div>
	);
}
