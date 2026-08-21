import { useTranslation } from "react-i18next";
import { HeaderControls } from "@/components/layout/HeaderControls";
import { TopBarBrand } from "@/components/layout/TopBarBrand";
import { TopBarShell } from "@/components/layout/TopBarShell";
import { ADMIN_TOPBAR_HEIGHT_CLASS } from "@/lib/constants";

interface AdminTopBarProps {
	onSidebarToggle: () => void;
	mobileOpen: boolean;
}

export function AdminTopBar({ onSidebarToggle, mobileOpen }: AdminTopBarProps) {
	const { t } = useTranslation(["core", "admin"]);

	return (
		<TopBarShell
			onSidebarToggle={onSidebarToggle}
			sidebarOpen={mobileOpen}
			sidebarToggleLabels={{
				open: t("open_admin_sidebar"),
				close: t("close_admin_sidebar"),
			}}
			left={
				<div className="flex min-w-0 items-center gap-3">
					<TopBarBrand to="/admin/overview" ariaLabel={t("admin:admin_home")} />
					<h1 className="truncate text-base font-semibold sm:text-lg">
						{t("admin_panel")}
					</h1>
				</div>
			}
			right={<HeaderControls showHomeButton />}
			heightClassName={ADMIN_TOPBAR_HEIGHT_CLASS}
		/>
	);
}
