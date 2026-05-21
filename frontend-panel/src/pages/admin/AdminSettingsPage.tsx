import { useCallback, useEffect, useEffectEvent, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { AdminSettingsCategoryContent } from "@/components/admin/settings/AdminSettingsCategoryContent";
import {
	MailTemplateVariablesDialog,
	TestEmailDialog,
} from "@/components/admin/settings/AdminSettingsDialogs";
import { AdminSettingsSaveBar } from "@/components/admin/settings/AdminSettingsSaveBar";
import {
	type AdminSettingsCategorySummary,
	AdminSettingsTabsLayout,
} from "@/components/admin/settings/AdminSettingsTabsLayout";
import { formatSubcategoryLabel } from "@/components/admin/settings/adminSettingsContentShared";
import { useAdminSettingsData } from "@/components/admin/settings/useAdminSettingsData";
import { useAdminSettingsNavigation } from "@/components/admin/settings/useAdminSettingsNavigation";
import { useAdminSettingsSaveBar } from "@/components/admin/settings/useAdminSettingsSaveBar";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import type { IconName } from "@/components/ui/icon";
import { TabsContent } from "@/components/ui/tabs";
import { usePageTitle } from "@/hooks/usePageTitle";
import {
	ADMIN_SETTINGS_CONTENT_BASE_BOTTOM_PADDING_DESKTOP_PX,
	ADMIN_SETTINGS_CONTENT_BASE_BOTTOM_PADDING_MOBILE_PX,
	ADMIN_SETTINGS_SAVE_BAR_MIN_RESERVED_HEIGHT_DESKTOP_PX,
	ADMIN_SETTINGS_SAVE_BAR_MIN_RESERVED_HEIGHT_MOBILE_PX,
} from "@/lib/constants";
import { isImeComposingKeyEvent } from "@/lib/keyboard";
import { setPublicSiteUrls } from "@/lib/publicSiteUrl";
import { useAuthStore } from "@/stores/authStore";
import { useBrandingStore } from "@/stores/brandingStore";
import { useThemeStore } from "@/stores/themeStore";

const CATEGORY_ORDER = [
	"general",
	"user",
	"auth",
	"mail",
	"network",
	"operations",
	"storage",
	"webdav",
	"audit",
	"custom",
	"other",
] as const;

const CATEGORY_INDEX: Record<string, number> = Object.fromEntries(
	CATEGORY_ORDER.map((category, index) => [category, index]),
);

const MOBILE_BREAKPOINT = 768;
const DESKTOP_NAV_BREAKPOINT = 1280;
const COMPACT_NAV_TAB_GAP = 8;
const COMPACT_NAV_OVERFLOW_GAP = 12;
const SAVE_BAR_ENTER_DURATION_MS = 180;
const SAVE_BAR_EXIT_DURATION_MS = 140;

export type AdminSettingsTab = (typeof CATEGORY_ORDER)[number];

function syncPublicSiteUrlsRuntime(value: string[] | null | undefined) {
	const siteUrl = setPublicSiteUrls(value);
	useBrandingStore.setState({ siteUrl });
}

function getCategoryIcon(category: string): IconName {
	switch (category) {
		case "user":
			return "User";
		case "auth":
			return "Shield";
		case "network":
			return "Globe";
		case "operations":
			return "Clock";
		case "mail":
			return "EnvelopeSimple";
		case "storage":
			return "HardDrive";
		case "webdav":
			return "FolderOpen";
		case "audit":
			return "Scroll";
		case "general":
			return "Gear";
		case "custom":
			return "BracketsCurly";
		default:
			return "Grid";
	}
}

function getAdminSettingsSectionTitle(
	section: AdminSettingsTab,
	t: ReturnType<typeof useTranslation>["t"],
) {
	switch (section) {
		case "user":
			return t("settings_category_user");
		case "auth":
			return t("settings_category_auth");
		case "network":
			return t("settings_category_network");
		case "operations":
			return t("settings_category_operations");
		case "mail":
			return t("settings_category_mail");
		case "storage":
			return t("settings_category_storage");
		case "webdav":
			return t("settings_category_webdav");
		case "audit":
			return t("settings_category_audit");
		case "custom":
			return t("settings_category_custom");
		case "other":
			return t("settings_category_other");
		default:
			return t("settings_category_general");
	}
}

export default function AdminSettingsPage({
	section = CATEGORY_ORDER[0],
}: {
	section?: AdminSettingsTab;
}) {
	const { t } = useTranslation("admin");
	usePageTitle(getAdminSettingsSectionTitle(section, t));
	const navigate = useNavigate();
	const currentUserEmail = useAuthStore((state) => state.user?.email ?? "");
	const editorTheme = useThemeStore((state) =>
		state.resolvedTheme === "dark" ? "vs-dark" : "vs",
	);
	const {
		activeTemplateVariableGroup,
		activeTemplateVariableGroupCode,
		appendCustomDraftRow,
		changedCount,
		configValidationErrors,
		deletedCustomConfigs,
		displayUnits,
		discardChanges,
		expandedSubcategoryGroups,
		expandedTemplateGroups,
		getDraftValue,
		getSystemConfigDescription,
		getSystemConfigLabel,
		getTemplateVariableDescription,
		getTemplateVariableGroupLabel,
		getTemplateVariableLabel,
		handleBuildWopiDiscoveryPreviewConfig,
		handleTestFfmpegCliCommand,
		handleTestFfprobeCliCommand,
		handleSaveAll,
		handleSendTestEmail,
		handleTestVipsCliCommand,
		hasAnyConfig,
		hasUnsavedChanges,
		hasValidationError,
		loading,
		markCustomDeleted,
		newCustomRowErrors,
		newCustomRows,
		openTemplateVariablesDialog,
		openTestEmailDialog,
		removeNewCustomRow,
		restoreDeletedCustom,
		saving,
		setActiveTemplateVariableGroupCode,
		setDisplayUnits,
		setTestEmailDialogOpen,
		setTestEmailTarget,
		sendingTestEmail,
		systemGroups,
		systemSubcategoryGroups,
		testEmailDialogOpen,
		testEmailTarget,
		toggleSubcategoryGroup,
		toggleTemplateGroup,
		updateDraftValue,
		updateNewCustomRow,
		validationMessage,
		visibleCustomConfigs,
	} = useAdminSettingsData({
		currentUserEmail,
		onPublicSiteUrlChanged: syncPublicSiteUrlsRuntime,
		t,
	});

	const systemCategories = useMemo(
		() =>
			Object.keys(systemGroups).sort((left, right) => {
				const leftIndex = CATEGORY_INDEX[left] ?? Number.MAX_SAFE_INTEGER;
				const rightIndex = CATEGORY_INDEX[right] ?? Number.MAX_SAFE_INTEGER;
				return leftIndex - rightIndex || left.localeCompare(right);
			}),
		[systemGroups],
	);

	const tabCategories = useMemo(() => {
		const categories = [...systemCategories];
		if (!categories.includes("custom")) {
			categories.push("custom");
		}
		return categories;
	}, [systemCategories]);

	const resolvedSection = useMemo(() => {
		if (tabCategories.includes(section)) {
			return section;
		}

		return tabCategories[0] ?? section;
	}, [section, tabCategories]);

	const handleSaveShortcut = useEffectEvent((event: KeyboardEvent) => {
		const mod = event.metaKey || event.ctrlKey;
		if (
			!mod ||
			event.key.toLowerCase() !== "s" ||
			isImeComposingKeyEvent(event)
		) {
			return;
		}

		event.preventDefault();
		if (event.repeat) {
			return;
		}

		void handleSaveAll();
	});

	const getCategoryLabel = useCallback(
		(category: string) => {
			switch (category) {
				case "user":
					return t("settings_category_user");
				case "auth":
					return t("settings_category_auth");
				case "network":
					return t("settings_category_network");
				case "operations":
					return t("settings_category_operations");
				case "mail":
					return t("settings_category_mail");
				case "storage":
					return t("settings_category_storage");
				case "webdav":
					return t("settings_category_webdav");
				case "audit":
					return t("settings_category_audit");
				case "general":
					return t("settings_category_general");
				case "custom":
					return t("settings_category_custom");
				case "other":
					return t("settings_category_other");
				default:
					return category;
			}
		},
		[t],
	);

	const getCategoryDescription = useCallback(
		(category: string) => {
			switch (category) {
				case "user":
					return t("settings_category_user_desc");
				case "auth":
					return t("settings_category_auth_desc");
				case "network":
					return t("settings_category_network_desc");
				case "operations":
					return t("settings_category_operations_desc");
				case "mail":
					return t("settings_category_mail_desc");
				case "storage":
					return t("settings_category_storage_desc");
				case "webdav":
					return t("settings_category_webdav_desc");
				case "audit":
					return t("settings_category_audit_desc");
				case "general":
					return t("settings_category_general_desc");
				case "custom":
					return t("custom_config_intro");
				case "other":
					return t("settings_category_other_desc");
				default:
					return undefined;
			}
		},
		[t],
	);

	const categorySummaries = useMemo<AdminSettingsCategorySummary[]>(
		() =>
			tabCategories.map((category) => ({
				category,
				description: getCategoryDescription(category),
				icon: getCategoryIcon(category),
				label: getCategoryLabel(category),
			})),
		[getCategoryDescription, getCategoryLabel, tabCategories],
	);

	const {
		activeCategorySummary,
		activeTab,
		compactInlineSummaries,
		compactNavContainerRef,
		compactOrderedSummaries,
		compactOverflowActiveSummary,
		compactOverflowDefaultMeasureRef,
		compactOverflowMeasureRefs,
		compactOverflowSummaries,
		compactTabMeasureRefs,
		handleCategoryChange,
		isCompactNavigation,
		isDesktopNavigation,
		isMobileNavigation,
		tabDirection,
		viewportWidth,
	} = useAdminSettingsNavigation({
		categoryIndex: CATEGORY_INDEX,
		categorySummaries,
		compactNavOverflowGap: COMPACT_NAV_OVERFLOW_GAP,
		compactNavTabGap: COMPACT_NAV_TAB_GAP,
		desktopBreakpoint: DESKTOP_NAV_BREAKPOINT,
		hasAnyConfig,
		loading,
		mobileBreakpoint: MOBILE_BREAKPOINT,
		navigate,
		resolvedSection,
		section,
		tabCategories,
	});
	const settingsContentBaseBottomPadding =
		viewportWidth < MOBILE_BREAKPOINT
			? ADMIN_SETTINGS_CONTENT_BASE_BOTTOM_PADDING_MOBILE_PX
			: ADMIN_SETTINGS_CONTENT_BASE_BOTTOM_PADDING_DESKTOP_PX;
	const {
		measureRef: saveBarMeasureRef,
		phase: saveBarPhase,
		reservedHeight: saveBarReservedHeight,
	} = useAdminSettingsSaveBar({
		desktopMinReservedHeight:
			ADMIN_SETTINGS_SAVE_BAR_MIN_RESERVED_HEIGHT_DESKTOP_PX,
		enterDurationMs: SAVE_BAR_ENTER_DURATION_MS,
		exitDurationMs: SAVE_BAR_EXIT_DURATION_MS,
		hasUnsavedChanges,
		mobileBreakpoint: MOBILE_BREAKPOINT,
		mobileMinReservedHeight:
			ADMIN_SETTINGS_SAVE_BAR_MIN_RESERVED_HEIGHT_MOBILE_PX,
		viewportWidth,
	});

	useEffect(() => {
		document.addEventListener("keydown", handleSaveShortcut);
		return () => document.removeEventListener("keydown", handleSaveShortcut);
	}, []);

	const addCustomDraftRow = useCallback(() => {
		appendCustomDraftRow();
		handleCategoryChange("custom");
	}, [appendCustomDraftRow, handleCategoryChange]);

	const getSubcategoryLabel = useCallback(
		(category: string, subcategory?: string) => {
			if (!subcategory) {
				return getCategoryLabel(category);
			}

			const translationKey = `settings_subcategory_${category}_${subcategory.replaceAll(".", "_")}`;
			const translated = t(translationKey);
			return translated === translationKey
				? formatSubcategoryLabel(subcategory)
				: translated;
		},
		[getCategoryLabel, t],
	);

	const getSubcategoryDescription = useCallback(
		(category: string, subcategory?: string) => {
			if (!subcategory) {
				return undefined;
			}

			const translationKey = `settings_subcategory_${category}_${subcategory.replaceAll(".", "_")}_desc`;
			const translated = t(translationKey);
			return translated === translationKey ? undefined : translated;
		},
		[t],
	);

	const getMailTemplateGroupLabel = useCallback(
		(groupId: string) => {
			const translationKey = `settings_mail_template_group_${groupId}`;
			const translated = t(translationKey);
			return translated === translationKey
				? formatSubcategoryLabel(groupId)
				: translated;
		},
		[t],
	);

	const renderCategoryContent = (category: string) => {
		return (
			<AdminSettingsCategoryContent
				activeTab={activeTab}
				addCustomDraftRow={addCustomDraftRow}
				category={category}
				configValidationErrors={configValidationErrors}
				deletedCustomConfigs={deletedCustomConfigs}
				displayUnits={displayUnits}
				editorTheme={editorTheme}
				expandedSubcategoryGroups={expandedSubcategoryGroups}
				expandedTemplateGroups={expandedTemplateGroups}
				getCategoryDescription={getCategoryDescription}
				getCategoryLabel={getCategoryLabel}
				getDraftValue={getDraftValue}
				getMailTemplateGroupLabel={getMailTemplateGroupLabel}
				getSubcategoryDescription={getSubcategoryDescription}
				getSubcategoryLabel={getSubcategoryLabel}
				getSystemConfigDescription={getSystemConfigDescription}
				getSystemConfigLabel={getSystemConfigLabel}
				handleBuildWopiDiscoveryPreviewConfig={
					handleBuildWopiDiscoveryPreviewConfig
				}
				handleTestFfmpegCliCommand={handleTestFfmpegCliCommand}
				handleTestFfprobeCliCommand={handleTestFfprobeCliCommand}
				handleTestVipsCliCommand={handleTestVipsCliCommand}
				isMobileNavigation={isMobileNavigation}
				markCustomDeleted={markCustomDeleted}
				newCustomRowErrors={newCustomRowErrors}
				newCustomRows={newCustomRows}
				openTemplateVariablesDialog={openTemplateVariablesDialog}
				openTestEmailDialog={openTestEmailDialog}
				removeNewCustomRow={removeNewCustomRow}
				restoreDeletedCustom={restoreDeletedCustom}
				setDisplayUnits={setDisplayUnits}
				systemGroups={systemGroups}
				systemSubcategoryGroups={systemSubcategoryGroups}
				t={t}
				tabDirection={tabDirection}
				toggleSubcategoryGroup={toggleSubcategoryGroup}
				toggleTemplateGroup={toggleTemplateGroup}
				updateDraftValue={updateDraftValue}
				updateNewCustomRow={updateNewCustomRow}
				visibleCustomConfigs={visibleCustomConfigs}
			/>
		);
	};

	return (
		<AdminLayout>
			<AdminPageShell className="pb-0 md:pb-0">
				<AdminPageHeader
					title={t("system_settings")}
					description={t("settings_intro")}
				/>

				{loading ? (
					<SkeletonTable columns={4} rows={8} />
				) : !hasAnyConfig ? (
					<EmptyState title={t("no_config")} />
				) : (
					<div
						data-testid="settings-content"
						className="flex flex-col gap-8 transition-[padding-bottom] duration-200 ease-out"
						style={{
							paddingBottom: `${settingsContentBaseBottomPadding + saveBarReservedHeight}px`,
						}}
					>
						<AdminSettingsTabsLayout
							activeCategorySummary={activeCategorySummary}
							activeTab={activeTab}
							categorySummaries={categorySummaries}
							compactInlineSummaries={compactInlineSummaries}
							compactNavContainerRef={compactNavContainerRef}
							compactOrderedSummaries={compactOrderedSummaries}
							compactOverflowActiveSummary={compactOverflowActiveSummary}
							compactOverflowDefaultMeasureRef={
								compactOverflowDefaultMeasureRef
							}
							compactOverflowMeasureRefs={compactOverflowMeasureRefs}
							compactOverflowSummaries={compactOverflowSummaries}
							compactTabMeasureRefs={compactTabMeasureRefs}
							handleCategoryChange={handleCategoryChange}
							isCompactNavigation={isCompactNavigation}
							isDesktopNavigation={isDesktopNavigation}
							isMobileNavigation={isMobileNavigation}
							t={t}
						>
							{tabCategories.map((category) => (
								<TabsContent
									key={category}
									value={category}
									className="min-w-0 pt-0"
								>
									{renderCategoryContent(category)}
								</TabsContent>
							))}
						</AdminSettingsTabsLayout>
					</div>
				)}
			</AdminPageShell>
			<MailTemplateVariablesDialog
				activeGroup={activeTemplateVariableGroup}
				activeGroupCode={activeTemplateVariableGroupCode}
				getVariableGroupLabel={getTemplateVariableGroupLabel}
				getVariableLabel={getTemplateVariableLabel}
				getVariableDescription={getTemplateVariableDescription}
				onOpenChange={(open) => {
					if (!open) {
						setActiveTemplateVariableGroupCode(null);
					}
				}}
			/>
			<TestEmailDialog
				open={testEmailDialogOpen}
				sending={sendingTestEmail}
				target={testEmailTarget}
				onOpenChange={(open) => {
					if (!sendingTestEmail) {
						setTestEmailDialogOpen(open);
					}
				}}
				onTargetChange={setTestEmailTarget}
				onSend={() => void handleSendTestEmail()}
			/>
			<AdminSettingsSaveBar
				phase={saveBarPhase}
				measureRef={saveBarMeasureRef}
				hasUnsavedChanges={hasUnsavedChanges}
				hasValidationError={hasValidationError}
				changedCount={changedCount}
				saving={saving}
				validationMessage={validationMessage}
				onDiscardChanges={discardChanges}
				onSaveAll={() => void handleSaveAll()}
			/>
		</AdminLayout>
	);
}
