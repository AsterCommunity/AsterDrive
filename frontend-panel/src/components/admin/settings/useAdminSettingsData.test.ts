import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	MEDIA_PROCESSING_CONFIG_KEY,
	MEDIA_PROCESSING_CONFIG_VERSION,
	type MediaProcessingEditorConfig,
} from "@/components/admin/mediaProcessingConfigEditorShared";
import {
	PREVIEW_APP_PROTECTED_BUILTIN_KEYS,
	PREVIEW_APPS_CONFIG_KEY,
} from "@/components/admin/previewAppsConfigEditorShared";
import { useAdminSettingsData } from "@/components/admin/settings/useAdminSettingsData";
import type {
	SystemConfig,
	SystemConfigSource,
	SystemConfigValueType,
} from "@/types/api";

const mockState = vi.hoisted(() => ({
	actionConfig: vi.fn(),
	brandingInvalidate: vi.fn(),
	brandingLoad: vi.fn(),
	deleteConfig: vi.fn(),
	handleApiError: vi.fn(),
	mediaDataSupportInvalidate: vi.fn(),
	mediaDataSupportLoad: vi.fn(),
	listConfigs: vi.fn(),
	previewInvalidate: vi.fn(),
	previewLoad: vi.fn(),
	schema: vi.fn(),
	sendTestEmail: vi.fn(),
	setConfig: vi.fn(),
	templateVariables: vi.fn(),
	thumbnailSupportInvalidate: vi.fn(),
	thumbnailSupportLoad: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/adminConfigMetadataCache", async () => {
	const actual = await vi.importActual<
		typeof import("@/lib/adminConfigMetadataCache")
	>("@/lib/adminConfigMetadataCache");
	return actual;
});

vi.mock("@/services/adminService", () => ({
	adminConfigService: {
		action: (...args: unknown[]) => mockState.actionConfig(...args),
		delete: (...args: unknown[]) => mockState.deleteConfig(...args),
		list: (...args: unknown[]) => mockState.listConfigs(...args),
		sendTestEmail: (...args: unknown[]) => mockState.sendTestEmail(...args),
		schema: (...args: unknown[]) => mockState.schema(...args),
		set: (...args: unknown[]) => mockState.setConfig(...args),
		templateVariables: (...args: unknown[]) =>
			mockState.templateVariables(...args),
	},
}));

vi.mock("@/stores/mediaDataSupportStore", () => {
	const useMediaDataSupportStore = Object.assign(vi.fn(), {
		getState: () => ({
			invalidate: mockState.mediaDataSupportInvalidate,
			load: mockState.mediaDataSupportLoad,
		}),
	});

	return { useMediaDataSupportStore };
});

vi.mock("@/stores/previewAppStore", () => {
	const usePreviewAppStore = Object.assign(vi.fn(), {
		getState: () => ({
			invalidate: mockState.previewInvalidate,
			load: mockState.previewLoad,
		}),
	});

	return { usePreviewAppStore };
});

vi.mock("@/stores/brandingStore", () => {
	const useBrandingStore = Object.assign(vi.fn(), {
		getState: () => ({
			invalidate: mockState.brandingInvalidate,
			load: mockState.brandingLoad,
		}),
	});

	return { useBrandingStore };
});

vi.mock("@/stores/thumbnailSupportStore", () => {
	const useThumbnailSupportStore = Object.assign(vi.fn(), {
		getState: () => ({
			invalidate: mockState.thumbnailSupportInvalidate,
			load: mockState.thumbnailSupportLoad,
		}),
	});

	return { useThumbnailSupportStore };
});

const translationMap: Record<string, string> = {
	cors_wildcard_credentials_validation_error:
		"cors_wildcard_credentials_validation_error",
	custom_config_key_duplicate: "custom_config_key_duplicate",
	custom_config_key_required: "custom_config_key_required",
	settings_saved: "settings_saved",
	settings_item_media_processing_registry_json_label:
		"Media processing registry",
	settings_item_media_processing_registry_json_desc:
		"Choose which media processors are enabled and bind extensions as needed. Built-in AsterDrive processors act as the final fallback when enabled.",
};

function t(key: string) {
	return translationMap[key] ?? key;
}

function createConfig(overrides: Partial<SystemConfig> = {}): SystemConfig {
	return {
		category: "general",
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
		...overrides,
	};
}

function createValidPreviewAppsConfig(
	extraApps: Record<string, unknown>[] = [],
) {
	return JSON.stringify(
		{
			version: 2,
			apps: [
				...PREVIEW_APP_PROTECTED_BUILTIN_KEYS.map((key) => ({
					enabled: true,
					key,
					labels: {
						en: key,
					},
					provider: "builtin",
				})),
				...extraApps,
			],
		},
		null,
		2,
	);
}

function createValidMediaProcessingConfig(
	overrides: Partial<
		Pick<MediaProcessingEditorConfig, "processors" | "version">
	> = {},
) {
	const config: MediaProcessingEditorConfig = {
		version: overrides.version ?? MEDIA_PROCESSING_CONFIG_VERSION,
		processors: overrides.processors ?? [
			{
				config: {
					command: "vips",
				},
				enabled: false,
				extensions: ["heic", "heif"],
				kind: "vips_cli",
				uses: ["thumbnail:image"],
			},
			{
				config: {
					command: "ffmpeg",
				},
				enabled: false,
				extensions: ["mp4"],
				kind: "ffmpeg_cli",
				uses: ["thumbnail:video"],
			},
			{
				config: {
					command: "ffprobe",
				},
				enabled: false,
				extensions: ["mp4"],
				kind: "ffprobe_cli",
				uses: ["metadata:video"],
			},
			{
				config: {
					command: "",
				},
				enabled: true,
				extensions: ["mp3", "flac"],
				kind: "lofty",
				uses: ["thumbnail:audio", "metadata:audio"],
			},
			{
				config: {
					command: "",
				},
				enabled: true,
				extensions: [],
				kind: "images",
				uses: ["thumbnail:image", "metadata:image"],
			},
		],
	};

	return JSON.stringify(config, null, 2);
}

function createBaseConfigs() {
	return [
		createConfig(),
		createConfig({
			category: "custom",
			key: "custom.theme",
			source: "custom",
			value: "ocean",
		}),
		createConfig({
			category: "general.preview",
			key: PREVIEW_APPS_CONFIG_KEY,
			value: createValidPreviewAppsConfig(),
			value_type: "multiline",
		}),
		createConfig({
			category: "storage.media_processing",
			key: MEDIA_PROCESSING_CONFIG_KEY,
			value: createValidMediaProcessingConfig(),
			value_type: "multiline",
		}),
		createConfig({
			category: "storage.media_processing",
			key: "media_metadata_enabled",
			value: "true",
			value_type: "boolean",
		}),
		createConfig({
			category: "storage.media_processing",
			key: "media_metadata_max_source_bytes",
			value: "1073741824",
			value_type: "number",
		}),
	];
}

function createCorsConfigs(overrides?: {
	allowCredentials?: string;
	allowedOrigins?: string;
}): SystemConfig[] {
	return [
		createConfig({
			category: "network",
			key: "cors_allowed_origins",
			value: overrides?.allowedOrigins ?? "*",
		}),
		createConfig({
			category: "network",
			key: "cors_allow_credentials",
			value: overrides?.allowCredentials ?? "false",
			value_type: "boolean",
		}),
	];
}

function getConfigCategory(key: string) {
	if (key.startsWith("custom")) return "custom";
	if (key === PREVIEW_APPS_CONFIG_KEY) return "general.preview";
	if (key === MEDIA_PROCESSING_CONFIG_KEY) return "storage.media_processing";
	if (key.startsWith("media_metadata_")) return "storage.media_processing";
	return "general";
}

function getMockConfigSource(key: string): SystemConfigSource {
	return key.startsWith("custom") ? "custom" : "system";
}

function getMockConfigValueType(key: string): SystemConfigValueType {
	if (key === "public_site_url") return "string_array";
	if (key === PREVIEW_APPS_CONFIG_KEY || key === MEDIA_PROCESSING_CONFIG_KEY)
		return "multiline";
	if (key === "media_metadata_enabled") return "boolean";
	if (key === "media_metadata_max_source_bytes") return "number";
	return "string";
}

function renderUseAdminSettingsData() {
	const onPublicSiteUrlChanged = vi.fn();
	const hook = renderHook(() =>
		useAdminSettingsData({
			currentUserEmail: "admin@example.com",
			onPublicSiteUrlChanged,
			t,
		}),
	);

	return {
		...hook,
		onPublicSiteUrlChanged,
	};
}

describe("useAdminSettingsData", () => {
	beforeEach(async () => {
		const { invalidateAdminConfigMetadataCache } = await import(
			"@/lib/adminConfigMetadataCache"
		);
		invalidateAdminConfigMetadataCache();
		mockState.actionConfig.mockReset();
		mockState.brandingInvalidate.mockReset();
		mockState.brandingLoad.mockReset();
		mockState.deleteConfig.mockReset();
		mockState.handleApiError.mockReset();
		mockState.listConfigs.mockReset();
		mockState.previewInvalidate.mockReset();
		mockState.previewLoad.mockReset();
		mockState.schema.mockReset();
		mockState.sendTestEmail.mockReset();
		mockState.setConfig.mockReset();
		mockState.templateVariables.mockReset();
		mockState.thumbnailSupportInvalidate.mockReset();
		mockState.mediaDataSupportInvalidate.mockReset();
		mockState.thumbnailSupportLoad.mockReset();
		mockState.mediaDataSupportLoad.mockReset();
		mockState.toastSuccess.mockReset();

		mockState.listConfigs.mockResolvedValue({
			items: createBaseConfigs(),
		});
		mockState.schema.mockResolvedValue([]);
		mockState.templateVariables.mockResolvedValue([]);
		mockState.previewLoad.mockResolvedValue(undefined);
		mockState.brandingLoad.mockResolvedValue(undefined);
		mockState.thumbnailSupportLoad.mockResolvedValue(undefined);
		mockState.deleteConfig.mockResolvedValue(undefined);
		mockState.setConfig.mockImplementation(
			(key: string, value: string | string[]) =>
				Promise.resolve(
					createConfig({
						category: getConfigCategory(key),
						key,
						source: getMockConfigSource(key),
						value,
						value_type: getMockConfigValueType(key),
					}),
				),
		);
	});

	it("validates staged custom rows for required and duplicate keys", async () => {
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		act(() => {
			result.current.appendCustomDraftRow();
		});

		const [firstRow] = result.current.newCustomRows;
		expect(firstRow).toBeDefined();

		act(() => {
			result.current.updateNewCustomRow(firstRow.id, "value", "hello");
		});

		expect(result.current.newCustomRowErrors.get(firstRow.id)).toBe(
			"custom_config_key_required",
		);

		act(() => {
			result.current.appendCustomDraftRow();
		});

		const [, secondRow] = result.current.newCustomRows;
		expect(secondRow).toBeDefined();

		act(() => {
			result.current.updateNewCustomRow(secondRow.id, "key", "custom.theme");
			result.current.updateNewCustomRow(secondRow.id, "value", "sunset");
		});

		expect(result.current.changedCount).toBe(2);
		expect(result.current.hasValidationError).toBe(true);
		expect(result.current.newCustomRowErrors.get(secondRow.id)).toBe(
			"custom_config_key_duplicate",
		);

		await act(async () => {
			await result.current.handleSaveAll();
		});

		expect(mockState.setConfig).not.toHaveBeenCalled();
		expect(mockState.deleteConfig).not.toHaveBeenCalled();
	});

	it("surfaces preview app parse issues for invalid drafts", async () => {
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		act(() => {
			result.current.updateDraftValue(PREVIEW_APPS_CONFIG_KEY, "{");
		});

		expect(result.current.changedCount).toBe(1);
		expect(result.current.hasValidationError).toBe(true);
		expect(result.current.previewAppsValidationIssues).toEqual([
			{ key: "preview_apps_error_parse" },
		]);
	});

	it("blocks saving when staged CORS values enable credentials with a wildcard origin", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: createCorsConfigs(),
		});

		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		act(() => {
			result.current.updateDraftValue("cors_allow_credentials", "true");
		});

		expect(result.current.hasValidationError).toBe(true);
		expect(
			result.current.configValidationErrors.get("cors_allowed_origins"),
		).toBe("cors_wildcard_credentials_validation_error");
		expect(
			result.current.configValidationErrors.get("cors_allow_credentials"),
		).toBe("cors_wildcard_credentials_validation_error");
		expect(result.current.validationMessage).toBe(
			"cors_wildcard_credentials_validation_error",
		);

		await act(async () => {
			await result.current.handleSaveAll();
		});

		expect(mockState.setConfig).not.toHaveBeenCalled();
	});

	it("blocks saving when staged CORS values switch an existing credentialed policy back to a wildcard origin", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: createCorsConfigs({
				allowCredentials: "true",
				allowedOrigins: "https://panel.example.com",
			}),
		});

		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		act(() => {
			result.current.updateDraftValue("cors_allowed_origins", "*");
		});

		expect(result.current.hasValidationError).toBe(true);
		expect(
			result.current.configValidationErrors.get("cors_allowed_origins"),
		).toBe("cors_wildcard_credentials_validation_error");
		expect(
			result.current.configValidationErrors.get("cors_allow_credentials"),
		).toBe("cors_wildcard_credentials_validation_error");

		await act(async () => {
			await result.current.handleSaveAll();
		});

		expect(mockState.setConfig).not.toHaveBeenCalled();
	});

	it("clears the staged CORS validation once the wildcard is replaced with an explicit origin and then saves both changes", async () => {
		mockState.listConfigs.mockResolvedValueOnce({
			items: createCorsConfigs(),
		});

		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		act(() => {
			result.current.updateDraftValue("cors_allow_credentials", "true");
		});

		expect(result.current.hasValidationError).toBe(true);

		act(() => {
			result.current.updateDraftValue(
				"cors_allowed_origins",
				"https://panel.example.com",
			);
		});

		expect(result.current.hasValidationError).toBe(false);
		expect(result.current.configValidationErrors.size).toBe(0);

		await act(async () => {
			await result.current.handleSaveAll();
		});

		expect(mockState.setConfig).toHaveBeenCalledWith(
			"cors_allowed_origins",
			"https://panel.example.com",
		);
		expect(mockState.setConfig).toHaveBeenCalledWith(
			"cors_allow_credentials",
			"true",
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("settings_saved");
	});

	it("saves changes, syncs public site url, and reloads public config stores when preview config changes", async () => {
		const { onPublicSiteUrlChanged, result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		const nextPreviewValue = createValidPreviewAppsConfig([
			{
				config: {
					allowed_origins: ["https://viewer.example.com"],
					mode: "iframe",
					url_template:
						"https://viewer.example.com/embed?src={{file_preview_url}}",
				},
				enabled: true,
				extensions: ["md"],
				icon: "https://viewer.example.com/icon.svg",
				key: "custom.viewer",
				labels: {
					en: "Viewer",
				},
				provider: "url_template",
			},
		]);
		const nextMediaProcessingValue = createValidMediaProcessingConfig({
			processors: [
				{
					config: {
						command: "/usr/local/bin/vips",
					},
					enabled: true,
					extensions: ["heic", "avif"],
					kind: "vips_cli",
					uses: ["thumbnail:image"],
				},
				{
					config: {
						command: "ffmpeg",
					},
					enabled: false,
					extensions: ["mp4"],
					kind: "ffmpeg_cli",
					uses: ["thumbnail:video"],
				},
				{
					config: {
						command: "/opt/bin/ffprobe",
					},
					enabled: true,
					extensions: ["mp4"],
					kind: "ffprobe_cli",
					uses: ["metadata:video"],
				},
				{
					config: {
						command: "",
					},
					enabled: true,
					extensions: ["mp3"],
					kind: "lofty",
					uses: ["thumbnail:audio", "metadata:audio"],
				},
				{
					config: {
						command: "",
					},
					enabled: true,
					extensions: [],
					kind: "images",
					uses: ["thumbnail:image", "metadata:image"],
				},
			],
		});

		act(() => {
			result.current.updateDraftValue("public_site_url", [
				"https://next.example.com",
			]);
			result.current.updateDraftValue(
				PREVIEW_APPS_CONFIG_KEY,
				nextPreviewValue,
			);
			result.current.updateDraftValue(
				MEDIA_PROCESSING_CONFIG_KEY,
				nextMediaProcessingValue,
			);
			result.current.markCustomDeleted("custom.theme");
			result.current.appendCustomDraftRow();
		});

		const [newRow] = result.current.newCustomRows;
		expect(newRow).toBeDefined();

		act(() => {
			result.current.updateNewCustomRow(newRow.id, "key", "custom.accent");
			result.current.updateNewCustomRow(newRow.id, "value", "sunset");
		});

		await act(async () => {
			await result.current.handleSaveAll();
		});

		expect(mockState.deleteConfig).toHaveBeenCalledWith("custom.theme");
		expect(mockState.setConfig).toHaveBeenCalledWith("public_site_url", [
			"https://next.example.com",
		]);
		expect(mockState.setConfig).toHaveBeenCalledWith(
			PREVIEW_APPS_CONFIG_KEY,
			nextPreviewValue,
		);
		expect(mockState.setConfig).toHaveBeenCalledWith(
			MEDIA_PROCESSING_CONFIG_KEY,
			nextMediaProcessingValue,
		);
		expect(mockState.setConfig).toHaveBeenCalledWith("custom.accent", "sunset");
		expect(onPublicSiteUrlChanged).toHaveBeenCalledWith([
			"https://next.example.com",
		]);
		expect(mockState.brandingInvalidate).toHaveBeenCalledTimes(1);
		expect(mockState.brandingLoad).toHaveBeenCalledWith({ force: true });
		expect(mockState.previewInvalidate).toHaveBeenCalledTimes(1);
		expect(mockState.previewLoad).toHaveBeenCalledWith({ force: true });
		expect(mockState.thumbnailSupportInvalidate).toHaveBeenCalledTimes(1);
		expect(mockState.thumbnailSupportLoad).toHaveBeenCalledWith({
			force: true,
		});
		expect(mockState.mediaDataSupportInvalidate).toHaveBeenCalledTimes(1);
		expect(mockState.mediaDataSupportLoad).toHaveBeenCalledWith({
			force: true,
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("settings_saved");

		await waitFor(() => {
			expect(result.current.hasUnsavedChanges).toBe(false);
		});

		expect(
			result.current.visibleCustomConfigs.map((config) => config.key),
		).toEqual(["custom.accent"]);
	});

	it("reloads branding but not preview or thumbnail stores when only public_site_url changes", async () => {
		const { onPublicSiteUrlChanged, result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		act(() => {
			result.current.updateDraftValue("public_site_url", [
				"https://only-public.example.com",
			]);
		});

		await act(async () => {
			await result.current.handleSaveAll();
		});

		expect(mockState.setConfig).toHaveBeenCalledWith("public_site_url", [
			"https://only-public.example.com",
		]);
		expect(onPublicSiteUrlChanged).toHaveBeenCalledWith([
			"https://only-public.example.com",
		]);
		expect(mockState.brandingInvalidate).toHaveBeenCalledTimes(1);
		expect(mockState.brandingLoad).toHaveBeenCalledWith({ force: true });
		expect(mockState.previewInvalidate).not.toHaveBeenCalled();
		expect(mockState.previewLoad).not.toHaveBeenCalled();
		expect(mockState.thumbnailSupportInvalidate).not.toHaveBeenCalled();
		expect(mockState.thumbnailSupportLoad).not.toHaveBeenCalled();
		expect(mockState.mediaDataSupportInvalidate).not.toHaveBeenCalled();
		expect(mockState.mediaDataSupportLoad).not.toHaveBeenCalled();
	});

	it("tests the media processing ffprobe command against the current draft", async () => {
		mockState.actionConfig.mockResolvedValueOnce({
			message: "ffprobe command '/opt/bin/ffprobe-custom' is available",
		});
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		await act(async () => {
			await result.current.handleTestFfprobeCliCommand(
				createValidMediaProcessingConfig({
					processors: [
						{
							config: {
								command: "vips",
							},
							enabled: false,
							extensions: ["heic"],
							kind: "vips_cli",
							uses: ["thumbnail:image"],
						},
						{
							config: {
								command: "ffmpeg",
							},
							enabled: false,
							extensions: ["mp4"],
							kind: "ffmpeg_cli",
							uses: ["thumbnail:video"],
						},
						{
							config: {
								command: "/opt/bin/ffprobe-custom",
							},
							enabled: false,
							extensions: ["mp4"],
							kind: "ffprobe_cli",
							uses: ["metadata:video"],
						},
						{
							config: {
								command: "",
							},
							enabled: true,
							extensions: ["mp3"],
							kind: "lofty",
							uses: ["thumbnail:audio", "metadata:audio"],
						},
						{
							config: {
								command: "",
							},
							enabled: true,
							extensions: [],
							kind: "images",
							uses: ["thumbnail:image", "metadata:image"],
						},
					],
				}),
			);
		});

		expect(mockState.actionConfig).toHaveBeenCalledWith(
			MEDIA_PROCESSING_CONFIG_KEY,
			expect.objectContaining({
				action: "test_ffprobe_cli",
				value: expect.stringContaining("/opt/bin/ffprobe-custom"),
			}),
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"ffprobe command '/opt/bin/ffprobe-custom' is available",
		);
	});

	it("reloads configs after save failure and reports the error", async () => {
		const error = new Error("save failed");
		mockState.setConfig.mockRejectedValueOnce(error);

		const { onPublicSiteUrlChanged, result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		act(() => {
			result.current.updateDraftValue("public_site_url", [
				"https://broken.example.com",
			]);
		});

		await act(async () => {
			await result.current.handleSaveAll();
		});

		expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		await waitFor(() => {
			expect(mockState.listConfigs).toHaveBeenCalledTimes(2);
		});
		expect(mockState.schema).toHaveBeenCalledTimes(1);
		expect(mockState.templateVariables).toHaveBeenCalledTimes(1);
		expect(onPublicSiteUrlChanged).not.toHaveBeenCalled();
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
		expect(result.current.saving).toBe(false);
	});
});
