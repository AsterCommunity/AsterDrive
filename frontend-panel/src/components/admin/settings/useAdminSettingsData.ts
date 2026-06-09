import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import {
	getMediaProcessingConfigIssuesFromString,
	MEDIA_PROCESSING_CONFIG_KEY,
} from "@/components/admin/mediaProcessingConfigEditorShared";
import {
	getOfflineDownloadEngineConfigIssuesFromString,
	OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
} from "@/components/admin/offlineDownloadEngineRegistryShared";
import {
	getPreviewAppsConfigIssuesFromString,
	PREVIEW_APPS_CONFIG_KEY,
} from "@/components/admin/previewAppsConfigEditorShared";
import {
	buildDraftValues,
	configDraftValueChanged,
	configDraftValuesEqual,
	configValueToString,
	configValueToStringArray,
	type DraftValues,
	formatSubcategoryLabel,
	getConfigDescription,
	getConfigIsSensitive,
	getConfigValueType,
	getMailTemplateGroupId,
	getMailTemplateGroupOrderIndex,
	getSubcategoryGroupKey,
	isRedactedConfigValue,
	isStringEnumSetType,
	isSystemConfigSource,
	type NewCustomDraft,
	normalizeCategory,
	normalizeSubcategory,
	type SizeDisplayUnitValue,
	type SystemSubcategoryGroup,
	serializeConfigDraftValue,
	sortConfigsByKey,
	type TimeDisplayUnitValue,
} from "@/components/admin/settings/adminSettingsContentShared";
import {
	isMailDeliveryConfigReady,
	MAIL_DELIVERY_CONFIG_KEYS,
} from "@/components/admin/settings/mailDeliveryConfigReady";
import { handleApiError } from "@/hooks/useApiError";
import {
	loadAdminConfigSchema,
	loadAdminTemplateVariables,
	readAdminConfigSchemaCache,
	readAdminTemplateVariablesCache,
} from "@/lib/adminConfigMetadataCache";
import { adminConfigService } from "@/services/adminService";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import { useMediaDataSupportStore } from "@/stores/mediaDataSupportStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";
import type {
	ConfigActionType,
	ConfigSchemaItem,
	SystemConfig,
	SystemConfigVisibility,
	TemplateVariableGroup,
} from "@/types/api";

const CONFIG_PAGE_SIZE = 100;
const PUBLIC_SITE_URL_KEY = "public_site_url";
const CORS_ALLOWED_ORIGINS_KEY = "cors_allowed_origins";
const CORS_ALLOW_CREDENTIALS_KEY = "cors_allow_credentials";
const AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY = "auth_email_code_login_enabled";
const LEGACY_OFFLINE_DOWNLOAD_ENGINE_KEY = "offline_download_engine";
const OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY = "offline_download_aria2_rpc_url";
const OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY =
	"offline_download_aria2_rpc_secret";
const OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY =
	"offline_download_aria2_request_timeout_secs";
const OFFLINE_DOWNLOAD_ACTION_DRAFT_KEYS = [
	OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
	OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY,
	OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
	OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY,
];
const PUBLIC_BRANDING_CONFIG_KEYS = new Set([
	PUBLIC_SITE_URL_KEY,
	"allow_user_registration",
	"branding_title",
	"branding_description",
	"branding_favicon_url",
	"branding_wordmark_dark_url",
	"branding_wordmark_light_url",
]);
const FRONTEND_CONFIG_KEYS = new Set([
	...PUBLIC_BRANDING_CONFIG_KEYS,
	"frontend_image_preview_preference",
]);
const MEDIA_DATA_SUPPORT_CONFIG_KEYS = new Set([
	MEDIA_PROCESSING_CONFIG_KEY,
	"media_metadata_enabled",
	"media_metadata_max_source_bytes",
]);

function draftValueChangedForKey(
	configsByKey: Map<string, SystemConfig>,
	key: string,
	value: DraftValues[string] | undefined,
) {
	const config = configsByKey.get(key);
	if (!config) {
		return !configDraftValuesEqual(value, undefined);
	}
	return configDraftValueChanged(config, value);
}

function effectiveDraftValueForConfig(
	config: SystemConfig | undefined,
	value: DraftValues[string] | undefined,
) {
	if (
		config &&
		getConfigIsSensitive(config) &&
		isRedactedConfigValue(config.value) &&
		configValueToString(value).trim() === ""
	) {
		return config.value as DraftValues[string];
	}
	return value;
}

type TranslationFn = (key: string, options?: Record<string, unknown>) => string;

async function loadAllSystemConfigs() {
	const items: SystemConfig[] = [];
	let offset = 0;

	while (true) {
		const page = await adminConfigService.list({
			limit: CONFIG_PAGE_SIZE,
			offset,
		});
		items.push(...page.items);

		const nextOffset = page.offset + page.items.length;
		const total = Number(page.total);
		const loadedAllKnownItems = Number.isFinite(total) && nextOffset >= total;
		const reachedShortPage =
			!Number.isFinite(total) && page.items.length < CONFIG_PAGE_SIZE;
		if (loadedAllKnownItems || reachedShortPage || page.items.length === 0) {
			break;
		}
		offset = nextOffset;
	}

	return items;
}

interface UseAdminSettingsDataProps {
	currentUserEmail: string;
	onPublicSiteUrlChanged: (value: string[] | null | undefined) => void;
	t: TranslationFn;
}

export function useAdminSettingsData({
	currentUserEmail,
	onPublicSiteUrlChanged,
	t,
}: UseAdminSettingsDataProps) {
	const customDraftIdRef = useRef(0);
	const [configs, setConfigs] = useState<SystemConfig[]>([]);
	const [schemas, setSchemas] = useState<ConfigSchemaItem[]>(
		() => readAdminConfigSchemaCache() ?? [],
	);
	const [templateVariableGroups, setTemplateVariableGroups] = useState<
		TemplateVariableGroup[]
	>(() => readAdminTemplateVariablesCache() ?? []);
	const [loading, setLoading] = useState(true);
	const [saving, setSaving] = useState(false);
	const [draftValues, setDraftValues] = useState<DraftValues>({});
	const [customVisibilityDrafts, setCustomVisibilityDrafts] = useState<
		Record<string, SystemConfigVisibility>
	>({});
	const [displayUnits, setDisplayUnits] = useState<
		Partial<Record<string, TimeDisplayUnitValue | SizeDisplayUnitValue>>
	>({});
	const [deletedCustomKeys, setDeletedCustomKeys] = useState<string[]>([]);
	const [newCustomRows, setNewCustomRows] = useState<NewCustomDraft[]>([]);
	const [expandedSubcategoryGroups, setExpandedSubcategoryGroups] = useState<
		Record<string, boolean>
	>({});
	const [expandedTemplateGroups, setExpandedTemplateGroups] = useState<
		Record<string, boolean>
	>({});
	const [testEmailDialogOpen, setTestEmailDialogOpen] = useState(false);
	const [activeTemplateVariableGroupCode, setActiveTemplateVariableGroupCode] =
		useState<string | null>(null);
	const [testEmailTarget, setTestEmailTarget] = useState("");
	const [sendingTestEmail, setSendingTestEmail] = useState(false);

	const resetEditableState = useCallback((nextConfigs: SystemConfig[]) => {
		setDraftValues(buildDraftValues(nextConfigs));
		setCustomVisibilityDrafts({});
		setDisplayUnits({});
		setDeletedCustomKeys([]);
		setNewCustomRows([]);
	}, []);

	const openTestEmailDialog = useCallback(() => {
		setTestEmailTarget(currentUserEmail);
		setTestEmailDialogOpen(true);
	}, [currentUserEmail]);

	const handleSendTestEmail = useCallback(async () => {
		setSendingTestEmail(true);
		try {
			const targetEmail = testEmailTarget.trim() || currentUserEmail.trim();
			await adminConfigService.sendTestEmail(targetEmail || undefined);
			toast.success(
				targetEmail
					? t("mail_test_email_sent", { email: targetEmail })
					: t("mail_test_email_sent_default"),
			);
			setTestEmailDialogOpen(false);
		} catch (error) {
			handleApiError(error);
		} finally {
			setSendingTestEmail(false);
		}
	}, [currentUserEmail, t, testEmailTarget]);

	const handleBuildWopiDiscoveryPreviewConfig = useCallback(
		async ({
			discoveryUrl,
			value,
		}: {
			discoveryUrl: string;
			value: string;
		}) => {
			try {
				const response = await adminConfigService.action(
					PREVIEW_APPS_CONFIG_KEY,
					{
						action: "build_wopi_discovery_preview_config",
						discovery_url: discoveryUrl,
						value,
					},
				);
				toast.success(t("preview_apps_wopi_discovery_success"));
				return response.value ?? value;
			} catch (error) {
				handleApiError(error);
				throw error;
			}
		},
		[t],
	);

	const handleTestVipsCliCommand = useCallback(async (value: string) => {
		try {
			const response = await adminConfigService.action(
				MEDIA_PROCESSING_CONFIG_KEY,
				{
					action: "test_vips_cli" satisfies ConfigActionType,
					value,
				},
			);
			toast.success(response.message);
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	}, []);

	const handleTestFfmpegCliCommand = useCallback(async (value: string) => {
		try {
			const response = await adminConfigService.action(
				MEDIA_PROCESSING_CONFIG_KEY,
				{
					action: "test_ffmpeg_cli" satisfies ConfigActionType,
					value,
				},
			);
			toast.success(response.message);
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	}, []);

	const handleTestFfprobeCliCommand = useCallback(async (value: string) => {
		try {
			const response = await adminConfigService.action(
				MEDIA_PROCESSING_CONFIG_KEY,
				{
					action: "test_ffprobe_cli" satisfies ConfigActionType,
					value,
				},
			);
			toast.success(response.message);
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	}, []);

	const handleTestAria2Rpc = useCallback(
		async (value: string) => {
			try {
				const draftConfigMap = new Map(
					configs.map((config) => [config.key, config] as const),
				);
				const draftValuesForAction = Object.fromEntries(
					OFFLINE_DOWNLOAD_ACTION_DRAFT_KEYS.flatMap((key) => {
						const draftValue =
							key === OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY
								? value
								: draftValues[key];
						if (
							draftValue == null ||
							!draftValueChangedForKey(draftConfigMap, key, draftValue)
						) {
							return [];
						}
						return [[key, configValueToString(draftValue)]];
					}),
				);
				const response = await adminConfigService.action(
					OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
					{
						action: "test_aria2_rpc" satisfies ConfigActionType,
						draft_values: draftValuesForAction,
						value,
					},
				);
				toast.success(response.message);
			} catch (error) {
				handleApiError(error);
				throw error;
			}
		},
		[configs, draftValues],
	);

	const load = useCallback(
		async (options?: { showLoading?: boolean }) => {
			const showLoading = options?.showLoading ?? true;

			try {
				if (showLoading) {
					setLoading(true);
				}
				const [cfgs, schemaList, nextTemplateVariableGroups] =
					await Promise.all([
						loadAllSystemConfigs(),
						loadAdminConfigSchema(),
						loadAdminTemplateVariables().catch((error) => {
							handleApiError(error);
							return [];
						}),
					]);
				setConfigs(cfgs);
				resetEditableState(cfgs);
				setSchemas(schemaList);
				setTemplateVariableGroups(nextTemplateVariableGroups);
			} catch (error) {
				handleApiError(error);
			} finally {
				if (showLoading) {
					setLoading(false);
				}
			}
		},
		[resetEditableState],
	);

	useEffect(() => {
		void load();
	}, [load]);

	const schemaMap = useMemo(() => {
		const map = new Map<string, ConfigSchemaItem>();
		for (const schema of schemas) {
			map.set(schema.key, schema);
		}
		return map;
	}, [schemas]);

	const resolveSchemaTranslation = useCallback(
		(translationKey: string | undefined, fallback?: string) => {
			if (!translationKey) {
				return fallback;
			}

			const translated = t(translationKey);
			return translated === translationKey ? fallback : translated;
		},
		[t],
	);

	const getSystemConfigLabel = useCallback(
		(config: SystemConfig) => {
			const schema = schemaMap.get(config.key);
			return (
				resolveSchemaTranslation(schema?.label_i18n_key, config.key) ??
				config.key
			);
		},
		[resolveSchemaTranslation, schemaMap],
	);

	const getSystemConfigDescription = useCallback(
		(config: SystemConfig) => {
			const schema = schemaMap.get(config.key);
			const fallback = schema?.description || getConfigDescription(config);
			return resolveSchemaTranslation(schema?.description_i18n_key, fallback);
		},
		[resolveSchemaTranslation, schemaMap],
	);

	const getSystemConfigSchema = useCallback(
		(config: SystemConfig) => schemaMap.get(config.key),
		[schemaMap],
	);

	const mailTemplateVariableGroups = useMemo(
		() =>
			[...templateVariableGroups]
				.filter((group) => group.category === "mail.template")
				.sort(
					(left, right) =>
						getMailTemplateGroupOrderIndex(left.template_code) -
							getMailTemplateGroupOrderIndex(right.template_code) ||
						left.template_code.localeCompare(right.template_code),
				),
		[templateVariableGroups],
	);

	const activeTemplateVariableGroup = useMemo(
		() =>
			activeTemplateVariableGroupCode
				? (mailTemplateVariableGroups.find(
						(group) => group.template_code === activeTemplateVariableGroupCode,
					) ?? null)
				: null,
		[activeTemplateVariableGroupCode, mailTemplateVariableGroups],
	);

	const getTemplateVariableGroupLabel = useCallback(
		(group: TemplateVariableGroup) =>
			resolveSchemaTranslation(
				group.label_i18n_key,
				formatSubcategoryLabel(group.template_code),
			) ?? formatSubcategoryLabel(group.template_code),
		[resolveSchemaTranslation],
	);

	const getTemplateVariableLabel = useCallback(
		(variable: TemplateVariableGroup["variables"][number]) =>
			resolveSchemaTranslation(variable.label_i18n_key, variable.token) ??
			variable.token,
		[resolveSchemaTranslation],
	);

	const getTemplateVariableDescription = useCallback(
		(variable: TemplateVariableGroup["variables"][number]) =>
			resolveSchemaTranslation(variable.description_i18n_key),
		[resolveSchemaTranslation],
	);

	const openTemplateVariablesDialog = useCallback((config: SystemConfig) => {
		setActiveTemplateVariableGroupCode(getMailTemplateGroupId(config.key));
	}, []);

	const systemConfigs = useMemo(() => {
		const hasOfflineDownloadRegistry = configs.some(
			(config) =>
				isSystemConfigSource(config.source) &&
				config.key === OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
		);

		return configs
			.filter(
				(config) =>
					isSystemConfigSource(config.source) &&
					!(
						hasOfflineDownloadRegistry &&
						config.key === LEGACY_OFFLINE_DOWNLOAD_ENGINE_KEY
					),
			)
			.sort(sortConfigsByKey);
	}, [configs]);

	const customConfigs = useMemo(
		() =>
			configs
				.filter((config) => !isSystemConfigSource(config.source))
				.sort(sortConfigsByKey),
		[configs],
	);

	const systemGroups = useMemo(() => {
		const groups: Record<string, SystemConfig[]> = {};

		for (const config of systemConfigs) {
			const category = normalizeCategory(config.category);
			if (!groups[category]) {
				groups[category] = [];
			}
			groups[category].push(config);
		}

		return groups;
	}, [systemConfigs]);

	const systemCategories = useMemo(
		() => Object.keys(systemGroups),
		[systemGroups],
	);

	const systemSubcategoryGroups = useMemo(() => {
		const groups: Record<string, SystemSubcategoryGroup[]> = {};

		for (const category of systemCategories) {
			const grouped = new Map<string, SystemSubcategoryGroup>();

			for (const config of systemGroups[category] ?? []) {
				const subcategory = normalizeSubcategory(config.category);
				const groupKey = getSubcategoryGroupKey(category, subcategory);
				const existingGroup = grouped.get(groupKey);
				if (existingGroup) {
					existingGroup.configs.push(config);
					continue;
				}

				grouped.set(groupKey, {
					category,
					subcategory,
					configs: [config],
				});
			}

			groups[category] = Array.from(grouped.values()).sort((left, right) => {
				if (!left.subcategory && !right.subcategory) return 0;
				if (!left.subcategory) return -1;
				if (!right.subcategory) return 1;
				return left.subcategory.localeCompare(right.subcategory);
			});
		}

		return groups;
	}, [systemCategories, systemGroups]);

	const deletedCustomKeySet = useMemo(
		() => new Set(deletedCustomKeys),
		[deletedCustomKeys],
	);

	const visibleCustomConfigs = useMemo(
		() =>
			customConfigs.filter((config) => !deletedCustomKeySet.has(config.key)),
		[customConfigs, deletedCustomKeySet],
	);

	const deletedCustomConfigs = useMemo(
		() => customConfigs.filter((config) => deletedCustomKeySet.has(config.key)),
		[customConfigs, deletedCustomKeySet],
	);

	const activeNewCustomRows = useMemo(
		() =>
			newCustomRows.filter(
				(row) => row.key.trim().length > 0 || row.value.trim().length > 0,
			),
		[newCustomRows],
	);

	const configsByKey = useMemo(
		() => new Map(configs.map((config) => [config.key, config] as const)),
		[configs],
	);

	const newCustomRowErrors = useMemo(() => {
		const errors = new Map<string, string>();
		const keyCounts = new Map<string, number>();

		for (const row of activeNewCustomRows) {
			const trimmedKey = row.key.trim();
			if (!trimmedKey) continue;
			keyCounts.set(trimmedKey, (keyCounts.get(trimmedKey) ?? 0) + 1);
		}

		const existingKeys = new Set(
			visibleCustomConfigs.map((config) => config.key),
		);

		for (const row of activeNewCustomRows) {
			const trimmedKey = row.key.trim();
			if (!trimmedKey) {
				errors.set(row.id, t("custom_config_key_required"));
				continue;
			}
			if (
				existingKeys.has(trimmedKey) ||
				(keyCounts.get(trimmedKey) ?? 0) > 1
			) {
				errors.set(row.id, t("custom_config_key_duplicate"));
			}
		}

		return errors;
	}, [activeNewCustomRows, t, visibleCustomConfigs]);

	const changedExistingConfigs = useMemo(
		() =>
			configs.filter((config) => {
				if (deletedCustomKeySet.has(config.key)) {
					return false;
				}
				const valueChanged = configDraftValueChanged(
					config,
					draftValues[config.key] ?? (config.value as DraftValues[string]),
				);
				const visibilityChanged =
					!isSystemConfigSource(config.source) &&
					Object.hasOwn(customVisibilityDrafts, config.key) &&
					customVisibilityDrafts[config.key] !==
						((config.visibility as SystemConfigVisibility | undefined) ??
							"private");
				return valueChanged || visibilityChanged;
			}),
		[configs, customVisibilityDrafts, deletedCustomKeySet, draftValues],
	);

	const previewAppsValidationIssues = useMemo(() => {
		const config = configs.find((item) => item.key === PREVIEW_APPS_CONFIG_KEY);
		if (!config) {
			return [];
		}

		return getPreviewAppsConfigIssuesFromString(
			configValueToString(
				draftValues[config.key] ?? (config.value as DraftValues[string]),
			),
		);
	}, [configs, draftValues]);

	const mediaProcessingValidationIssues = useMemo(() => {
		const config = configs.find(
			(item) => item.key === MEDIA_PROCESSING_CONFIG_KEY,
		);
		if (!config) {
			return [];
		}

		return getMediaProcessingConfigIssuesFromString(
			configValueToString(
				draftValues[config.key] ?? (config.value as DraftValues[string]),
			),
		);
	}, [configs, draftValues]);

	const offlineDownloadEngineValidationIssues = useMemo(() => {
		const config = configs.find(
			(item) => item.key === OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY,
		);
		if (!config) {
			return [];
		}

		return getOfflineDownloadEngineConfigIssuesFromString(
			configValueToString(
				draftValues[config.key] ?? (config.value as DraftValues[string]),
			),
		);
	}, [configs, draftValues]);

	const getDraftValueForKey = useCallback(
		(key: string) => {
			const config = configsByKey.get(key);
			return effectiveDraftValueForConfig(
				config,
				draftValues[key] ?? (config?.value as DraftValues[string]),
			);
		},
		[configsByKey, draftValues],
	);

	const configValidationErrors = useMemo(() => {
		const errors = new Map<string, string>();
		const allowedOrigins = configValueToString(
			getDraftValueForKey(CORS_ALLOWED_ORIGINS_KEY),
		).trim();
		const allowCredentials =
			configValueToString(
				getDraftValueForKey(CORS_ALLOW_CREDENTIALS_KEY),
			).trim() === "true";
		const emailCodeLoginEnabled =
			configValueToString(
				getDraftValueForKey(AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY),
			).trim() === "true";

		if (allowCredentials && allowedOrigins === "*") {
			const message = t("cors_wildcard_credentials_validation_error");
			errors.set(CORS_ALLOWED_ORIGINS_KEY, message);
			errors.set(CORS_ALLOW_CREDENTIALS_KEY, message);
		}

		if (
			emailCodeLoginEnabled &&
			!isMailDeliveryConfigReady(getDraftValueForKey)
		) {
			errors.set(
				AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
				t("email_code_mfa_mail_config_required"),
			);
		}

		for (const config of configs) {
			if (!isStringEnumSetType(getConfigValueType(config))) {
				continue;
			}
			const schema = schemaMap.get(config.key);
			const options = schema?.options ?? [];
			const allowedValues = new Set(options.map((option) => option.value));
			const selectedValues = configValueToStringArray(
				getDraftValueForKey(config.key),
			);
			const seen = new Set<string>();
			const invalidValue = selectedValues.find((value) => {
				if (seen.has(value)) {
					return true;
				}
				seen.add(value);
				return !allowedValues.has(value);
			});
			if (invalidValue) {
				errors.set(
					config.key,
					t("settings_enum_set_invalid_value", { value: invalidValue }),
				);
			}
		}

		return errors;
	}, [configs, getDraftValueForKey, schemaMap, t]);

	const changedCount =
		changedExistingConfigs.length +
		deletedCustomConfigs.length +
		activeNewCustomRows.length;
	const hasValidationError =
		newCustomRowErrors.size > 0 ||
		previewAppsValidationIssues.length > 0 ||
		mediaProcessingValidationIssues.length > 0 ||
		offlineDownloadEngineValidationIssues.length > 0 ||
		configValidationErrors.size > 0;
	const hasUnsavedChanges = changedCount > 0;
	const hasAnyConfig = configs.length > 0;
	const validationMessage =
		configValidationErrors.values().next().value ??
		(mediaProcessingValidationIssues.length > 0
			? t("media_processing_validation_error")
			: offlineDownloadEngineValidationIssues.length > 0
				? t("offline_download_engine_validation_error")
				: previewAppsValidationIssues.length > 0
					? t("preview_apps_validation_error")
					: newCustomRowErrors.size > 0
						? t("custom_config_validation_error")
						: undefined);

	const getDraftValue = useCallback(
		(config: SystemConfig) =>
			draftValues[config.key] ?? (config.value as DraftValues[string]),
		[draftValues],
	);

	const getCustomVisibilityDraft = useCallback(
		(config: SystemConfig) =>
			customVisibilityDrafts[config.key] ??
			(config.visibility as SystemConfigVisibility | undefined) ??
			"private",
		[customVisibilityDrafts],
	);

	const getDraftValueByKey = useCallback(
		(key: string) => {
			if (Object.hasOwn(draftValues, key)) {
				return draftValues[key];
			}
			return configsByKey.get(key)?.value as DraftValues[string] | undefined;
		},
		[configsByKey, draftValues],
	);

	const updateDraftValue = useCallback(
		(key: string, value: DraftValues[string]) => {
			setDraftValues((previous) => {
				const next = { ...previous, [key]: value };
				const readNextValue = (lookupKey: string) => {
					const config = configsByKey.get(lookupKey);
					if (Object.hasOwn(next, lookupKey)) {
						return effectiveDraftValueForConfig(config, next[lookupKey]);
					}
					return config?.value as DraftValues[string] | undefined;
				};

				if (
					MAIL_DELIVERY_CONFIG_KEYS.has(key) &&
					configsByKey.has(AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY) &&
					!isMailDeliveryConfigReady(readNextValue)
				) {
					next[AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY] = "false";
				}

				return next;
			});
		},
		[configsByKey],
	);

	const updateCustomVisibilityDraft = useCallback(
		(key: string, visibility: SystemConfigVisibility) => {
			setCustomVisibilityDrafts((previous) => ({
				...previous,
				[key]: visibility,
			}));
		},
		[],
	);

	const toggleSubcategoryGroup = useCallback(
		(groupKey: string, nextExpanded: boolean) => {
			setExpandedSubcategoryGroups((previous) => ({
				...previous,
				[groupKey]: nextExpanded,
			}));
		},
		[],
	);

	const toggleTemplateGroup = useCallback(
		(groupKey: string, nextExpanded: boolean) => {
			setExpandedTemplateGroups((previous) => ({
				...previous,
				[groupKey]: nextExpanded,
			}));
		},
		[],
	);

	const discardChanges = useCallback(() => {
		resetEditableState(configs);
	}, [configs, resetEditableState]);

	const appendCustomDraftRow = useCallback(() => {
		customDraftIdRef.current += 1;
		setNewCustomRows((previous) => [
			...previous,
			{
				id: `new-custom-${customDraftIdRef.current}`,
				key: "",
				value: "",
				visibility: "private",
			},
		]);
	}, []);

	const updateNewCustomRow = useCallback(
		(id: string, field: keyof Omit<NewCustomDraft, "id">, value: string) => {
			setNewCustomRows((previous) =>
				previous.map((row) =>
					row.id === id ? { ...row, [field]: value } : row,
				),
			);
		},
		[],
	);

	const removeNewCustomRow = useCallback((id: string) => {
		setNewCustomRows((previous) => previous.filter((row) => row.id !== id));
	}, []);

	const markCustomDeleted = useCallback((key: string) => {
		setDeletedCustomKeys((previous) =>
			previous.includes(key) ? previous : [...previous, key],
		);
	}, []);

	const restoreDeletedCustom = useCallback((key: string) => {
		setDeletedCustomKeys((previous) => previous.filter((item) => item !== key));
	}, []);

	const handleSaveAll = useCallback(async () => {
		if (saving || hasValidationError || !hasUnsavedChanges) {
			return;
		}

		try {
			setSaving(true);
			const previewAppsChanged = changedExistingConfigs.some(
				(config) => config.key === PREVIEW_APPS_CONFIG_KEY,
			);
			const mediaProcessingChanged = changedExistingConfigs.some(
				(config) => config.key === MEDIA_PROCESSING_CONFIG_KEY,
			);
			const mediaDataSupportChanged = changedExistingConfigs.some((config) =>
				MEDIA_DATA_SUPPORT_CONFIG_KEYS.has(config.key),
			);
			const frontendConfigChanged = changedExistingConfigs.some((config) =>
				FRONTEND_CONFIG_KEYS.has(config.key),
			);
			const nextConfigsByKey = new Map(
				configs.map((config) => [config.key, config] as const),
			);

			for (const config of deletedCustomConfigs) {
				await adminConfigService.delete(config.key);
				nextConfigsByKey.delete(config.key);
			}

			const orderedChangedConfigs = [...changedExistingConfigs].sort(
				(left, right) => {
					const priority = (config: SystemConfig) =>
						config.key === AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY ? 1 : 0;
					return priority(left) - priority(right);
				},
			);

			for (const config of orderedChangedConfigs) {
				const nextValue = getDraftValue(config);
				const serializedValue = serializeConfigDraftValue(nextValue);
				const savedConfig = isSystemConfigSource(config.source)
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

			const nextConfigs = Array.from(nextConfigsByKey.values());
			setConfigs(nextConfigs);
			resetEditableState(nextConfigs);
			const nextPublicSiteUrl =
				nextConfigsByKey.get(PUBLIC_SITE_URL_KEY)?.value;
			if (Array.isArray(nextPublicSiteUrl)) {
				onPublicSiteUrlChanged(nextPublicSiteUrl);
			}
			if (frontendConfigChanged) {
				useFrontendConfigStore.getState().invalidate();
				void useFrontendConfigStore.getState().load({ force: true });
			}
			if (previewAppsChanged) {
				usePreviewAppStore.getState().invalidate();
				void usePreviewAppStore.getState().load({ force: true });
			}
			if (mediaProcessingChanged) {
				useThumbnailSupportStore.getState().invalidate();
				void useThumbnailSupportStore.getState().load({ force: true });
			}
			if (mediaDataSupportChanged) {
				useMediaDataSupportStore.getState().invalidate();
				void useMediaDataSupportStore.getState().load({ force: true });
			}
			toast.success(t("settings_saved"));
		} catch (error) {
			handleApiError(error);
			try {
				await load({ showLoading: false });
			} catch (reloadError) {
				handleApiError(reloadError);
			}
		} finally {
			setSaving(false);
		}
	}, [
		activeNewCustomRows,
		changedExistingConfigs,
		configs,
		deletedCustomConfigs,
		getDraftValue,
		getCustomVisibilityDraft,
		hasUnsavedChanges,
		hasValidationError,
		load,
		onPublicSiteUrlChanged,
		resetEditableState,
		saving,
		t,
	]);

	return {
		activeTemplateVariableGroup,
		activeTemplateVariableGroupCode,
		appendCustomDraftRow,
		changedCount,
		configValidationErrors,
		configs,
		deletedCustomConfigs,
		displayUnits,
		discardChanges,
		expandedSubcategoryGroups,
		expandedTemplateGroups,
		getDraftValue,
		getDraftValueByKey,
		getCustomVisibilityDraft,
		getSystemConfigDescription,
		getSystemConfigLabel,
		getSystemConfigSchema,
		getTemplateVariableDescription,
		getTemplateVariableGroupLabel,
		getTemplateVariableLabel,
		handleBuildWopiDiscoveryPreviewConfig,
		handleTestAria2Rpc,
		handleTestFfmpegCliCommand,
		handleTestFfprobeCliCommand,
		handleSaveAll,
		handleSendTestEmail,
		handleTestVipsCliCommand,
		hasAnyConfig,
		hasUnsavedChanges,
		hasValidationError,
		loading,
		markCustomDeleted,
		newCustomRowErrors,
		newCustomRows,
		openTemplateVariablesDialog,
		openTestEmailDialog,
		previewAppsValidationIssues,
		removeNewCustomRow,
		restoreDeletedCustom,
		saving,
		setActiveTemplateVariableGroupCode,
		setDisplayUnits,
		setTestEmailDialogOpen,
		setTestEmailTarget,
		sendingTestEmail,
		systemCategories,
		systemGroups,
		systemSubcategoryGroups,
		testEmailDialogOpen,
		testEmailTarget,
		toggleSubcategoryGroup,
		toggleTemplateGroup,
		updateCustomVisibilityDraft,
		updateDraftValue,
		updateNewCustomRow,
		validationMessage,
		visibleCustomConfigs,
	};
}
