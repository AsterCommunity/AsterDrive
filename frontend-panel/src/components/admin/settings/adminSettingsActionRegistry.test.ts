import { describe, expect, it } from "vitest";
import { OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY } from "@/components/admin/offlineDownloadEngineRegistryShared";
import { PREVIEW_APPS_CONFIG_KEY } from "@/components/admin/previewAppsConfigEditorShared";
import {
	buildAdminSettingsActionRequest,
	buildChangedDraftValuesForAction,
	OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY,
	OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
	OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY,
} from "@/components/admin/settings/adminSettingsActionRegistry";
import type { DraftValues } from "@/components/admin/settings/adminSettingsContentShared";
import type { SystemConfig } from "@/types/api";

function createConfig(overrides: Partial<SystemConfig> = {}): SystemConfig {
	return {
		category: "file_processing.offline_download",
		description: "",
		id: 1,
		is_sensitive: false,
		key: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
		requires_restart: false,
		source: "system",
		updated_at: "2026-04-15T00:00:00Z",
		updated_by: null,
		value: "saved",
		value_type: "string",
		visibility: "private",
		...overrides,
	};
}

describe("adminSettingsActionRegistry", () => {
	it("builds media processing action requests through the registry", () => {
		expect(
			buildAdminSettingsActionRequest(
				{ actionId: "test_ffmpeg_cli", value: "ffmpeg-json" },
				{ configs: [], draftValues: {} },
			),
		).toEqual({
			configKey: "media_processing_registry_json",
			request: {
				action: "test_ffmpeg_cli",
				value: "ffmpeg-json",
			},
		});
	});

	it("builds WOPI discovery action requests through the registry", () => {
		expect(
			buildAdminSettingsActionRequest(
				{
					actionId: "build_wopi_discovery_preview_config",
					discoveryUrl: "https://office.example.com/hosting/discovery",
					value: "{}",
				},
				{ configs: [], draftValues: {} },
			),
		).toEqual({
			configKey: PREVIEW_APPS_CONFIG_KEY,
			request: {
				action: "build_wopi_discovery_preview_config",
				discovery_url: "https://office.example.com/hosting/discovery",
				value: "{}",
			},
		});
	});

	it("uses schema action descriptors for action target keys", () => {
		expect(
			buildAdminSettingsActionRequest(
				{
					actionId: "send_test_email",
					targetEmail: "ops@example.com",
				},
				{
					configs: [],
					draftValues: {},
					schemas: [
						{
							category: "mail.config",
							description: "",
							description_i18n_key: "settings_item_mail_smtp_host_desc",
							is_sensitive: false,
							key: "mail_smtp_host",
							label_i18n_key: "settings_item_mail_smtp_host_label",
							requires_restart: false,
							value_type: "string",
							actions: [
								{
									action: "send_test_email",
									label_i18n_key: "mail_send_test_email",
									presentation: {
										category: "mail",
										group: "test",
										order: 10,
										subcategory: "config",
									},
									target_key: "mail",
								},
							],
						},
					],
				},
			),
		).toEqual({
			configKey: "mail",
			request: {
				action: "send_test_email",
				target_email: "ops@example.com",
			},
		});
	});

	it("only includes changed offline download draft values for aria2 actions", () => {
		const draftValues: DraftValues = {
			[OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY]: "http://draft-aria2:6800/jsonrpc",
			[OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY]: "",
			[OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY]: "10",
		};

		const draftValuesForAction = buildChangedDraftValuesForAction({
			configs: [
				createConfig({
					key: OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY,
					value: "http://saved-aria2:6800/jsonrpc",
				}),
				createConfig({
					is_sensitive: true,
					key: OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
					value: "***REDACTED***",
				}),
				createConfig({
					key: OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY,
					value: "10",
				}),
			],
			draftKeys: [
				OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY,
				OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
				OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY,
			],
			draftValues,
		});

		expect(draftValuesForAction).toEqual({
			[OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY]: "http://draft-aria2:6800/jsonrpc",
		});
	});

	it("preserves a literal redacted marker when the admin intentionally types it", () => {
		const request = buildAdminSettingsActionRequest(
			{
				actionId: "test_aria2_rpc",
				value: "registry-draft",
			},
			{
				configs: [
					createConfig({
						key: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
						value: "saved-registry",
					}),
					createConfig({
						is_sensitive: true,
						key: OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
						value: "***REDACTED***",
					}),
				],
				draftValues: {
					[OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY]: "***REDACTED***",
				},
			},
		);

		expect(request).toEqual({
			configKey: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
			request: {
				action: "test_aria2_rpc",
				draft_values: {
					[OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY]: "registry-draft",
					[OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY]: "***REDACTED***",
				},
				value: "registry-draft",
			},
		});
	});
});
