import { MEDIA_PROCESSING_CONFIG_KEY } from "@/components/admin/mediaProcessingConfigEditorShared";
import { OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY } from "@/components/admin/offlineDownloadEngineRegistryShared";
import { PREVIEW_APPS_CONFIG_KEY } from "@/components/admin/previewAppsConfigEditorShared";
import {
	configDraftValueChanged,
	configValueToString,
	type DraftValues,
} from "@/components/admin/settings/adminSettingsContentShared";
import { adminConfigService } from "@/services/adminService";
import type {
	ConfigActionType,
	ConfigSchemaItem,
	ExecuteConfigActionRequest,
	ExecuteConfigActionResponse,
	SystemConfig,
} from "@/types/api";

const MAIL_CONFIG_ACTION_KEY = "mail";
export const OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY =
	"offline_download_aria2_rpc_url";
export const OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY =
	"offline_download_aria2_rpc_secret";
export const OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY =
	"offline_download_aria2_request_timeout_secs";

const OFFLINE_DOWNLOAD_ACTION_DRAFT_KEYS = [
	OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
	OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY,
	OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
	OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY,
];

export type AdminSettingsActionInput =
	| {
			actionId: "build_wopi_discovery_preview_config";
			discoveryUrl: string;
			value: string;
	  }
	| {
			actionId: "send_test_email";
			targetEmail?: string;
	  }
	| {
			actionId: "test_vips_cli" | "test_ffmpeg_cli" | "test_ffprobe_cli";
			value: string;
	  }
	| {
			actionId: "test_aria2_rpc";
			value: string;
	  };

export interface AdminSettingsActionContext {
	configs: SystemConfig[];
	draftValues: DraftValues;
	schemas?: ConfigSchemaItem[];
}

export interface AdminSettingsActionRequest {
	configKey: string;
	request: ExecuteConfigActionRequest;
}

function configsByKey(configs: SystemConfig[]) {
	return new Map(configs.map((config) => [config.key, config] as const));
}

export function getAdminSettingsActionDescriptors(
	schemas: ConfigSchemaItem[] | undefined,
) {
	return (schemas ?? []).flatMap((schema) => {
		const actions = schema.actions;
		return actions ?? [];
	});
}

export function findAdminSettingsActionDescriptor(
	schemas: ConfigSchemaItem[] | undefined,
	action: ConfigActionType,
) {
	return (
		getAdminSettingsActionDescriptors(schemas).find(
			(descriptor) => descriptor.action === action,
		) ?? null
	);
}

function actionTargetKey(
	context: AdminSettingsActionContext,
	action: ConfigActionType,
	fallback: string,
) {
	return (
		findAdminSettingsActionDescriptor(context.schemas, action)?.target_key ??
		fallback
	);
}

function actionDraftKeys(
	context: AdminSettingsActionContext,
	action: ConfigActionType,
	fallback: readonly string[],
) {
	const descriptor = findAdminSettingsActionDescriptor(context.schemas, action);
	return descriptor?.draft_value_keys?.length
		? descriptor.draft_value_keys
		: fallback;
}

export function buildChangedDraftValuesForAction({
	configs,
	draftKeys,
	draftValues,
	overrideValues,
}: {
	configs: SystemConfig[];
	draftKeys: readonly string[];
	draftValues: DraftValues;
	overrideValues?: Partial<Record<string, DraftValues[string]>>;
}) {
	const configMap = configsByKey(configs);

	return Object.fromEntries(
		draftKeys.flatMap((key) => {
			const config = configMap.get(key);
			const draftValue = Object.hasOwn(overrideValues ?? {}, key)
				? overrideValues?.[key]
				: draftValues[key];
			if (
				draftValue == null ||
				(config && !configDraftValueChanged(config, draftValue))
			) {
				return [];
			}
			return [[key, configValueToString(draftValue)]];
		}),
	);
}

export function buildAdminSettingsActionRequest(
	input: AdminSettingsActionInput,
	context: AdminSettingsActionContext,
): AdminSettingsActionRequest {
	switch (input.actionId) {
		case "build_wopi_discovery_preview_config":
			return {
				configKey: actionTargetKey(
					context,
					input.actionId,
					PREVIEW_APPS_CONFIG_KEY,
				),
				request: {
					action: input.actionId satisfies ConfigActionType,
					discovery_url: input.discoveryUrl,
					value: input.value,
				},
			};
		case "send_test_email":
			return {
				configKey: actionTargetKey(
					context,
					input.actionId,
					MAIL_CONFIG_ACTION_KEY,
				),
				request: {
					action: input.actionId satisfies ConfigActionType,
					target_email: input.targetEmail,
				},
			};
		case "test_vips_cli":
		case "test_ffmpeg_cli":
		case "test_ffprobe_cli":
			return {
				configKey: actionTargetKey(
					context,
					input.actionId,
					MEDIA_PROCESSING_CONFIG_KEY,
				),
				request: {
					action: input.actionId satisfies ConfigActionType,
					value: input.value,
				},
			};
		case "test_aria2_rpc":
			return {
				configKey: actionTargetKey(
					context,
					input.actionId,
					OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
				),
				request: {
					action: input.actionId satisfies ConfigActionType,
					draft_values: buildChangedDraftValuesForAction({
						configs: context.configs,
						draftKeys: actionDraftKeys(
							context,
							input.actionId,
							OFFLINE_DOWNLOAD_ACTION_DRAFT_KEYS,
						),
						draftValues: context.draftValues,
						overrideValues: {
							[OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY]: input.value,
						},
					}),
					value: input.value,
				},
			};
	}
}

export async function executeAdminSettingsAction(
	input: AdminSettingsActionInput,
	context: AdminSettingsActionContext,
): Promise<ExecuteConfigActionResponse> {
	const { configKey, request } = buildAdminSettingsActionRequest(
		input,
		context,
	);
	return adminConfigService.action(configKey, request);
}
