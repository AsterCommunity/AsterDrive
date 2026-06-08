import { useTranslation } from "react-i18next";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { HeaderControls } from "@/components/layout/HeaderControls";
import { TopBarShell } from "@/components/layout/TopBarShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";

interface TopBarProps {
	onSidebarToggle: () => void;
	mobileOpen: boolean;
	onSearchOpen: () => void;
}

export function TopBar({
	onSidebarToggle,
	mobileOpen,
	onSearchOpen,
}: TopBarProps) {
	const { t } = useTranslation(["core", "search"]);

	const searchButton = (
		<Button
			type="button"
			variant="outline"
			size="sm"
			onClick={onSearchOpen}
			aria-label={t("search:open_search")}
			className="h-9 w-full justify-between rounded-full border-border/50 bg-muted/35 px-3 text-muted-foreground shadow-none hover:bg-muted/55 hover:text-foreground"
		>
			<span className="flex min-w-0 items-center gap-2">
				<Icon name="MagnifyingGlass" className="size-4 shrink-0" />
				<span className="truncate text-sm">{t("search:placeholder")}</span>
			</span>
			<span className="hidden items-center gap-1 rounded-full border border-border/60 bg-background/80 px-2 py-0.5 text-[11px] font-medium text-muted-foreground md:inline-flex">
				<kbd className="font-sans">Ctrl</kbd>
				<span>K</span>
			</span>
		</Button>
	);

	return (
		<TopBarShell
			onSidebarToggle={onSidebarToggle}
			sidebarOpen={mobileOpen}
			sidebarToggleLabels={{
				open: t("open_sidebar"),
				close: t("close_sidebar"),
			}}
			left={
				<div className="flex min-w-0 items-center gap-2 sm:gap-3">
					<AsterDriveWordmark
						alt={t("app_name")}
						className="hidden h-16 w-auto shrink-0 px-6 md:block"
					/>
				</div>
			}
			center={
				<div className="flex w-full max-w-xl items-center">{searchButton}</div>
			}
			right={
				<HeaderControls
					mobileSearchAction={
						<Button
							type="button"
							variant="ghost"
							size="icon-sm"
							onClick={onSearchOpen}
							aria-label={t("search:open_search")}
							className="sm:hidden"
						>
							<Icon name="MagnifyingGlass" className="size-4" />
						</Button>
					}
					showAdminEntry
				/>
			}
		/>
	);
}
