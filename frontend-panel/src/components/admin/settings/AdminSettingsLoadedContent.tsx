import { AdminSettingsCategoryContent } from "@/components/admin/settings/AdminSettingsCategoryContent";
import type { AdminSettingsCategoryContentProps } from "@/components/admin/settings/AdminSettingsCategoryContentContext";
import {
	type AdminSettingsCategorySummary,
	AdminSettingsTabsLayout,
} from "@/components/admin/settings/AdminSettingsTabsLayout";
import { ADMIN_SETTINGS_PADDING_TRANSITION_CLASS } from "@/components/admin/settings/adminSettingsAnimation";
import type { AdminSettingsTranslationFn } from "@/components/admin/settings/adminSettingsCategoryMetadata";
import type { useAdminSettingsNavigation } from "@/components/admin/settings/useAdminSettingsNavigation";
import { TabsContent } from "@/components/ui/tabs";

export type AdminSettingsCategoryContentBaseProps = Omit<
	AdminSettingsCategoryContentProps,
	"category"
>;

type AdminSettingsNavigationState = ReturnType<
	typeof useAdminSettingsNavigation
>;

interface AdminSettingsCategoryTabContentProps {
	category: string;
	contentProps: AdminSettingsCategoryContentBaseProps;
}

function AdminSettingsCategoryTabContent({
	category,
	contentProps,
}: AdminSettingsCategoryTabContentProps) {
	return (
		<TabsContent value={category} className="min-w-0 pt-0">
			<AdminSettingsCategoryContent {...contentProps} category={category} />
		</TabsContent>
	);
}

interface AdminSettingsLoadedContentProps {
	categorySummaries: AdminSettingsCategorySummary[];
	contentBaseBottomPadding: number;
	contentProps: AdminSettingsCategoryContentBaseProps;
	navigation: AdminSettingsNavigationState;
	saveBarReservedHeight: number;
	t: AdminSettingsTranslationFn;
	tabCategories: string[];
}

export function AdminSettingsLoadedContent({
	categorySummaries,
	contentBaseBottomPadding,
	contentProps,
	navigation,
	saveBarReservedHeight,
	t,
	tabCategories,
}: AdminSettingsLoadedContentProps) {
	return (
		<div
			data-testid="settings-content"
			className={`flex flex-col gap-6 ${ADMIN_SETTINGS_PADDING_TRANSITION_CLASS}`}
			style={{
				paddingBottom: `${contentBaseBottomPadding + saveBarReservedHeight}px`,
			}}
		>
			<AdminSettingsTabsLayout
				activeCategorySummary={navigation.activeCategorySummary}
				activeTab={navigation.activeTab}
				categorySummaries={categorySummaries}
				compactInlineSummaries={navigation.compactInlineSummaries}
				compactNavContainerRef={navigation.compactNavContainerRef}
				compactOrderedSummaries={navigation.compactOrderedSummaries}
				compactOverflowActiveSummary={navigation.compactOverflowActiveSummary}
				compactOverflowDefaultMeasureRef={
					navigation.compactOverflowDefaultMeasureRef
				}
				compactOverflowMeasureRefs={navigation.compactOverflowMeasureRefs}
				compactOverflowSummaries={navigation.compactOverflowSummaries}
				compactTabMeasureRefs={navigation.compactTabMeasureRefs}
				handleCategoryChange={navigation.handleCategoryChange}
				isCompactNavigation={navigation.isCompactNavigation}
				isDesktopNavigation={navigation.isDesktopNavigation}
				isMobileNavigation={navigation.isMobileNavigation}
				t={t}
			>
				{tabCategories.map((category) => (
					<AdminSettingsCategoryTabContent
						key={category}
						category={category}
						contentProps={contentProps}
					/>
				))}
			</AdminSettingsTabsLayout>
		</div>
	);
}
