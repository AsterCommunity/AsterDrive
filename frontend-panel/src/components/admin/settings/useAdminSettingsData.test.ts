import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	MEDIA_PROCESSING_CONFIG_KEY,
	MEDIA_PROCESSING_CONFIG_VERSION,
	type MediaProcessingEditorConfig,
} from "@/components/admin/mediaProcessingConfigEditorShared";
import {
	defaultOfflineDownloadEngineConfig,
	OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
} from "@/components/admin/offlineDownloadEngineRegistryShared";
import {
	PREVIEW_APP_PROTECTED_BUILTIN_KEYS,
	PREVIEW_APPS_CONFIG_KEY,
} from "@/components/admin/previewAppsConfigEditorShared";
import { useAdminSettingsData } from "@/components/admin/settings/useAdminSettingsData";
import type {
	SystemConfig,
	SystemConfigSource,
	SystemConfigValueType,
	SystemConfigVisibility,
} from "@/types/api";

const mockState = vi.hoisted(() => ({
	actionConfig: vi.fn(),
	deleteConfig: vi.fn(),
	frontendConfigInvalidate: vi.fn(),
	frontendConfigLoad: vi.fn(),
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

const OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY = "offline_download_aria2_rpc_url";
const OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY =
	"offline_download_aria2_rpc_secret";
const OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY =
	"offline_download_aria2_request_timeout_secs";

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

vi.mock("@/stores/frontendConfigStore", () => {
	const useFrontendConfigStore = Object.assign(vi.fn(), {
		getState: () => ({
			invalidate: mockState.frontendConfigInvalidate,
			load: mockState.frontendConfigLoad,
		}),
	});

	return { useFrontendConfigStore };
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
			category: "site.preview",
			key: PREVIEW_APPS_CONFIG_KEY,
			value: createValidPreviewAppsConfig(),
			value_type: "multiline",
		}),
		createConfig({
			category: "file_processing.media",
			key: MEDIA_PROCESSING_CONFIG_KEY,
			value: createValidMediaProcessingConfig(),
			value_type: "multiline",
		}),
		createConfig({
			category: "file_processing.media",
			key: "media_metadata_enabled",
			value: "true",
			value_type: "boolean",
		}),
		createConfig({
			category: "file_processing.media",
			key: "media_metadata_max_source_bytes",
			value: "1073741824",
			value_type: "number",
		}),
		createConfig({
			category: "file_processing.offline_download",
			key: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
			value: JSON.stringify(defaultOfflineDownloadEngineConfig(), null, 2),
			value_type: "multiline",
		}),
		createConfig({
			category: "file_processing.offline_download",
			key: OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY,
			value: "http://saved-aria2:6800/jsonrpc",
		}),
		createConfig({
			category: "file_processing.offline_download",
			is_sensitive: true,
			key: OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
			value: "***REDACTED***",
		}),
		createConfig({
			category: "file_processing.offline_download",
			key: OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY,
			value: "10",
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
	if (key === PREVIEW_APPS_CONFIG_KEY) return "site.preview";
	if (key === MEDIA_PROCESSING_CONFIG_KEY) return "file_processing.media";
	if (key === OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY)
		return "file_processing.offline_download";
	if (key.startsWith("offline_download_aria2_"))
		return "file_processing.offline_download";
	if (key.startsWith("media_metadata_")) return "file_processing.media";
	return "site";
}

function getMockConfigSource(key: string): SystemConfigSource {
	return key.startsWith("custom") ? "custom" : "system";
}

function getMockConfigValueType(key: string): SystemConfigValueType {
	if (key === "public_site_url") return "string_array";
	if (
		key === PREVIEW_APPS_CONFIG_KEY ||
		key === MEDIA_PROCESSING_CONFIG_KEY ||
		key === OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY
	)
		return "multiline";
	if (key === "media_metadata_enabled") return "boolean";
	if (key === "media_metadata_max_source_bytes") return "number";
	if (key === OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY) return "number";
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
		mockState.deleteConfig.mockReset();
		mockState.frontendConfigInvalidate.mockReset();
		mockState.frontendConfigLoad.mockReset();
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
		mockState.frontendConfigLoad.mockResolvedValue(undefined);
		mockState.thumbnailSupportLoad.mockResolvedValue(undefined);
		mockState.deleteConfig.mockResolvedValue(undefined);
		mockState.setConfig.mockImplementation(
			(
				key: string,
				value: string | string[],
				visibility?: SystemConfigVisibility,
			) =>
				Promise.resolve(
					createConfig({
						category: getConfigCategory(key),
						key,
						source: getMockConfigSource(key),
						value,
						value_type: getMockConfigValueType(key),
						visibility: visibility ?? "private",
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

	it("keeps redacted sensitive configs empty and unchanged after loading", async () => {
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		const secretConfig = result.current.configs.find(
			(config) => config.key === OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
		);
		expect(secretConfig).toBeDefined();
		expect(result.current.getDraftValue(secretConfig as SystemConfig)).toBe("");
		expect(result.current.changedCount).toBe(0);
		expect(result.current.hasUnsavedChanges).toBe(false);
	});

	it("does not save unchanged redacted sensitive configs when another config changes", async () => {
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		act(() => {
			result.current.updateDraftValue("public_site_url", [
				"https://next.example.com",
			]);
		});

		await act(async () => {
			await result.current.handleSaveAll();
		});

		expect(mockState.setConfig).toHaveBeenCalledWith("public_site_url", [
			"https://next.example.com",
		]);
		expect(mockState.setConfig).not.toHaveBeenCalledWith(
			OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
			expect.anything(),
		);
	});

	it("saves a literal redacted marker when the admin types it as a new sensitive value", async () => {
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		act(() => {
			result.current.updateDraftValue(
				OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
				"***REDACTED***",
			);
		});

		expect(result.current.changedCount).toBe(1);

		await act(async () => {
			await result.current.handleSaveAll();
		});

		expect(mockState.setConfig).toHaveBeenCalledWith(
			OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
			"***REDACTED***",
		);
	});

	it("saves custom config visibility for existing and new custom rows", async () => {
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		const existingConfig = result.current.visibleCustomConfigs.find(
			(config) => config.key === "custom.theme",
		);
		expect(existingConfig).toBeDefined();

		act(() => {
			result.current.updateCustomVisibilityDraft(
				"custom.theme",
				"authenticated",
			);
			result.current.appendCustomDraftRow();
		});

		const [row] = result.current.newCustomRows;
		expect(row).toBeDefined();

		act(() => {
			result.current.updateNewCustomRow(row.id, "key", "custom.banner");
			result.current.updateNewCustomRow(row.id, "value", "visible");
			result.current.updateNewCustomRow(row.id, "visibility", "public");
		});

		expect(result.current.changedCount).toBe(2);
		expect(result.current.hasValidationError).toBe(false);

		await act(async () => {
			await result.current.handleSaveAll();
		});

		expect(mockState.setConfig).toHaveBeenCalledWith(
			"custom.theme",
			"ocean",
			"authenticated",
		);
		expect(mockState.setConfig).toHaveBeenCalledWith(
			"custom.banner",
			"visible",
			"public",
		);
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

	it("blocks saving when offline download engine registry is invalid", async () => {
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		act(() => {
			result.current.updateDraftValue(
				OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
				"{",
			);
		});

		expect(result.current.changedCount).toBe(1);
		expect(result.current.hasValidationError).toBe(true);
		expect(result.current.validationMessage).toBe(
			"offline_download_engine_validation_error",
		);

		await act(async () => {
			await result.current.handleSaveAll();
		});

		expect(mockState.setConfig).not.toHaveBeenCalled();
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
		expect(mockState.setConfig).toHaveBeenCalledWith(
			"custom.accent",
			"sunset",
			"private",
		);
		expect(onPublicSiteUrlChanged).toHaveBeenCalledWith([
			"https://next.example.com",
		]);
		expect(mockState.frontendConfigInvalidate).toHaveBeenCalledTimes(1);
		expect(mockState.frontendConfigLoad).toHaveBeenCalledWith({ force: true });
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

	it("reloads frontend config but not preview or thumbnail stores when only public_site_url changes", async () => {
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
		expect(mockState.frontendConfigInvalidate).toHaveBeenCalledTimes(1);
		expect(mockState.frontendConfigLoad).toHaveBeenCalledWith({ force: true });
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

	it("tests aria2 RPC against current unsaved drafts", async () => {
		mockState.actionConfig.mockResolvedValueOnce({
			message: "aria2 RPC ready: version 1.37.0, 12 enabled features",
		});
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		const registryDraft = JSON.stringify(
			{
				version: 1,
				engines: [
					{ kind: "aria2", enabled: true },
					{ kind: "builtin", enabled: true },
				],
			},
			null,
			2,
		);

		act(() => {
			result.current.updateDraftValue(
				OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY,
				"http://draft-aria2:6800/jsonrpc",
			);
			result.current.updateDraftValue(
				OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
				"draft-secret",
			);
			result.current.updateDraftValue(
				OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY,
				"3",
			);
		});

		await act(async () => {
			await result.current.handleTestAria2Rpc(registryDraft);
		});

		expect(mockState.actionConfig).toHaveBeenCalledWith(
			OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
			expect.objectContaining({
				action: "test_aria2_rpc",
				draft_values: expect.objectContaining({
					[OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY]: registryDraft,
					[OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY]:
						"http://draft-aria2:6800/jsonrpc",
					[OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY]: "draft-secret",
					[OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY]: "3",
				}),
				value: registryDraft,
			}),
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"aria2 RPC ready: version 1.37.0, 12 enabled features",
		);
	});

	it("tests aria2 RPC with saved config when there are no offline download drafts", async () => {
		mockState.actionConfig.mockResolvedValueOnce({
			message: "aria2 RPC ready: version 1.37.0, 12 enabled features",
		});
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		const savedRegistry = result.current.configs.find(
			(config) => config.key === OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
		)?.value as string;

		await act(async () => {
			await result.current.handleTestAria2Rpc(savedRegistry);
		});

		expect(mockState.actionConfig).toHaveBeenCalledWith(
			OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
			{
				action: "test_aria2_rpc",
				draft_values: {},
				value: savedRegistry,
			},
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"aria2 RPC ready: version 1.37.0, 12 enabled features",
		);
	});

	it("does not send the redacted aria2 secret when only other draft fields changed", async () => {
		mockState.actionConfig.mockResolvedValueOnce({
			message: "aria2 RPC ready: version 1.37.0, 12 enabled features",
		});
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		const savedRegistry = result.current.configs.find(
			(config) => config.key === OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
		)?.value as string;

		act(() => {
			result.current.updateDraftValue(
				OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY,
				"http://draft-aria2:6800/jsonrpc",
			);
		});

		await act(async () => {
			await result.current.handleTestAria2Rpc(savedRegistry);
		});

		expect(mockState.actionConfig).toHaveBeenCalledWith(
			OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
			{
				action: "test_aria2_rpc",
				draft_values: {
					[OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY]:
						"http://draft-aria2:6800/jsonrpc",
				},
				value: savedRegistry,
			},
		);
		expect(mockState.actionConfig).not.toHaveBeenCalledWith(
			OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
			expect.objectContaining({
				draft_values: expect.objectContaining({
					[OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY]: "***REDACTED***",
				}),
			}),
		);
	});

	it("sends a literal redacted marker when the admin types it as a new aria2 secret", async () => {
		mockState.actionConfig.mockResolvedValueOnce({
			message: "aria2 RPC ready: version 1.37.0, 12 enabled features",
		});
		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		const savedRegistry = result.current.configs.find(
			(config) => config.key === OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
		)?.value as string;

		act(() => {
			result.current.updateDraftValue(
				OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
				"***REDACTED***",
			);
		});

		await act(async () => {
			await result.current.handleTestAria2Rpc(savedRegistry);
		});

		expect(mockState.actionConfig).toHaveBeenCalledWith(
			OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
			{
				action: "test_aria2_rpc",
				draft_values: {
					[OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY]: "***REDACTED***",
				},
				value: savedRegistry,
			},
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

	it("loads all config pages when the first page is full", async () => {
		const firstPage = Array.from({ length: 100 }, (_, index) =>
			createConfig({
				key: `dummy_config_${index}`,
				value: String(index),
			}),
		);
		mockState.listConfigs
			.mockResolvedValueOnce({
				items: firstPage,
				limit: 100,
				offset: 0,
				total: 101,
			})
			.mockResolvedValueOnce({
				items: [
					createConfig({
						category: "mail.template",
						key: "mail_template_login_email_code_html",
						value: "<p>{{code}}</p>",
						value_type: "multiline",
					}),
				],
				limit: 100,
				offset: 100,
				total: 101,
			});

		const { result } = renderUseAdminSettingsData();

		await waitFor(() => expect(result.current.loading).toBe(false));

		expect(mockState.listConfigs).toHaveBeenCalledWith({
			limit: 100,
			offset: 0,
		});
		expect(mockState.listConfigs).toHaveBeenCalledWith({
			limit: 100,
			offset: 100,
		});
		expect(result.current.systemGroups.mail).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					key: "mail_template_login_email_code_html",
				}),
			]),
		);
	});
});
