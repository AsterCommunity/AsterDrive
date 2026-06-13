import { useRef, useState } from "react";
import { AdminNumberUnitInput } from "@/components/admin/AdminNumberUnitInput";
import { MediaProcessingConfigEditor } from "@/components/admin/MediaProcessingConfigEditor";
import { MEDIA_PROCESSING_CONFIG_KEY } from "@/components/admin/mediaProcessingConfigEditorShared";
import { OfflineDownloadEngineRegistryEditor } from "@/components/admin/OfflineDownloadEngineRegistryEditor";
import { OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY } from "@/components/admin/offlineDownloadEngineRegistryShared";
import { PreviewAppsConfigEditor } from "@/components/admin/PreviewAppsConfigEditor";
import { PREVIEW_APPS_CONFIG_KEY } from "@/components/admin/previewAppsConfigEditorShared";
import { useAdminSettingsCategoryContent } from "@/components/admin/settings/AdminSettingsCategoryContentContext";
import {
	ConfigCodeEditor,
	type ConfigDraftValue,
	configDraftValueChanged,
	configValueToString,
	configValueToStringArray,
	formatDisplayValue,
	getAvailableDisplayUnits,
	getBrandingAssetPreviewAppearance,
	getConfigDescription,
	getConfigEditorLanguage,
	getConfigIsSensitive,
	getConfigRequiresRestart,
	getConfigValueType,
	getPreferredDisplayUnit,
	getTimeConfigBaseUnit,
	isBooleanType,
	isBrandingAssetConfig,
	isMultilineType,
	isNumberType,
	isRedactedConfigValue,
	isSizeConfig,
	isStringArrayType,
	isStringEnumSetType,
	type NewCustomDraft,
	parseWholeNumber,
	SIZE_DISPLAY_UNITS,
	type SizeDisplayUnitValue,
	TIME_DISPLAY_UNITS,
	type TimeDisplayUnitValue,
	UrlAssetPreview,
} from "@/components/admin/settings/adminSettingsContentShared";
import { isMailDeliveryConfigReady } from "@/components/admin/settings/mailDeliveryConfigReady";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { convertNumberUnitValueToBaseUnit } from "@/lib/numberUnit";
import { normalizePublicSiteUrl } from "@/lib/publicSiteUrl";
import { cn } from "@/lib/utils";
import type {
	ConfigSchemaOption,
	SystemConfig,
	SystemConfigVisibility,
} from "@/types/api";

const PUBLIC_SITE_URL_KEY = "public_site_url";
const EMAIL_CODE_LOGIN_ENABLED_CONFIG_KEY = "auth_email_code_login_enabled";
const AUTH_LOCAL_EMAIL_ALLOWLIST_KEY = "auth_local_email_allowlist";
const AUTH_LOCAL_EMAIL_BLOCKLIST_KEY = "auth_local_email_blocklist";
type ScaledDisplayUnitValue = TimeDisplayUnitValue | SizeDisplayUnitValue;
const CUSTOM_VISIBILITY_OPTIONS: SystemConfigVisibility[] = [
	"private",
	"public",
	"authenticated",
];

function resolveSettingTranslation(
	t: (key: string, options?: Record<string, unknown>) => string,
	key: string,
	fallback: string,
) {
	const translated = t(key);
	return translated === key ? fallback : translated;
}

function formatEnumGroupLabel(
	t: (key: string, options?: Record<string, unknown>) => string,
	group: string,
) {
	return resolveSettingTranslation(
		t,
		`settings_enum_group_${group}`,
		group
			.split(/[._-]+/)
			.filter(Boolean)
			.map((part) => part[0]?.toUpperCase() + part.slice(1))
			.join(" "),
	);
}

function formatEnumOptionLabel(
	t: (key: string, options?: Record<string, unknown>) => string,
	option: ConfigSchemaOption,
) {
	return resolveSettingTranslation(
		t,
		option.label_i18n_key,
		option.value
			.split("_")
			.filter(Boolean)
			.map((part) => part[0]?.toUpperCase() + part.slice(1))
			.join(" "),
	);
}

function getEnumOptionGroup(option: ConfigSchemaOption) {
	return option.group || "other";
}

function isScaledDisplayUnitValue(
	value: string,
	units: ReadonlyArray<{ value: string }>,
): value is ScaledDisplayUnitValue {
	return units.some((unit) => unit.value === value);
}

function SettingDescriptionHelp({
	description,
	label,
}: {
	description?: string;
	label: string;
}) {
	if (!description) {
		return null;
	}

	return (
		<TooltipProvider delay={0}>
			<Tooltip>
				<TooltipTrigger
					type="button"
					aria-label={label}
					className="inline-flex size-6 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-accent/55 hover:text-foreground focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/35"
				>
					<Icon name="Question" className="size-5" aria-hidden="true" />
				</TooltipTrigger>
				<TooltipContent
					side="top"
					align="start"
					className="max-w-[min(22rem,calc(100vw-2rem))] whitespace-normal break-words leading-5"
				>
					{description}
				</TooltipContent>
			</Tooltip>
		</TooltipProvider>
	);
}

function FieldMeta({ config }: { config: SystemConfig }) {
	const {
		getDraftValue,
		getSystemConfigDescription,
		getSystemConfigLabel,
		navigateToMailSettings,
		openTemplateVariablesDialog,
		t,
	} = useAdminSettingsCategoryContent();
	const draftChanged = configDraftValueChanged(config, getDraftValue(config));
	const requiresRestart = getConfigRequiresRestart(config);
	const configLabel = getSystemConfigLabel(config);
	const configDescription = getSystemConfigDescription(config);
	const showRawKey = configLabel !== config.key;
	const showTemplateVariableLink =
		config.category === "mail.template" && config.key.endsWith("_html");
	const showEmailCodeMailSettingsLink =
		config.key === EMAIL_CODE_LOGIN_ENABLED_CONFIG_KEY;

	return (
		<div className="space-y-1">
			<div className="flex flex-wrap items-center gap-2">
				<p
					className={
						showRawKey
							? "break-words text-sm font-medium"
							: "break-all font-mono text-sm font-medium"
					}
				>
					{configLabel}
				</p>
				<SettingDescriptionHelp
					description={configDescription}
					label={t("settings_config_description_help", {
						label: configLabel,
					})}
				/>
				{draftChanged ? (
					<span className="text-xs font-medium text-primary">
						{t("settings_status_unsaved")}
					</span>
				) : null}
				{requiresRestart ? (
					<span className="text-xs text-muted-foreground">
						{t("requires_restart")}
					</span>
				) : null}
			</div>
			{showTemplateVariableLink ? (
				<button
					type="button"
					className="w-fit text-sm text-primary underline-offset-4 transition-colors hover:text-primary/80 hover:underline"
					onClick={() => openTemplateVariablesDialog(config)}
				>
					{t("mail_template_variable_link")}
				</button>
			) : null}
			{showEmailCodeMailSettingsLink ? (
				<button
					type="button"
					className="inline-flex w-fit items-center gap-1.5 text-sm font-medium text-primary underline-offset-4 transition-colors hover:text-primary/80 hover:underline"
					onClick={navigateToMailSettings}
				>
					<Icon name="EnvelopeSimple" className="size-4" />
					{t("email_code_mfa_mail_settings_link")}
					<Icon name="ArrowRight" className="size-3.5" />
				</button>
			) : null}
		</div>
	);
}

function StringArrayConfigControl({
	config,
	draftValue,
	fullWidth,
	hasError,
}: {
	config: SystemConfig;
	draftValue: string[];
	fullWidth?: boolean;
	hasError?: boolean;
}) {
	const { t, updateDraftValue } = useAdminSettingsCategoryContent();
	const isPublicSiteUrl = config.key === PUBLIC_SITE_URL_KEY;
	const isLocalEmailPolicy =
		config.key === AUTH_LOCAL_EMAIL_ALLOWLIST_KEY ||
		config.key === AUTH_LOCAL_EMAIL_BLOCKLIST_KEY;
	const itemLabel = t(
		isLocalEmailPolicy
			? "local_email_policy_item_label"
			: isPublicSiteUrl
				? "public_site_url_origin_label"
				: "settings_string_array_item_label",
	);
	const addLabel = t(
		isLocalEmailPolicy
			? "local_email_policy_add_item"
			: isPublicSiteUrl
				? "public_site_url_add_origin"
				: "settings_string_array_add_item",
	);
	const addCurrentOriginLabel = t("public_site_url_add_current_origin");
	const removeLabel = t(
		isLocalEmailPolicy
			? "local_email_policy_remove_item"
			: isPublicSiteUrl
				? "public_site_url_remove_origin"
				: "settings_string_array_remove_item",
	);
	const primaryLabel = isPublicSiteUrl
		? t("public_site_url_primary_origin")
		: null;
	const placeholder = isPublicSiteUrl
		? "https://drive.example.com"
		: isLocalEmailPolicy
			? t("local_email_policy_placeholder")
			: t("config_value");
	const rows = draftValue.length > 0 ? draftValue : [""];
	const nextRowIdRef = useRef(0);
	const rowIdsRef = useRef<string[]>([]);
	const createRowId = () => {
		const rowId = `${config.key}-string-array-row-${nextRowIdRef.current}`;
		nextRowIdRef.current += 1;
		return rowId;
	};
	while (rowIdsRef.current.length < rows.length) {
		rowIdsRef.current.push(createRowId());
	}
	if (rowIdsRef.current.length > rows.length) {
		rowIdsRef.current = rowIdsRef.current.slice(0, rows.length);
	}
	const rowItems = rows.map((row, index) => {
		return {
			key: rowIdsRef.current[index],
			value: row,
		};
	});
	const currentOrigin =
		isPublicSiteUrl && typeof window !== "undefined"
			? normalizePublicSiteUrl(window.location.origin)
			: null;
	const showAddCurrentOrigin =
		Boolean(currentOrigin) &&
		!rows.some((row) => normalizePublicSiteUrl(row) === currentOrigin);

	const updateRows = (nextRows: string[]) => {
		updateDraftValue(
			config.key,
			nextRows.some((row) => row.trim()) ? nextRows : [],
		);
	};

	return (
		<div
			className={cn("space-y-2", fullWidth ? "w-full max-w-3xl" : "max-w-3xl")}
		>
			{showAddCurrentOrigin && currentOrigin ? (
				<Button
					type="button"
					variant="outline"
					size="sm"
					aria-label={addCurrentOriginLabel}
					title={addCurrentOriginLabel}
					onClick={() => {
						const emptyRowIndex = rows.findIndex((row) => !row.trim());
						if (emptyRowIndex >= 0) {
							const nextRows = [...rows];
							nextRows[emptyRowIndex] = currentOrigin;
							updateRows(nextRows);
							return;
						}
						rowIdsRef.current.push(createRowId());
						updateRows([...rows, currentOrigin]);
					}}
				>
					<Icon name="Plus" className="size-3.5" />
					{addCurrentOriginLabel}
				</Button>
			) : null}
			{rowItems.map((item, index) => {
				return (
					<div key={item.key} className="flex items-center gap-2">
						<Input
							type={isPublicSiteUrl ? "url" : "text"}
							inputMode={isPublicSiteUrl ? "url" : "text"}
							className="min-w-0 flex-1"
							value={item.value}
							aria-label={`${itemLabel} ${index + 1}`}
							aria-invalid={hasError ? true : undefined}
							onChange={(event) => {
								const nextRows = [...rows];
								nextRows[index] = event.target.value;
								updateRows(nextRows);
							}}
							placeholder={placeholder}
						/>
						{primaryLabel && index === 0 ? (
							<span className="shrink-0 text-xs font-medium text-primary">
								{primaryLabel}
							</span>
						) : null}
						<Button
							type="button"
							variant="outline"
							size="icon"
							aria-label={addLabel}
							title={addLabel}
							onClick={() => {
								const nextRows = [...rows];
								nextRows.splice(index + 1, 0, "");
								rowIdsRef.current.splice(index + 1, 0, createRowId());
								updateRows(nextRows);
							}}
						>
							<Icon name="Plus" className="size-4" />
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							aria-label={removeLabel}
							title={removeLabel}
							onClick={() => {
								if (rows.length <= 1) {
									updateRows([]);
									return;
								}
								rowIdsRef.current.splice(index, 1);
								updateRows(rows.filter((_, rowIndex) => rowIndex !== index));
							}}
						>
							<Icon name="Trash" className="size-4" />
						</Button>
					</div>
				);
			})}
		</div>
	);
}

function StringEnumSetConfigControl({
	config,
	draftValue,
	hasError,
}: {
	config: SystemConfig;
	draftValue: string[];
	hasError?: boolean;
}) {
	const { getSystemConfigSchema, t, updateDraftValue } =
		useAdminSettingsCategoryContent();
	const [query, setQuery] = useState("");
	const schema = getSystemConfigSchema(config);
	const options = schema?.options ?? [];
	const selectedValues = new Set(draftValue);
	const normalizedQuery = query.trim().toLocaleLowerCase();
	const optionLabels = options.map((option) => ({
		option,
		label: formatEnumOptionLabel(t, option),
	}));
	const filteredOptions = normalizedQuery
		? optionLabels.filter(
				({ label, option }) =>
					label.toLocaleLowerCase().includes(normalizedQuery) ||
					option.value.toLocaleLowerCase().includes(normalizedQuery) ||
					getEnumOptionGroup(option)
						.toLocaleLowerCase()
						.includes(normalizedQuery),
			)
		: optionLabels;
	const groupedOptions = filteredOptions.reduce(
		(groups, item) => {
			const group = getEnumOptionGroup(item.option);
			if (!groups[group]) {
				groups[group] = [];
			}
			groups[group].push(item);
			return groups;
		},
		{} as Record<string, Array<{ option: ConfigSchemaOption; label: string }>>,
	);
	const visibleValues = filteredOptions.map(({ option }) => option.value);
	const selectedVisibleCount = visibleValues.filter((value) =>
		selectedValues.has(value),
	).length;
	const selectedCount = draftValue.length;
	const totalCount = options.length;

	const setSelected = (values: string[]) => {
		// Preserve backend option order so diffs and saved JSON stay stable.
		const allowedValues = new Set(options.map((option) => option.value));
		const nextValues = values.filter((value, index) => {
			return allowedValues.has(value) && values.indexOf(value) === index;
		});
		updateDraftValue(config.key, nextValues);
	};

	const toggleValue = (value: string) => {
		if (selectedValues.has(value)) {
			setSelected(draftValue.filter((item) => item !== value));
			return;
		}
		// New selections are inserted by schema order, not click order.
		const orderedValues = options
			.filter(
				(option) => option.value === value || selectedValues.has(option.value),
			)
			.map((option) => option.value);
		setSelected(orderedValues);
	};

	const selectVisible = () => {
		const nextValues = new Set(draftValue);
		for (const value of visibleValues) {
			nextValues.add(value);
		}
		const orderedValues = options
			.filter((option) => nextValues.has(option.value))
			.map((option) => option.value);
		setSelected(orderedValues);
	};

	return (
		<div className="w-full max-w-5xl space-y-3">
			<div className="flex flex-col gap-2 sm:flex-row sm:items-center">
				<div className="relative min-w-0 flex-1">
					<Icon
						name="MagnifyingGlass"
						className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
					/>
					<Input
						className="pl-9"
						value={query}
						aria-invalid={hasError ? true : undefined}
						onChange={(event) => setQuery(event.target.value)}
						placeholder={t("settings_enum_set_search_placeholder")}
					/>
				</div>
				<div className="flex shrink-0 flex-wrap items-center gap-2">
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={selectVisible}
					>
						<Icon name="ListChecks" className="size-3.5" />
						{t("settings_enum_set_select_visible")}
					</Button>
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={() => setSelected(options.map((option) => option.value))}
					>
						<Icon name="Check" className="size-3.5" />
						{t("settings_enum_set_select_all")}
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={() => setSelected([])}
					>
						<Icon name="X" className="size-3.5" />
						{t("settings_enum_set_clear")}
					</Button>
				</div>
			</div>

			<div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
				<span>
					{t("settings_enum_set_selected_count", {
						count: selectedCount,
						total: totalCount,
					})}
				</span>
				{normalizedQuery ? (
					<span>
						{t("settings_enum_set_visible_count", {
							count: filteredOptions.length,
							selected: selectedVisibleCount,
						})}
					</span>
				) : null}
			</div>

			{options.length === 0 ? (
				<p className="text-sm text-muted-foreground">
					{t("settings_enum_set_no_options")}
				</p>
			) : filteredOptions.length === 0 ? (
				<p className="text-sm text-muted-foreground">
					{t("settings_enum_set_no_matches")}
				</p>
			) : (
				<div className="max-h-[34rem] overflow-y-auto rounded-md border">
					{Object.entries(groupedOptions).map(([group, items]) => {
						const selectedInGroup = items.filter(({ option }) =>
							selectedValues.has(option.value),
						).length;

						return (
							<section key={group} className="border-b last:border-b-0">
								<div className="sticky top-0 z-10 flex items-center justify-between gap-3 border-b bg-background/95 px-3 py-2 backdrop-blur">
									<p className="text-xs font-semibold uppercase tracking-normal text-muted-foreground">
										{formatEnumGroupLabel(t, group)}
									</p>
									<span className="text-xs text-muted-foreground">
										{selectedInGroup}/{items.length}
									</span>
								</div>
								<div className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3">
									{items.map(({ option, label }) => {
										const checked = selectedValues.has(option.value);
										return (
											<button
												key={option.value}
												type="button"
												className={cn(
													"flex min-h-11 items-center gap-2 border-b px-3 py-2 text-left text-sm transition-colors last:border-b-0 sm:border-r sm:[&:nth-child(2n)]:border-r-0 xl:[&:nth-child(2n)]:border-r xl:[&:nth-child(3n)]:border-r-0",
													checked
														? "bg-primary/5 text-foreground"
														: "hover:bg-muted/50",
												)}
												aria-pressed={checked}
												onClick={() => toggleValue(option.value)}
											>
												<span
													className={cn(
														"flex size-4 shrink-0 items-center justify-center rounded border",
														checked
															? "border-primary bg-primary text-primary-foreground"
															: "border-border bg-background",
													)}
													aria-hidden="true"
												>
													{checked ? (
														<Icon name="Check" className="size-3" />
													) : null}
												</span>
												<span className="min-w-0">
													<span className="block truncate">{label}</span>
													<span className="block truncate font-mono text-xs text-muted-foreground">
														{option.value}
													</span>
												</span>
											</button>
										);
									})}
								</div>
							</section>
						);
					})}
				</div>
			)}
		</div>
	);
}

function StringEnumConfigControl({
	config,
	draftValue,
	hasError,
}: {
	config: SystemConfig;
	draftValue: string;
	hasError?: boolean;
}) {
	const { getSystemConfigSchema, t, updateDraftValue } =
		useAdminSettingsCategoryContent();
	const options = getSystemConfigSchema(config)?.options ?? [];

	return (
		<Select
			items={options.map((option) => ({
				label: formatEnumOptionLabel(t, option),
				value: option.value,
			}))}
			value={draftValue}
			onValueChange={(value) => {
				if (value !== null) {
					updateDraftValue(config.key, value);
				}
			}}
		>
			<SelectTrigger
				id={config.key}
				width="fit"
				className="min-w-52"
				aria-invalid={hasError ? true : undefined}
				aria-label={getConfigDescription(config) || config.key}
			>
				<SelectValue />
			</SelectTrigger>
			<SelectContent>
				{options.map((option) => (
					<SelectItem key={option.value} value={option.value}>
						{formatEnumOptionLabel(t, option)}
					</SelectItem>
				))}
			</SelectContent>
		</Select>
	);
}

function ScaledNumberInputControl({
	config,
	draftValue,
	fullWidth,
	hasError,
	unitLabelKey,
	units,
}: {
	config: SystemConfig;
	draftValue: string;
	fullWidth?: boolean;
	hasError?: boolean;
	unitLabelKey: string;
	units: ReadonlyArray<{
		labelKey: string;
		multiplier: number;
		value: string;
	}>;
}) {
	const { displayUnits, setDisplayUnits, t, updateDraftValue } =
		useAdminSettingsCategoryContent();
	const hasInvalidDraftValue = Boolean(
		draftValue.trim() && parseWholeNumber(draftValue) === null,
	);

	const availableUnits = getAvailableDisplayUnits(units, draftValue);
	const preferredUnit = getPreferredDisplayUnit(units, draftValue);
	const selectedUnit =
		availableUnits.find((unit) => unit.value === displayUnits[config.key]) ??
		preferredUnit;
	const displayValue = formatDisplayValue(draftValue, selectedUnit);

	const updateFromDisplayValue = (value: string) => {
		const nextDisplayValue = value.trim();
		if (!nextDisplayValue) {
			const selectedUnitValue = selectedUnit.value;
			if (isScaledDisplayUnitValue(selectedUnitValue, availableUnits)) {
				setDisplayUnits((previous) => ({
					...previous,
					[config.key]: selectedUnitValue,
				}));
			}
			updateDraftValue(config.key, "");
			return;
		}
		const nextValue = convertNumberUnitValueToBaseUnit(
			nextDisplayValue,
			selectedUnit,
		);
		if (nextValue === null) {
			const selectedUnitValue = selectedUnit.value;
			if (isScaledDisplayUnitValue(selectedUnitValue, availableUnits)) {
				setDisplayUnits((previous) => ({
					...previous,
					[config.key]: selectedUnitValue,
				}));
			}
			updateDraftValue(config.key, nextDisplayValue);
			return;
		}

		updateDraftValue(config.key, String(nextValue));
	};

	return (
		<AdminNumberUnitInput
			value={displayValue}
			unit={selectedUnit.value}
			units={availableUnits}
			placeholder={t("config_value")}
			unitAriaLabel={t(unitLabelKey)}
			invalid={hasError || hasInvalidDraftValue}
			className={fullWidth ? "w-full max-w-2xl" : "max-w-2xl"}
			inputClassName="w-full sm:max-w-48"
			onValueChange={updateFromDisplayValue}
			onUnitChange={(value) => {
				if (!isScaledDisplayUnitValue(value, availableUnits)) {
					return;
				}
				setDisplayUnits((previous) => ({
					...previous,
					[config.key]: value,
				}));
			}}
		/>
	);
}

function ConfigInputControl({
	config,
	draftValue,
	fullWidth,
	hasError,
}: {
	config: SystemConfig;
	draftValue: ConfigDraftValue;
	fullWidth?: boolean;
	hasError?: boolean;
}) {
	const {
		editorTheme,
		getSystemConfigSchema,
		handleBuildWopiDiscoveryPreviewConfig,
		handleTestAria2Rpc,
		handleTestFfmpegCliCommand,
		handleTestFfprobeCliCommand,
		handleTestVipsCliCommand,
		t,
		updateDraftValue,
	} = useAdminSettingsCategoryContent();
	const valueType = getConfigValueType(config);
	const draftStringValue = configValueToString(draftValue);
	const isSensitive = getConfigIsSensitive(config);
	const sensitiveRedactedValue =
		isSensitive && isRedactedConfigValue(config.value);
	const inputPlaceholder = sensitiveRedactedValue
		? t("settings_sensitive_keep_placeholder")
		: t("config_value");
	const multiline = isMultilineType(valueType);
	const stringArray = isStringArrayType(valueType);
	const stringEnumSet = isStringEnumSetType(valueType);
	const schema = getSystemConfigSchema(config);
	const stringEnum =
		valueType === "string" && (schema?.options?.length ?? 0) > 0;
	const brandingPreviewAppearance = isBrandingAssetConfig(config)
		? getBrandingAssetPreviewAppearance(config)
		: null;

	if (brandingPreviewAppearance) {
		return (
			<div className="flex max-w-4xl items-end gap-3">
				<div className="w-full max-w-3xl">
					<Input
						type={
							isNumberType(valueType)
								? "number"
								: isSensitive
									? "password"
									: "text"
						}
						inputMode={isNumberType(valueType) ? "decimal" : "text"}
						value={draftStringValue}
						aria-invalid={hasError ? true : undefined}
						onChange={(event) =>
							updateDraftValue(config.key, event.target.value)
						}
						placeholder={inputPlaceholder}
					/>
				</div>
				<UrlAssetPreview
					url={draftStringValue}
					appearance={brandingPreviewAppearance}
				/>
			</div>
		);
	}

	if (config.key === PREVIEW_APPS_CONFIG_KEY) {
		return (
			<PreviewAppsConfigEditor
				onBuildWopiDiscoveryPreviewConfig={
					handleBuildWopiDiscoveryPreviewConfig
				}
				value={draftStringValue}
				onChange={(nextValue) => updateDraftValue(config.key, nextValue)}
			/>
		);
	}

	if (config.key === MEDIA_PROCESSING_CONFIG_KEY) {
		return (
			<MediaProcessingConfigEditor
				onTestFfmpegCliCommand={handleTestFfmpegCliCommand}
				onTestFfprobeCliCommand={handleTestFfprobeCliCommand}
				onTestVipsCliCommand={handleTestVipsCliCommand}
				value={draftStringValue}
				onChange={(nextValue) => updateDraftValue(config.key, nextValue)}
			/>
		);
	}

	if (config.key === OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY) {
		return (
			<OfflineDownloadEngineRegistryEditor
				onTestAria2Rpc={handleTestAria2Rpc}
				value={draftStringValue}
				onChange={(nextValue) => updateDraftValue(config.key, nextValue)}
			/>
		);
	}

	if (stringArray) {
		return (
			<StringArrayConfigControl
				config={config}
				draftValue={configValueToStringArray(draftValue)}
				fullWidth={fullWidth}
				hasError={hasError}
			/>
		);
	}

	if (stringEnumSet) {
		return (
			<StringEnumSetConfigControl
				config={config}
				draftValue={configValueToStringArray(draftValue)}
				hasError={hasError}
			/>
		);
	}

	if (stringEnum) {
		return (
			<StringEnumConfigControl
				config={config}
				draftValue={draftStringValue}
				hasError={hasError}
			/>
		);
	}

	if (multiline) {
		return (
			<ConfigCodeEditor
				language={getConfigEditorLanguage(config)}
				theme={editorTheme}
				value={draftStringValue}
				onChange={(value) => updateDraftValue(config.key, value)}
			/>
		);
	}

	const timeConfigBaseUnit = getTimeConfigBaseUnit(config);
	if (timeConfigBaseUnit) {
		return (
			<ScaledNumberInputControl
				config={config}
				draftValue={draftStringValue}
				fullWidth={fullWidth}
				hasError={hasError}
				unitLabelKey="settings_time_unit_label"
				units={TIME_DISPLAY_UNITS[timeConfigBaseUnit]}
			/>
		);
	}

	if (isSizeConfig(config)) {
		return (
			<ScaledNumberInputControl
				config={config}
				draftValue={draftStringValue}
				fullWidth={fullWidth}
				hasError={hasError}
				unitLabelKey="settings_size_unit_label"
				units={SIZE_DISPLAY_UNITS}
			/>
		);
	}

	return (
		<Input
			type={
				isNumberType(valueType) ? "number" : isSensitive ? "password" : "text"
			}
			inputMode={isNumberType(valueType) ? "decimal" : "text"}
			className={fullWidth ? "w-full max-w-2xl" : "max-w-2xl"}
			value={draftStringValue}
			aria-invalid={hasError ? true : undefined}
			onChange={(event) => updateDraftValue(config.key, event.target.value)}
			placeholder={inputPlaceholder}
		/>
	);
}

function CustomVisibilitySelect({
	id,
	value,
	onChange,
}: {
	id: string;
	value: SystemConfigVisibility;
	onChange: (value: SystemConfigVisibility) => void;
}) {
	const { t } = useAdminSettingsCategoryContent();

	return (
		<Select
			items={CUSTOM_VISIBILITY_OPTIONS.map((option) => ({
				label: t(`custom_config_visibility_${option}`),
				value: option,
			}))}
			value={value}
			onValueChange={(nextValue) =>
				onChange(nextValue as SystemConfigVisibility)
			}
		>
			<SelectTrigger
				id={id}
				width="fit"
				className="min-w-40"
				aria-label={t("custom_config_visibility")}
			>
				<SelectValue />
			</SelectTrigger>
			<SelectContent>
				{CUSTOM_VISIBILITY_OPTIONS.map((option) => (
					<SelectItem key={option} value={option}>
						{t(`custom_config_visibility_${option}`)}
					</SelectItem>
				))}
			</SelectContent>
		</Select>
	);
}

export function SystemConfigRow({ config }: { config: SystemConfig }) {
	const {
		configValidationErrors,
		getDraftValue,
		getDraftValueByKey,
		t,
		updateDraftValue,
	} = useAdminSettingsCategoryContent();
	const draftValue = getDraftValue(config);
	const draftStringValue = configValueToString(draftValue);
	const valueType = getConfigValueType(config);
	const error = configValidationErrors.get(config.key);
	const isEmailCodeLoginToggle =
		config.key === EMAIL_CODE_LOGIN_ENABLED_CONFIG_KEY;
	const emailCodeMailReady = isEmailCodeLoginToggle
		? isMailDeliveryConfigReady(getDraftValueByKey)
		: true;
	const emailCodeEnableBlocked =
		isEmailCodeLoginToggle &&
		!emailCodeMailReady &&
		draftStringValue !== "true";
	const emailCodeMailRequiredMessage =
		isEmailCodeLoginToggle && !emailCodeMailReady
			? t("email_code_mfa_mail_config_required")
			: null;

	return (
		<div className="space-y-3">
			<FieldMeta config={config} />
			{isBooleanType(valueType) ? (
				<div className="flex items-center gap-3 text-sm">
					<Switch
						id={config.key}
						aria-invalid={error ? true : undefined}
						checked={draftStringValue === "true"}
						disabled={emailCodeEnableBlocked}
						onCheckedChange={(checked) => {
							if (checked && isEmailCodeLoginToggle && !emailCodeMailReady) {
								return;
							}
							updateDraftValue(config.key, checked ? "true" : "false");
						}}
					/>
					<span>
						{draftStringValue === "true"
							? t("settings_value_on")
							: t("settings_value_off")}
					</span>
				</div>
			) : (
				<ConfigInputControl
					config={config}
					draftValue={draftValue}
					hasError={Boolean(error)}
				/>
			)}
			{error ? <p className="text-sm text-destructive">{error}</p> : null}
			{!error && emailCodeMailRequiredMessage ? (
				<p className="text-sm text-muted-foreground">
					{emailCodeMailRequiredMessage}
				</p>
			) : null}
		</div>
	);
}

export function CustomConfigRow({ config }: { config: SystemConfig }) {
	const {
		getCustomVisibilityDraft,
		getDraftValue,
		markCustomDeleted,
		t,
		updateCustomVisibilityDraft,
	} = useAdminSettingsCategoryContent();
	const draftValue = getDraftValue(config);
	const visibilityDraft = getCustomVisibilityDraft(config);
	const valueType = getConfigValueType(config);
	const draftChanged =
		configDraftValueChanged(config, draftValue) ||
		visibilityDraft !==
			((config.visibility as SystemConfigVisibility | undefined) ?? "private");
	const multiline = isMultilineType(valueType);
	const configDescription = getConfigDescription(config);

	return (
		<div className="space-y-3">
			<div className="space-y-1">
				<div className="flex flex-wrap items-center gap-2">
					<p className="break-all font-mono text-sm font-medium">
						{config.key}
					</p>
					<SettingDescriptionHelp
						description={configDescription}
						label={t("settings_config_description_help", {
							label: config.key,
						})}
					/>
					{draftChanged ? (
						<span className="text-xs font-medium text-primary">
							{t("settings_status_unsaved")}
						</span>
					) : null}
				</div>
			</div>

			<div
				className={
					multiline
						? "space-y-3"
						: "flex flex-col gap-3 xl:flex-row xl:items-center"
				}
			>
				<ConfigInputControl config={config} draftValue={draftValue} fullWidth />
				<div className="flex flex-wrap items-center gap-2">
					<CustomVisibilitySelect
						id={`${config.key}-visibility`}
						value={visibilityDraft}
						onChange={(visibility) =>
							updateCustomVisibilityDraft(config.key, visibility)
						}
					/>
					<Button
						variant="ghost"
						size="sm"
						className="justify-start text-destructive"
						onClick={() => markCustomDeleted(config.key)}
					>
						{t("core:delete")}
					</Button>
				</div>
			</div>
		</div>
	);
}

export function NewCustomRow({ row }: { row: NewCustomDraft }) {
	const { newCustomRowErrors, removeNewCustomRow, t, updateNewCustomRow } =
		useAdminSettingsCategoryContent();
	const error = newCustomRowErrors.get(row.id);

	return (
		<div className="space-y-3">
			<p className="text-sm font-medium text-muted-foreground">
				{t("custom_config_new_entry")}
			</p>
			<div className="flex max-w-4xl flex-col gap-3 lg:flex-row">
				<Input
					className="lg:max-w-sm"
					value={row.key}
					aria-invalid={error ? true : undefined}
					onChange={(event) =>
						updateNewCustomRow(row.id, "key", event.target.value)
					}
					placeholder={t("custom_config_key_placeholder")}
				/>
				<Input
					className="lg:max-w-xl"
					value={row.value}
					onChange={(event) =>
						updateNewCustomRow(row.id, "value", event.target.value)
					}
					placeholder={t("config_value")}
				/>
				<CustomVisibilitySelect
					id={`${row.id}-visibility`}
					value={row.visibility}
					onChange={(visibility) =>
						updateNewCustomRow(row.id, "visibility", visibility)
					}
				/>
				<Button
					variant="ghost"
					size="sm"
					className="justify-start text-destructive"
					onClick={() => removeNewCustomRow(row.id)}
				>
					{t("core:delete")}
				</Button>
			</div>
			{error ? <p className="text-sm text-destructive">{error}</p> : null}
		</div>
	);
}
