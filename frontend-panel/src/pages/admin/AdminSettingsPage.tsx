import { useCallback, useEffect, useEffectEvent, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import {
	MailTemplateVariablesDialog,
	TestEmailDialog,
} from "@/components/admin/settings/AdminSettingsDialogs";
import {
	type AdminSettingsCategoryContentBaseProps,
	AdminSettingsLoadedContent,
} from "@/components/admin/settings/AdminSettingsLoadedContent";
import { AdminSettingsSaveBar } from "@/components/admin/settings/AdminSettingsSaveBar";
import {
	ADMIN_SETTINGS_CATEGORY_INDEX,
	ADMIN_SETTINGS_CATEGORY_ORDER,
	type AdminSettingsTab,
	getAdminSettingsSectionTitle,
	useAdminSettingsCategoryMetadata,
	useAdminSettingsContentLabels,
} from "@/components/admin/settings/adminSettingsCategoryMetadata";
import { useAdminSettingsData } from "@/components/admin/settings/useAdminSettingsData";
import { useAdminSettingsNavigation } from "@/components/admin/settings/useAdminSettingsNavigation";
import { useAdminSettingsSaveBar } from "@/components/admin/settings/useAdminSettingsSaveBar";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
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

const MOBILE_BREAKPOINT = 768;
const DESKTOP_NAV_BREAKPOINT = 1280;
const COMPACT_NAV_TAB_GAP = 8;
const COMPACT_NAV_OVERFLOW_GAP = 12;
const SAVE_BAR_ENTER_DURATION_MS = 180;
const SAVE_BAR_EXIT_DURATION_MS = 140;

function syncPublicSiteUrlsRuntime(value: string[] | null | undefined) {
	const siteUrl = setPublicSiteUrls(value);
	useBrandingStore.setState({ siteUrl });
}

export default function AdminSettingsPage({
	section = ADMIN_SETTINGS_CATEGORY_ORDER[0],
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
		getDraftValueByKey,
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

	const {
		categorySummaries,
		getCategoryDescription,
		getCategoryLabel,
		tabCategories,
	} = useAdminSettingsCategoryMetadata({ systemGroups, t });

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

	const navigation = useAdminSettingsNavigation({
		categoryIndex: ADMIN_SETTINGS_CATEGORY_INDEX,
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
	const { activeTab, handleCategoryChange, isMobileNavigation, tabDirection } =
		navigation;
	const { viewportWidth } = navigation;
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

	const {
		getMailTemplateGroupLabel,
		getSubcategoryDescription,
		getSubcategoryLabel,
	} = useAdminSettingsContentLabels({ getCategoryLabel, t });

	const navigateToMailSettings = useCallback(
		() => navigate("/admin/settings/mail"),
		[navigate],
	);

	const categoryContentProps: AdminSettingsCategoryContentBaseProps = {
		activeTab,
		addCustomDraftRow,
		configValidationErrors,
		deletedCustomConfigs,
		displayUnits,
		editorTheme,
		expandedSubcategoryGroups,
		expandedTemplateGroups,
		getCategoryDescription,
		getCategoryLabel,
		getDraftValue,
		getDraftValueByKey,
		getMailTemplateGroupLabel,
		getSubcategoryDescription,
		getSubcategoryLabel,
		getSystemConfigDescription,
		getSystemConfigLabel,
		handleBuildWopiDiscoveryPreviewConfig,
		handleTestFfmpegCliCommand,
		handleTestFfprobeCliCommand,
		handleTestVipsCliCommand,
		isMobileNavigation,
		markCustomDeleted,
		navigateToMailSettings,
		newCustomRowErrors,
		newCustomRows,
		openTemplateVariablesDialog,
		openTestEmailDialog,
		removeNewCustomRow,
		restoreDeletedCustom,
		setDisplayUnits,
		systemGroups,
		systemSubcategoryGroups,
		t,
		tabDirection,
		toggleSubcategoryGroup,
		toggleTemplateGroup,
		updateDraftValue,
		updateNewCustomRow,
		visibleCustomConfigs,
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
					<AdminSettingsLoadedContent
						categorySummaries={categorySummaries}
						contentBaseBottomPadding={settingsContentBaseBottomPadding}
						contentProps={categoryContentProps}
						navigation={navigation}
						saveBarReservedHeight={saveBarReservedHeight}
						t={t}
						tabCategories={tabCategories}
					/>
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
