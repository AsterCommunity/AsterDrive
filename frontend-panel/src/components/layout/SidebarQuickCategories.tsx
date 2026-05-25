import { useTranslation } from "react-i18next";
import { Icon } from "@/components/ui/icon";
import { sidebarNavItemClass } from "@/lib/utils";
import type { FileCategory } from "@/types/api";
import { QUICK_CATEGORY_LINKS } from "./sidebarLinks";

interface SidebarQuickCategoriesProps {
	onMobileClose: () => void;
	onSearchCategoryOpen?: (category: FileCategory) => void;
}

export function SidebarQuickCategories({
	onMobileClose,
	onSearchCategoryOpen,
}: SidebarQuickCategoriesProps) {
	const { t } = useTranslation();

	return (
		<div className="p-2 space-y-1">
			<p className="px-3 py-1 text-xs font-medium text-muted-foreground">
				{t("search:quick_categories")}
			</p>
			{QUICK_CATEGORY_LINKS.map((link) => (
				<button
					key={link.category}
					type="button"
					onClick={() => {
						onSearchCategoryOpen?.(link.category);
						onMobileClose();
					}}
					className={sidebarNavItemClass(false, "w-full text-left")}
				>
					<Icon name={link.icon} className="size-4 shrink-0" />
					{t(`search:${link.labelKey}`)}
				</button>
			))}
		</div>
	);
}
