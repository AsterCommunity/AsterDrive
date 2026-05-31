import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
	ADMIN_SETTINGS_CATEGORY_INDEX,
	ADMIN_SETTINGS_CATEGORY_ORDER,
	type AdminSettingsTab,
	getAdminSettingsCategoryIcon,
	getAdminSettingsSectionTitle,
	useAdminSettingsCategoryMetadata,
	useAdminSettingsContentLabels,
} from "@/components/admin/settings/adminSettingsCategoryMetadata";

function t(key: string) {
	const translations: Record<string, string> = {
		custom_config_intro: "Custom config intro",
		settings_category_audit: "Audit",
		settings_category_audit_desc: "Audit settings",
		settings_category_auth: "Authentication",
		settings_category_auth_desc: "Authentication settings",
		settings_category_custom: "Custom",
		settings_category_file_processing: "File processing",
		settings_category_file_processing_desc: "File processing settings",
		settings_category_mail: "Mail",
		settings_category_mail_desc: "Mail settings",
		settings_category_network: "Network",
		settings_category_network_desc: "Network settings",
		settings_category_other: "Other",
		settings_category_other_desc: "Other settings",
		settings_category_runtime: "Runtime",
		settings_category_runtime_desc: "Runtime settings",
		settings_category_site: "Site",
		settings_category_site_desc: "Site settings",
		settings_category_storage: "Storage",
		settings_category_storage_desc: "Storage settings",
		settings_category_user: "User",
		settings_category_user_desc: "User settings",
		settings_category_webdav: "WebDAV",
		settings_category_webdav_desc: "WebDAV settings",
		settings_mail_template_group_login_email_code: "Login email code",
		settings_subcategory_mail_config: "Mail config",
		settings_subcategory_mail_config_desc: "Mail delivery settings",
	};

	return translations[key] ?? key;
}

describe("adminSettingsCategoryMetadata", () => {
	it("keeps the category order, indexes, section titles, and icons in sync", () => {
		expect(ADMIN_SETTINGS_CATEGORY_ORDER).toEqual([
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
		]);
		expect(ADMIN_SETTINGS_CATEGORY_INDEX.site).toBe(0);
		expect(ADMIN_SETTINGS_CATEGORY_INDEX.other).toBe(11);

		expect(
			ADMIN_SETTINGS_CATEGORY_ORDER.map((category) =>
				getAdminSettingsSectionTitle(category, t),
			),
		).toEqual([
			"Site",
			"User",
			"Authentication",
			"Mail",
			"Network",
			"Runtime",
			"Storage",
			"File processing",
			"WebDAV",
			"Audit",
			"Custom",
			"Other",
		]);
		expect(
			ADMIN_SETTINGS_CATEGORY_ORDER.map((category) =>
				getAdminSettingsCategoryIcon(category),
			),
		).toEqual([
			"Gear",
			"User",
			"Shield",
			"EnvelopeSimple",
			"Globe",
			"Gauge",
			"HardDrive",
			"Cpu",
			"FolderOpen",
			"Scroll",
			"BracketsCurly",
			"Grid",
		]);

		expect(getAdminSettingsSectionTitle("unknown" as AdminSettingsTab, t)).toBe(
			"Site",
		);
		expect(getAdminSettingsCategoryIcon("unknown")).toBe("Grid");
	});

	it("sorts configured system categories and appends custom when absent", () => {
		const { result } = renderHook(() =>
			useAdminSettingsCategoryMetadata({
				systemGroups: {
					storage: [{}],
					auth: [{}],
					unknown_bucket: [{}],
					site: [{}],
				},
				t,
			}),
		);

		expect(result.current.tabCategories).toEqual([
			"site",
			"auth",
			"storage",
			"unknown_bucket",
			"custom",
		]);
		expect(result.current.categorySummaries).toEqual([
			{
				category: "site",
				description: "Site settings",
				icon: "Gear",
				label: "Site",
			},
			{
				category: "auth",
				description: "Authentication settings",
				icon: "Shield",
				label: "Authentication",
			},
			{
				category: "storage",
				description: "Storage settings",
				icon: "HardDrive",
				label: "Storage",
			},
			{
				category: "unknown_bucket",
				description: undefined,
				icon: "Grid",
				label: "unknown_bucket",
			},
			{
				category: "custom",
				description: "Custom config intro",
				icon: "BracketsCurly",
				label: "Custom",
			},
		]);
		expect(result.current.getCategoryLabel("mail")).toBe("Mail");
		expect(result.current.getCategoryDescription("mail")).toBe("Mail settings");
		expect(result.current.getCategoryDescription("unknown_bucket")).toBe(
			undefined,
		);
	});

	it("does not append a duplicate custom category", () => {
		const { result } = renderHook(() =>
			useAdminSettingsCategoryMetadata({
				systemGroups: {
					custom: [{}],
					network: [{}],
				},
				t,
			}),
		);

		expect(result.current.tabCategories).toEqual(["network", "custom"]);
	});

	it("resolves subcategory and mail template labels with fallbacks", () => {
		const { result } = renderHook(() =>
			useAdminSettingsContentLabels({
				getCategoryLabel: (category) => `category:${category}`,
				t,
			}),
		);

		expect(result.current.getSubcategoryLabel("mail")).toBe("category:mail");
		expect(result.current.getSubcategoryLabel("mail", "config")).toBe(
			"Mail config",
		);
		expect(
			result.current.getSubcategoryLabel("runtime", "background_task"),
		).toBe("Background Task");
		expect(result.current.getSubcategoryDescription("mail")).toBe(undefined);
		expect(result.current.getSubcategoryDescription("mail", "config")).toBe(
			"Mail delivery settings",
		);
		expect(
			result.current.getSubcategoryDescription("runtime", "background_task"),
		).toBe(undefined);
		expect(result.current.getMailTemplateGroupLabel("login_email_code")).toBe(
			"Login email code",
		);
		expect(result.current.getMailTemplateGroupLabel("password_reset")).toBe(
			"Password Reset",
		);
	});
});
