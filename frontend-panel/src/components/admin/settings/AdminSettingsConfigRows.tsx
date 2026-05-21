import { useEffect, useRef, useState } from "react";
import { MediaProcessingConfigEditor } from "@/components/admin/MediaProcessingConfigEditor";
import { MEDIA_PROCESSING_CONFIG_KEY } from "@/components/admin/mediaProcessingConfigEditorShared";
import { PreviewAppsConfigEditor } from "@/components/admin/PreviewAppsConfigEditor";
import { PREVIEW_APPS_CONFIG_KEY } from "@/components/admin/previewAppsConfigEditorShared";
import { useAdminSettingsCategoryContent } from "@/components/admin/settings/AdminSettingsCategoryContentContext";
import {
	ConfigCodeEditor,
	type ConfigDraftValue,
	configDraftValuesEqual,
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
	isSizeConfig,
	isStringArrayType,
	type NewCustomDraft,
	parseWholeNumber,
	SIZE_DISPLAY_UNITS,
	type SizeDisplayUnitValue,
	TIME_DISPLAY_UNITS,
	type TimeDisplayUnitValue,
	UrlAssetPreview,
} from "@/components/admin/settings/adminSettingsContentShared";
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
import { normalizePublicSiteUrl } from "@/lib/publicSiteUrl";
import { cn } from "@/lib/utils";
import type { SystemConfig } from "@/types/api";

const PUBLIC_SITE_URL_KEY = "public_site_url";

function FieldMeta({ config }: { config: SystemConfig }) {
	const {
		getDraftValue,
		getSystemConfigDescription,
		getSystemConfigLabel,
		openTemplateVariablesDialog,
		t,
	} = useAdminSettingsCategoryContent();
	const draftChanged = !configDraftValuesEqual(
		getDraftValue(config),
		config.value as ConfigDraftValue,
	);
	const requiresRestart = getConfigRequiresRestart(config);
	const configLabel = getSystemConfigLabel(config);
	const configDescription = getSystemConfigDescription(config);
	const showRawKey = configLabel !== config.key;
	const showTemplateVariableLink =
		config.category === "mail.template" && config.key.endsWith("_html");

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
			{configDescription ? (
				<p className="max-w-3xl break-words text-sm text-muted-foreground">
					{configDescription}
				</p>
			) : null}
			{showTemplateVariableLink ? (
				<button
					type="button"
					className="w-fit text-sm text-primary underline-offset-4 transition-colors hover:text-primary/80 hover:underline"
					onClick={() => openTemplateVariablesDialog(config)}
				>
					{t("mail_template_variable_link")}
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
	const itemLabel = t(
		isPublicSiteUrl
			? "public_site_url_origin_label"
			: "settings_string_array_item_label",
	);
	const addLabel = t(
		isPublicSiteUrl
			? "public_site_url_add_origin"
			: "settings_string_array_add_item",
	);
	const addCurrentOriginLabel = t("public_site_url_add_current_origin");
	const removeLabel = t(
		isPublicSiteUrl
			? "public_site_url_remove_origin"
			: "settings_string_array_remove_item",
	);
	const primaryLabel = isPublicSiteUrl
		? t("public_site_url_primary_origin")
		: null;
	const placeholder = isPublicSiteUrl
		? "https://drive.example.com"
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
					<Icon name="Plus" className="h-3.5 w-3.5" />
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
							<Icon name="Plus" className="h-4 w-4" />
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
							<Icon name="Trash" className="h-4 w-4" />
						</Button>
					</div>
				);
			})}
		</div>
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
	const hasInvalidDraftValue =
		draftValue.trim() && parseWholeNumber(draftValue) === null;

	const availableUnits = getAvailableDisplayUnits(units, draftValue);
	const preferredUnit = getPreferredDisplayUnit(units, draftValue);
	const selectedUnit =
		availableUnits.find((unit) => unit.value === displayUnits[config.key]) ??
		preferredUnit;
	const displayValue = formatDisplayValue(draftValue, selectedUnit);
	const [editingValue, setEditingValue] = useState(displayValue);
	const [focused, setFocused] = useState(false);

	useEffect(() => {
		if (!focused) {
			setEditingValue(displayValue);
		}
	}, [displayValue, focused]);

	if (hasInvalidDraftValue) {
		return null;
	}

	const updateFromDisplayValue = (value: string) => {
		const nextDisplayValue = value.trim();
		if (!nextDisplayValue) {
			updateDraftValue(config.key, "");
			return;
		}
		if (!/^\d+$/.test(nextDisplayValue)) {
			return;
		}

		const parsed = Number(nextDisplayValue);
		if (!Number.isSafeInteger(parsed)) {
			return;
		}

		const nextValue = parsed * selectedUnit.multiplier;
		if (!Number.isSafeInteger(nextValue)) {
			return;
		}

		updateDraftValue(config.key, String(nextValue));
	};

	return (
		<div
			className={cn(
				"flex flex-col gap-3 sm:flex-row sm:items-center",
				fullWidth ? "w-full max-w-2xl" : "max-w-2xl",
			)}
		>
			<Input
				type="number"
				inputMode="numeric"
				step="1"
				className="w-full sm:max-w-48"
				value={focused ? editingValue : displayValue}
				aria-invalid={hasError ? true : undefined}
				onChange={(event) => {
					const nextValue = event.target.value;
					setEditingValue(nextValue);
					updateFromDisplayValue(nextValue);
				}}
				onFocus={(event) => {
					setFocused(true);
					setEditingValue(event.currentTarget.value);
				}}
				onBlur={() => {
					setFocused(false);
					setEditingValue(displayValue);
				}}
				placeholder={t("config_value")}
			/>
			<Select
				items={availableUnits.map((unit) => ({
					label: t(unit.labelKey),
					value: unit.value,
				}))}
				value={selectedUnit.value}
				onValueChange={(value) =>
					setDisplayUnits((previous) => ({
						...previous,
						[config.key]: value as TimeDisplayUnitValue | SizeDisplayUnitValue,
					}))
				}
			>
				<SelectTrigger
					id={`${config.key}-unit`}
					width="fit"
					className="min-w-28"
					aria-label={t(unitLabelKey)}
				>
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					{availableUnits.map((unit) => (
						<SelectItem key={unit.value} value={unit.value}>
							{t(unit.labelKey)}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
		</div>
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
		handleBuildWopiDiscoveryPreviewConfig,
		handleTestFfmpegCliCommand,
		handleTestFfprobeCliCommand,
		handleTestVipsCliCommand,
		t,
		updateDraftValue,
	} = useAdminSettingsCategoryContent();
	const valueType = getConfigValueType(config);
	const draftStringValue = configValueToString(draftValue);
	const isSensitive = getConfigIsSensitive(config);
	const multiline = isMultilineType(valueType);
	const stringArray = isStringArrayType(valueType);
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
						placeholder={t("config_value")}
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
			placeholder={t("config_value")}
		/>
	);
}

export function SystemConfigRow({ config }: { config: SystemConfig }) {
	const { configValidationErrors, getDraftValue, t, updateDraftValue } =
		useAdminSettingsCategoryContent();
	const draftValue = getDraftValue(config);
	const draftStringValue = configValueToString(draftValue);
	const valueType = getConfigValueType(config);
	const error = configValidationErrors.get(config.key);

	return (
		<div className="space-y-3">
			<FieldMeta config={config} />
			{isBooleanType(valueType) ? (
				<div className="flex items-center gap-3 text-sm">
					<Switch
						id={config.key}
						aria-invalid={error ? true : undefined}
						checked={draftStringValue === "true"}
						onCheckedChange={(checked) =>
							updateDraftValue(config.key, checked ? "true" : "false")
						}
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
		</div>
	);
}

export function CustomConfigRow({ config }: { config: SystemConfig }) {
	const { getDraftValue, markCustomDeleted, t } =
		useAdminSettingsCategoryContent();
	const draftValue = getDraftValue(config);
	const valueType = getConfigValueType(config);
	const draftChanged = !configDraftValuesEqual(
		draftValue,
		config.value as ConfigDraftValue,
	);
	const multiline = isMultilineType(valueType);

	return (
		<div className="space-y-3">
			<div className="space-y-1">
				<div className="flex flex-wrap items-center gap-2">
					<p className="break-all font-mono text-sm font-medium">
						{config.key}
					</p>
					{draftChanged ? (
						<span className="text-xs font-medium text-primary">
							{t("settings_status_unsaved")}
						</span>
					) : null}
				</div>
				{getConfigDescription(config) ? (
					<p className="max-w-3xl break-words text-sm text-muted-foreground">
						{getConfigDescription(config)}
					</p>
				) : null}
			</div>

			<div
				className={
					multiline
						? "space-y-3"
						: "flex flex-col gap-3 sm:flex-row sm:items-center"
				}
			>
				<ConfigInputControl config={config} draftValue={draftValue} fullWidth />
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
