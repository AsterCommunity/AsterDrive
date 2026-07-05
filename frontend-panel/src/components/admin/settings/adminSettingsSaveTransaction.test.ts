import { beforeEach, describe, expect, it, vi } from "vitest";
import { PREVIEW_APPS_CONFIG_KEY } from "@/components/admin/previewAppsConfigEditorShared";
import { executeAdminSettingsSaveTransaction } from "@/components/admin/settings/adminSettingsSaveTransaction";
import type { SystemConfig, SystemConfigVisibility } from "@/types/api";

const mockState = vi.hoisted(() => ({
	deleteConfig: vi.fn(),
	setConfig: vi.fn(),
}));

vi.mock("@/services/adminService", () => ({
	adminConfigService: {
		delete: (...args: unknown[]) => mockState.deleteConfig(...args),
		set: (...args: unknown[]) => mockState.setConfig(...args),
	},
}));

function createConfig(overrides: Partial<SystemConfig> = {}): SystemConfig {
	return {
		category: "site",
		description: "",
		id: 1,
		is_sensitive: false,
		key: "public_site_url",
		requires_restart: false,
		source: "system",
		updated_at: "2026-04-15T00:00:00Z",
		updated_by: null,
		value: ["https://old.example.com"],
		value_type: "string_array",
		visibility: "private",
		...overrides,
	};
}

describe("adminSettingsSaveTransaction", () => {
	beforeEach(() => {
		mockState.deleteConfig.mockReset();
		mockState.setConfig.mockReset();
		mockState.setConfig.mockImplementation(
			(
				key: string,
				value: string | string[],
				visibility?: SystemConfigVisibility,
			) =>
				Promise.resolve(
					createConfig({
						key,
						source: key.startsWith("custom.") ? "custom" : "system",
						value,
						visibility: visibility ?? "private",
					}),
				),
		);
	});

	it("executes delete update and create operations and reports invalidation targets", async () => {
		const existingPublicSiteUrl = createConfig();
		const existingPreviewApps = createConfig({
			key: PREVIEW_APPS_CONFIG_KEY,
			value: "{}",
			value_type: "multiline",
		});
		const existingCustom = createConfig({
			key: "custom.theme",
			source: "custom",
			value: "ocean",
		});
		const deletedCustom = createConfig({
			key: "custom.old",
			source: "custom",
			value: "stale",
		});

		const result = await executeAdminSettingsSaveTransaction({
			activeNewCustomRows: [
				{
					id: "row-1",
					key: "custom.banner",
					value: "visible",
					visibility: "public",
				},
			],
			changedExistingConfigs: [
				existingPublicSiteUrl,
				existingPreviewApps,
				existingCustom,
			],
			configs: [
				existingPublicSiteUrl,
				existingPreviewApps,
				existingCustom,
				deletedCustom,
			],
			deletedCustomConfigs: [deletedCustom],
			getCustomVisibilityDraft: (config) =>
				config.key === "custom.theme" ? "authenticated" : "private",
			getDraftValue: (config) => {
				if (config.key === "public_site_url") {
					return ["https://next.example.com"];
				}
				if (config.key === PREVIEW_APPS_CONFIG_KEY) {
					return '{"apps":[]}';
				}
				if (config.key === "custom.theme") {
					return "sunset";
				}
				return config.value as string;
			},
		});

		expect(mockState.deleteConfig).toHaveBeenCalledWith("custom.old");
		expect(mockState.setConfig).toHaveBeenCalledWith("public_site_url", [
			"https://next.example.com",
		]);
		expect(mockState.setConfig).toHaveBeenCalledWith(
			PREVIEW_APPS_CONFIG_KEY,
			'{"apps":[]}',
		);
		expect(mockState.setConfig).toHaveBeenCalledWith(
			"custom.theme",
			"sunset",
			"authenticated",
		);
		expect(mockState.setConfig).toHaveBeenCalledWith(
			"custom.banner",
			"visible",
			"public",
		);
		expect(result.nextPublicSiteUrl).toEqual(["https://next.example.com"]);
		expect(Array.from(result.invalidationTargets).sort()).toEqual([
			"frontend_config",
			"preview_apps",
		]);
		expect(result.nextConfigs.map((config) => config.key).sort()).toEqual([
			"custom.banner",
			"custom.theme",
			"frontend_preview_apps_json",
			"public_site_url",
		]);
	});

	it("fails when the backend returns a different key for a new custom row", async () => {
		mockState.setConfig.mockResolvedValueOnce(
			createConfig({
				key: "custom.other",
				source: "custom",
				value: "visible",
			}),
		);

		await expect(
			executeAdminSettingsSaveTransaction({
				activeNewCustomRows: [
					{
						id: "row-1",
						key: "custom.banner",
						value: "visible",
						visibility: "public",
					},
				],
				changedExistingConfigs: [],
				configs: [],
				deletedCustomConfigs: [],
				getCustomVisibilityDraft: () => "private",
				getDraftValue: () => "",
			}),
		).rejects.toThrow("Saved config key mismatch: expected custom.banner");
	});
});
