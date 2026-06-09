import type { AdminSettingsCategoryContentProps } from "@/components/admin/settings/AdminSettingsCategoryContentContext";
import { AdminSettingsCategoryContentProvider } from "@/components/admin/settings/AdminSettingsCategoryContentContext";
import { AdminSettingsCustomCategorySection } from "@/components/admin/settings/AdminSettingsCustomCategorySection";
import { AdminSettingsSystemCategorySection } from "@/components/admin/settings/AdminSettingsSystemCategorySection";
import { ADMIN_SETTINGS_PANEL_ANIMATION_BY_DIRECTION } from "@/components/admin/settings/adminSettingsAnimation";

export function AdminSettingsCategoryContent(
	props: AdminSettingsCategoryContentProps,
) {
	const panelAnimationClass =
		ADMIN_SETTINGS_PANEL_ANIMATION_BY_DIRECTION[props.tabDirection];
	const showCategoryHeader = !props.isMobileNavigation;

	return (
		<AdminSettingsCategoryContentProvider value={props}>
			{props.category === "custom" ? (
				<AdminSettingsCustomCategorySection
					category={props.category}
					panelAnimationClass={panelAnimationClass}
					showCategoryHeader={showCategoryHeader}
				/>
			) : (
				<AdminSettingsSystemCategorySection
					category={props.category}
					panelAnimationClass={panelAnimationClass}
					showCategoryHeader={showCategoryHeader}
				/>
			)}
		</AdminSettingsCategoryContentProvider>
	);
}
