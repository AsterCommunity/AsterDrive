import { useTranslation } from "react-i18next";
import { NavLink } from "react-router-dom";
import { Icon } from "@/components/ui/icon";
import { SIDEBAR_SECTION_TITLE_CLASS, sidebarNavItemClass } from "@/lib/utils";
import type { Workspace } from "@/lib/workspace";
import { workspaceCategoryPath } from "@/lib/workspace";
import { QUICK_CATEGORY_LINKS } from "./sidebarLinks";

interface SidebarQuickCategoriesProps {
	onMobileClose: () => void;
	workspace: Workspace;
}

export function SidebarQuickCategories({
	onMobileClose,
	workspace,
}: SidebarQuickCategoriesProps) {
	const { t } = useTranslation();

	return (
		<div className="p-2 space-y-1">
			<p className={SIDEBAR_SECTION_TITLE_CLASS}>
				{t("search:quick_categories")}
			</p>
			{QUICK_CATEGORY_LINKS.map((link) => (
				<NavLink
					key={link.category}
					to={workspaceCategoryPath(workspace, link.category)}
					onClick={() => {
						onMobileClose();
					}}
					className={({ isActive }) =>
						sidebarNavItemClass(isActive, "w-full text-left", {
							indicator: true,
						})
					}
				>
					<Icon name={link.icon} className="size-4 shrink-0" />
					{t(`search:${link.labelKey}`)}
				</NavLink>
			))}
		</div>
	);
}
