import { describe, expect, it } from "vitest";
import { MEDIA_PROCESSING_CONFIG_KEY } from "@/components/admin/mediaProcessingConfigEditorShared";
import { PREVIEW_APPS_CONFIG_KEY } from "@/components/admin/previewAppsConfigEditorShared";
import {
	type AdminSettingsInvalidationTarget,
	collectAdminSettingsInvalidationTargets,
} from "@/components/admin/settings/adminSettingsInvalidation";
import type { ConfigSchemaItem } from "@/types/api";

function targetsFor(keys: string[], schemas?: ConfigSchemaItem[]) {
	return Array.from(
		collectAdminSettingsInvalidationTargets(
			keys.map((key) => ({ key })),
			schemas,
		),
	).sort() as AdminSettingsInvalidationTarget[];
}

function schemaFor(
	key: string,
	invalidates?: AdminSettingsInvalidationTarget[],
): ConfigSchemaItem {
	return {
		category: "site",
		description: "",
		description_i18n_key: "",
		is_sensitive: false,
		key,
		label_i18n_key: "",
		requires_restart: false,
		value_type: "string",
		...(invalidates ? { invalidates } : {}),
	};
}

describe("adminSettingsInvalidation", () => {
	it("uses backend schema invalidation targets when available", () => {
		expect(
			targetsFor(
				["custom.plugin_config", "public_site_url"],
				[
					schemaFor("custom.plugin_config", [
						"preview_apps",
						"media_data_support",
					]),
					schemaFor("public_site_url", []),
				],
			),
		).toEqual(["media_data_support", "preview_apps"]);
	});

	it("falls back to built-in keys when schema metadata is unavailable", () => {
		expect(
			targetsFor(["public_site_url"], [schemaFor("public_site_url")]),
		).toEqual(["frontend_config"]);
	});

	it("invalidates frontend config for public branding and image preview preferences", () => {
		expect(
			targetsFor([
				"public_site_url",
				"auth_allow_user_registration",
				"frontend_image_preview_preference",
			]),
		).toEqual(["frontend_config"]);
	});

	it("invalidates preview app and support stores for preview/media config changes", () => {
		expect(
			targetsFor([PREVIEW_APPS_CONFIG_KEY, MEDIA_PROCESSING_CONFIG_KEY]),
		).toEqual(["media_data_support", "preview_apps", "thumbnail_support"]);
	});

	it("invalidates media data support for metadata setting changes", () => {
		expect(
			targetsFor(["media_metadata_enabled", "media_metadata_max_source_bytes"]),
		).toEqual(["media_data_support"]);
	});

	it("ignores unrelated settings", () => {
		expect(targetsFor(["custom.theme", "audit_retention_days"])).toEqual([]);
	});
});
