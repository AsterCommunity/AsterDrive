import { describe, expect, it } from "vitest";
import { MEDIA_PROCESSING_CONFIG_KEY } from "@/components/admin/mediaProcessingConfigEditorShared";
import { PREVIEW_APPS_CONFIG_KEY } from "@/components/admin/previewAppsConfigEditorShared";
import {
	type AdminSettingsInvalidationTarget,
	collectAdminSettingsInvalidationTargets,
} from "@/components/admin/settings/adminSettingsInvalidation";

function targetsFor(keys: string[]) {
	return Array.from(
		collectAdminSettingsInvalidationTargets(keys.map((key) => ({ key }))),
	).sort() as AdminSettingsInvalidationTarget[];
}

describe("adminSettingsInvalidation", () => {
	it("invalidates frontend config for public branding and image preview preferences", () => {
		expect(
			targetsFor(["public_site_url", "frontend_image_preview_preference"]),
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
