import type { ComponentProps, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { SortMenu } from "@/components/common/SortMenu";
import { ToolbarBar } from "@/components/common/ToolbarBar";
import { ViewToggle } from "@/components/common/ViewToggle";
import { Icon, type IconName } from "@/components/ui/icon";
import { useMediaQuery } from "@/hooks/useMediaQuery";
import { FolderBreadcrumb } from "./FolderBreadcrumb";

interface FileBrowserNavigationBarProps {
	breadcrumb: ComponentProps<typeof FolderBreadcrumb>;
	icon: IconName;
	searchQuery?: string | null;
	onRefresh: () => void | Promise<void>;
	refreshDisabled?: boolean;
	viewMode: "grid" | "list";
	onViewModeChange: (mode: "grid" | "list") => void;
	sorting?: ComponentProps<typeof SortMenu>;
	actions?: ReactNode;
}

/** Shared file/trash navigation. Resource loading and mutation actions belong to callers. */
export function FileBrowserNavigationBar({
	breadcrumb,
	icon,
	searchQuery,
	onRefresh,
	refreshDisabled = false,
	viewMode,
	onViewModeChange,
	sorting,
	actions,
}: FileBrowserNavigationBarProps) {
	const { t } = useTranslation();
	const compact = useMediaQuery("(max-width: 639px)");
	return (
		<ToolbarBar
			left={
				<>
					<span className="flex size-7 shrink-0 items-center justify-center rounded-lg bg-accent/55 text-accent-foreground sm:h-8 sm:w-8">
						<Icon name={icon} className="size-4" />
					</span>
					<div className="min-w-0 flex-1">
						{searchQuery != null ? (
							<span className="block truncate text-xs text-muted-foreground sm:text-sm">
								{t("core:search")}: &quot;{searchQuery}&quot;
							</span>
						) : (
							<FolderBreadcrumb compact={compact} {...breadcrumb} />
						)}
					</div>
					<button
						type="button"
						className="flex size-7 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent/55 hover:text-accent-foreground sm:h-8 sm:w-8"
						disabled={refreshDisabled}
						onClick={() => void onRefresh()}
						aria-label={t("core:refresh")}
						title={t("core:refresh")}
					>
						<Icon name="ArrowsClockwise" className="size-4" />
					</button>
				</>
			}
			right={
				<>
					{actions}
					{sorting ? <SortMenu {...sorting} /> : null}
					<ViewToggle value={viewMode} onChange={onViewModeChange} />
				</>
			}
		/>
	);
}
