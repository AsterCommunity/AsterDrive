import {
	type ConfigDraftValue,
	type NewCustomDraft,
	serializeConfigDraftValue,
} from "@/components/admin/settings/adminSettingsContentShared";
import { collectAdminSettingsInvalidationTargets } from "@/components/admin/settings/adminSettingsInvalidation";
import { adminConfigService } from "@/services/adminService";
import type { SystemConfig, SystemConfigVisibility } from "@/types/api";

const AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY = "auth_email_code_login_enabled";
const PUBLIC_SITE_URL_KEY = "public_site_url";

export interface AdminSettingsSaveTransactionInput {
	activeNewCustomRows: NewCustomDraft[];
	changedExistingConfigs: SystemConfig[];
	configs: SystemConfig[];
	deletedCustomConfigs: SystemConfig[];
	getCustomVisibilityDraft: (config: SystemConfig) => SystemConfigVisibility;
	getDraftValue: (config: SystemConfig) => ConfigDraftValue;
}

export interface AdminSettingsSaveTransactionResult {
	invalidationTargets: ReturnType<
		typeof collectAdminSettingsInvalidationTargets
	>;
	nextConfigs: SystemConfig[];
	nextPublicSiteUrl: SystemConfig["value"] | undefined;
}

function saveOrderPriority(config: SystemConfig) {
	return config.key === AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY ? 1 : 0;
}

export async function executeAdminSettingsSaveTransaction({
	activeNewCustomRows,
	changedExistingConfigs,
	configs,
	deletedCustomConfigs,
	getCustomVisibilityDraft,
	getDraftValue,
}: AdminSettingsSaveTransactionInput): Promise<AdminSettingsSaveTransactionResult> {
	const invalidationTargets = collectAdminSettingsInvalidationTargets(
		changedExistingConfigs,
	);
	const nextConfigsByKey = new Map(
		configs.map((config) => [config.key, config] as const),
	);

	for (const config of deletedCustomConfigs) {
		await adminConfigService.delete(config.key);
		nextConfigsByKey.delete(config.key);
	}

	const orderedChangedConfigs = [...changedExistingConfigs].sort(
		(left, right) => saveOrderPriority(left) - saveOrderPriority(right),
	);

	for (const config of orderedChangedConfigs) {
		const nextValue = getDraftValue(config);
		const serializedValue = serializeConfigDraftValue(nextValue);
		const savedConfig =
			config.source === "system"
				? await adminConfigService.set(config.key, serializedValue)
				: await adminConfigService.set(
						config.key,
						serializedValue,
						getCustomVisibilityDraft(config),
					);
		nextConfigsByKey.set(
			config.key,
			savedConfig.key === config.key
				? savedConfig
				: { ...config, value: serializedValue },
		);
	}

	for (const row of activeNewCustomRows) {
		const key = row.key.trim();
		const savedConfig = await adminConfigService.set(
			key,
			row.value,
			row.visibility,
		);
		if (savedConfig.key !== key) {
			throw new Error(`Saved config key mismatch: expected ${key}`);
		}
		nextConfigsByKey.set(key, savedConfig);
	}

	return {
		invalidationTargets,
		nextConfigs: Array.from(nextConfigsByKey.values()),
		nextPublicSiteUrl: nextConfigsByKey.get(PUBLIC_SITE_URL_KEY)?.value,
	};
}
