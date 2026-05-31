import { useCallback, useMemo } from "react";
import type { AdminSettingsCategorySummary } from "@/components/admin/settings/AdminSettingsTabsLayout";
import { formatSubcategoryLabel } from "@/components/admin/settings/adminSettingsContentShared";
import type { IconName } from "@/components/ui/icon";

const CATEGORY_META = {
	site: {
		icon: "Gear",
		labelKey: "settings_category_site",
		descKey: "settings_category_site_desc",
	},
	user: {
		icon: "User",
		labelKey: "settings_category_user",
		descKey: "settings_category_user_desc",
	},
	auth: {
		icon: "Shield",
		labelKey: "settings_category_auth",
		descKey: "settings_category_auth_desc",
	},
	mail: {
		icon: "EnvelopeSimple",
		labelKey: "settings_category_mail",
		descKey: "settings_category_mail_desc",
	},
	network: {
		icon: "Globe",
		labelKey: "settings_category_network",
		descKey: "settings_category_network_desc",
	},
	runtime: {
		icon: "Gauge",
		labelKey: "settings_category_runtime",
		descKey: "settings_category_runtime_desc",
	},
	storage: {
		icon: "HardDrive",
		labelKey: "settings_category_storage",
		descKey: "settings_category_storage_desc",
	},
	file_processing: {
		icon: "Cpu",
		labelKey: "settings_category_file_processing",
		descKey: "settings_category_file_processing_desc",
	},
	webdav: {
		icon: "FolderOpen",
		labelKey: "settings_category_webdav",
		descKey: "settings_category_webdav_desc",
	},
	audit: {
		icon: "Scroll",
		labelKey: "settings_category_audit",
		descKey: "settings_category_audit_desc",
	},
	custom: {
		icon: "BracketsCurly",
		labelKey: "settings_category_custom",
		descKey: "custom_config_intro",
	},
	other: {
		icon: "Grid",
		labelKey: "settings_category_other",
		descKey: "settings_category_other_desc",
	},
} as const satisfies Record<
	string,
	{
		descKey?: string;
		icon: IconName;
		labelKey: string;
	}
>;

export const ADMIN_SETTINGS_CATEGORY_ORDER = Object.keys(
	CATEGORY_META,
) as (keyof typeof CATEGORY_META)[];

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
	return CATEGORY_META[category as keyof typeof CATEGORY_META]?.icon ?? "Grid";
}

export function getAdminSettingsSectionTitle(
	section: AdminSettingsTab,
	t: AdminSettingsTranslationFn,
) {
	const meta = CATEGORY_META[section as keyof typeof CATEGORY_META];
	return t(meta?.labelKey ?? CATEGORY_META.site.labelKey);
}

function getAdminSettingsCategoryLabel(
	category: string,
	t: AdminSettingsTranslationFn,
) {
	const meta = CATEGORY_META[category as keyof typeof CATEGORY_META];
	return meta ? t(meta.labelKey) : category;
}

function getAdminSettingsCategoryDescription(
	category: string,
	t: AdminSettingsTranslationFn,
) {
	const descKey =
		CATEGORY_META[category as keyof typeof CATEGORY_META]?.descKey;
	return descKey ? t(descKey) : undefined;
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
