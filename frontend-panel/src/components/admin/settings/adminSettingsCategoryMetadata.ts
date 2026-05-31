import { useCallback, useMemo } from "react";
import type { AdminSettingsCategorySummary } from "@/components/admin/settings/AdminSettingsTabsLayout";
import { formatSubcategoryLabel } from "@/components/admin/settings/adminSettingsContentShared";
import type { IconName } from "@/components/ui/icon";

export const ADMIN_SETTINGS_CATEGORY_ORDER = [
	"site",
	"user",
	"auth",
	"mail",
	"network",
	"runtime",
	"storage",
	"file_processing",
	"webdav",
	"audit",
	"custom",
	"other",
] as const;

export type AdminSettingsTab = (typeof ADMIN_SETTINGS_CATEGORY_ORDER)[number];

export const ADMIN_SETTINGS_CATEGORY_INDEX: Record<string, number> =
	Object.fromEntries(
		ADMIN_SETTINGS_CATEGORY_ORDER.map((category, index) => [category, index]),
	);

export type AdminSettingsTranslationFn = (
	key: string,
	options?: Record<string, unknown>,
) => string;

export function getAdminSettingsCategoryIcon(category: string): IconName {
	switch (category) {
		case "user":
			return "User";
		case "auth":
			return "Shield";
		case "network":
			return "Globe";
		case "runtime":
			return "Gauge";
		case "mail":
			return "EnvelopeSimple";
		case "storage":
			return "HardDrive";
		case "file_processing":
			return "Cpu";
		case "webdav":
			return "FolderOpen";
		case "audit":
			return "Scroll";
		case "site":
			return "Gear";
		case "custom":
			return "BracketsCurly";
		default:
			return "Grid";
	}
}

export function getAdminSettingsSectionTitle(
	section: AdminSettingsTab,
	t: AdminSettingsTranslationFn,
) {
	switch (section) {
		case "user":
			return t("settings_category_user");
		case "auth":
			return t("settings_category_auth");
		case "network":
			return t("settings_category_network");
		case "runtime":
			return t("settings_category_runtime");
		case "mail":
			return t("settings_category_mail");
		case "storage":
			return t("settings_category_storage");
		case "file_processing":
			return t("settings_category_file_processing");
		case "webdav":
			return t("settings_category_webdav");
		case "audit":
			return t("settings_category_audit");
		case "custom":
			return t("settings_category_custom");
		case "other":
			return t("settings_category_other");
		default:
			return t("settings_category_site");
	}
}

function getAdminSettingsCategoryLabel(
	category: string,
	t: AdminSettingsTranslationFn,
) {
	switch (category) {
		case "user":
			return t("settings_category_user");
		case "auth":
			return t("settings_category_auth");
		case "network":
			return t("settings_category_network");
		case "runtime":
			return t("settings_category_runtime");
		case "mail":
			return t("settings_category_mail");
		case "storage":
			return t("settings_category_storage");
		case "file_processing":
			return t("settings_category_file_processing");
		case "webdav":
			return t("settings_category_webdav");
		case "audit":
			return t("settings_category_audit");
		case "site":
			return t("settings_category_site");
		case "custom":
			return t("settings_category_custom");
		case "other":
			return t("settings_category_other");
		default:
			return category;
	}
}

function getAdminSettingsCategoryDescription(
	category: string,
	t: AdminSettingsTranslationFn,
) {
	switch (category) {
		case "user":
			return t("settings_category_user_desc");
		case "auth":
			return t("settings_category_auth_desc");
		case "network":
			return t("settings_category_network_desc");
		case "runtime":
			return t("settings_category_runtime_desc");
		case "mail":
			return t("settings_category_mail_desc");
		case "storage":
			return t("settings_category_storage_desc");
		case "file_processing":
			return t("settings_category_file_processing_desc");
		case "webdav":
			return t("settings_category_webdav_desc");
		case "audit":
			return t("settings_category_audit_desc");
		case "site":
			return t("settings_category_site_desc");
		case "custom":
			return t("custom_config_intro");
		case "other":
			return t("settings_category_other_desc");
		default:
			return undefined;
	}
}

export function useAdminSettingsCategoryMetadata({
	systemGroups,
	t,
}: {
	systemGroups: Record<string, readonly unknown[]>;
	t: AdminSettingsTranslationFn;
}) {
	const systemCategories = useMemo(
		() =>
			Object.keys(systemGroups).sort((left, right) => {
				const leftIndex =
					ADMIN_SETTINGS_CATEGORY_INDEX[left] ?? Number.MAX_SAFE_INTEGER;
				const rightIndex =
					ADMIN_SETTINGS_CATEGORY_INDEX[right] ?? Number.MAX_SAFE_INTEGER;
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

	const getCategoryLabel = useCallback(
		(category: string) => getAdminSettingsCategoryLabel(category, t),
		[t],
	);

	const getCategoryDescription = useCallback(
		(category: string) => getAdminSettingsCategoryDescription(category, t),
		[t],
	);

	const categorySummaries = useMemo<AdminSettingsCategorySummary[]>(
		() =>
			tabCategories.map((category) => ({
				category,
				description: getCategoryDescription(category),
				icon: getAdminSettingsCategoryIcon(category),
				label: getCategoryLabel(category),
			})),
		[getCategoryDescription, getCategoryLabel, tabCategories],
	);

	return {
		categorySummaries,
		getCategoryDescription,
		getCategoryLabel,
		tabCategories,
	};
}

export function useAdminSettingsContentLabels({
	getCategoryLabel,
	t,
}: {
	getCategoryLabel: (category: string) => string;
	t: AdminSettingsTranslationFn;
}) {
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

	return {
		getMailTemplateGroupLabel,
		getSubcategoryDescription,
		getSubcategoryLabel,
	};
}
