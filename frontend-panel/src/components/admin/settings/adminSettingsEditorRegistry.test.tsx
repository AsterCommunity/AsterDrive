import { describe, expect, it } from "vitest";
import { MEDIA_PROCESSING_CONFIG_KEY } from "@/components/admin/mediaProcessingConfigEditorShared";
import { OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY } from "@/components/admin/offlineDownloadEngineRegistryShared";
import { PREVIEW_APPS_CONFIG_KEY } from "@/components/admin/previewAppsConfigEditorShared";
import {
	getAdminSettingsEditorDescriptor,
	hasAdminSettingsEditor,
} from "@/components/admin/settings/adminSettingsEditorRegistry";

describe("adminSettingsEditorRegistry", () => {
	it("discovers built-in special settings editors by key", () => {
		expect(hasAdminSettingsEditor(PREVIEW_APPS_CONFIG_KEY)).toBe(true);
		expect(hasAdminSettingsEditor(MEDIA_PROCESSING_CONFIG_KEY)).toBe(true);
		expect(hasAdminSettingsEditor(OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY)).toBe(
			true,
		);
	});

	it("returns null for ordinary settings", () => {
		expect(getAdminSettingsEditorDescriptor("public_site_url")).toBeNull();
		expect(hasAdminSettingsEditor("public_site_url")).toBe(false);
	});
});
