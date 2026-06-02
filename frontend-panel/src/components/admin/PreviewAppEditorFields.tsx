import type { ReactNode } from "react";
import { DelimitedListInput } from "@/components/admin/DelimitedListInput";
import { Input } from "@/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import {
	getTablePreviewDelimiterLabelKey,
	normalizeTablePreviewDelimiter,
	type TablePreviewDelimiterValue,
} from "@/lib/tablePreview";
import { cn } from "@/lib/utils";
import {
	formatPreviewAppsDelimitedInput,
	getPreviewAppProvider,
	isTablePreviewAppKey,
	isUrlTemplatePreviewApp,
	isWopiPreviewApp,
	type PreviewAppsEditorApp,
	type PreviewAppsEditorConfig,
	parsePreviewAppsDelimitedInput,
} from "./previewAppsConfigEditorShared";

type Translate = (
	key: string,
	values?: Record<string, number | string>,
) => string;

interface PreviewAppEditorFieldsProps {
	app: PreviewAppsEditorApp;
	index: number;
	protectedBuiltin: boolean;
	t: Translate;
	updateApp: (
		index: number,
		updater: (app: PreviewAppsEditorApp) => PreviewAppsEditorApp,
	) => void;
	updateDraft: (
		updater: (current: PreviewAppsEditorConfig) => PreviewAppsEditorConfig,
	) => void;
	onOpenUrlTemplateVariables: () => void;
}

type UpdateApp = PreviewAppEditorFieldsProps["updateApp"];
type UpdateDraft = PreviewAppEditorFieldsProps["updateDraft"];

function getTablePreviewDelimiterLabel(
	delimiter: TablePreviewDelimiterValue,
	t: Translate,
) {
	return t(getTablePreviewDelimiterLabelKey(delimiter));
}

function EditorField({
	children,
	className,
	description,
	label,
}: {
	children: ReactNode;
	className?: string;
	description?: ReactNode;
	label: string;
}) {
	return (
		<div className={cn("space-y-1.5", className)}>
			<p className="text-xs font-medium text-muted-foreground">{label}</p>
			{children}
			{description ? (
				<div className="text-xs text-muted-foreground">{description}</div>
			) : null}
		</div>
	);
}

const TABLE_PREVIEW_DELIMITERS = ["auto", ",", "\t", ";", "|"] as const;

export function PreviewAppEditorFields({
	app,
	index,
	protectedBuiltin,
	t,
	updateApp,
	updateDraft,
	onOpenUrlTemplateVariables,
}: PreviewAppEditorFieldsProps) {
	return (
		<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
			<PreviewAppIdentityFields
				app={app}
				index={index}
				protectedBuiltin={protectedBuiltin}
				t={t}
				updateApp={updateApp}
				updateDraft={updateDraft}
			/>
			{!protectedBuiltin ? (
				<PreviewAppProviderField
					app={app}
					index={index}
					t={t}
					updateApp={updateApp}
				/>
			) : null}
			<PreviewAppLabelFields
				app={app}
				index={index}
				t={t}
				updateApp={updateApp}
			/>
			<PreviewAppExtensionsField
				app={app}
				index={index}
				t={t}
				updateApp={updateApp}
			/>
			{isTablePreviewAppKey(app.key) ? (
				<TableDelimiterField
					app={app}
					index={index}
					t={t}
					updateApp={updateApp}
				/>
			) : null}
			{isUrlTemplatePreviewApp(app) ? (
				<UrlTemplateFields
					app={app}
					index={index}
					onOpenUrlTemplateVariables={onOpenUrlTemplateVariables}
					t={t}
					updateApp={updateApp}
				/>
			) : null}
			{isWopiPreviewApp(app) ? (
				<WopiFields app={app} index={index} t={t} updateApp={updateApp} />
			) : null}
		</div>
	);
}

interface PreviewAppFieldGroupProps {
	app: PreviewAppsEditorApp;
	index: number;
	t: Translate;
	updateApp: UpdateApp;
}

function PreviewAppIdentityFields({
	app,
	index,
	protectedBuiltin,
	t,
	updateApp,
	updateDraft,
}: PreviewAppFieldGroupProps & {
	protectedBuiltin: boolean;
	updateDraft: UpdateDraft;
}) {
	return (
		<>
			<EditorField label={t("preview_apps_key_label")}>
				<Input
					disabled={protectedBuiltin}
					value={app.key}
					onChange={(event) => {
						const nextKey = event.target.value;
						updateDraft((current) => ({
							...current,
							apps: current.apps.map((candidate, appIndex) =>
								appIndex === index ? { ...candidate, key: nextKey } : candidate,
							),
						}));
					}}
				/>
				{protectedBuiltin ? (
					<p className="text-xs text-muted-foreground">
						{t("preview_apps_builtin_key_locked")}
					</p>
				) : null}
			</EditorField>
			<EditorField
				label={t("preview_apps_icon_label")}
				description={t("preview_apps_icon_hint")}
			>
				<Input
					value={app.icon}
					onChange={(event) =>
						updateApp(index, (current) => ({
							...current,
							icon: event.target.value,
						}))
					}
				/>
			</EditorField>
		</>
	);
}

function PreviewAppProviderField({
	app,
	index,
	t,
	updateApp,
}: PreviewAppFieldGroupProps) {
	const providerOptions = [
		{
			label: t("preview_apps_provider_url_template"),
			value: "url_template",
		},
		{
			label: t("preview_apps_provider_wopi"),
			value: "wopi",
		},
	];

	return (
		<EditorField label={t("preview_apps_provider_label")}>
			<Select
				items={providerOptions}
				value={getPreviewAppProvider(app.provider) || "url_template"}
				onValueChange={(provider) =>
					updateApp(index, (current) => ({
						...current,
						provider: provider === "wopi" ? "wopi" : "url_template",
						config: {
							...current.config,
							mode:
								typeof current.config.mode === "string"
									? current.config.mode
									: "iframe",
						},
					}))
				}
			>
				<SelectTrigger size="sm" aria-label={t("preview_apps_provider_label")}>
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					{providerOptions.map((option) => (
						<SelectItem key={option.value} value={option.value}>
							{option.label}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
		</EditorField>
	);
}

function PreviewAppLabelFields({
	app,
	index,
	t,
	updateApp,
}: PreviewAppFieldGroupProps) {
	return (
		<>
			<EditorField label={t("preview_apps_label_zh_label")}>
				<Input
					value={app.labels.zh ?? ""}
					onChange={(event) =>
						updateApp(index, (current) => ({
							...current,
							labels: {
								...current.labels,
								zh: event.target.value,
							},
						}))
					}
				/>
			</EditorField>
			<EditorField label={t("preview_apps_label_en_label")}>
				<Input
					value={app.labels.en ?? ""}
					onChange={(event) =>
						updateApp(index, (current) => ({
							...current,
							labels: {
								...current.labels,
								en: event.target.value,
							},
						}))
					}
				/>
			</EditorField>
		</>
	);
}

function PreviewAppExtensionsField({
	app,
	index,
	t,
	updateApp,
}: PreviewAppFieldGroupProps) {
	return (
		<EditorField
			className="md:col-span-2 xl:col-span-2"
			label={t("preview_apps_matches_extensions")}
			description={t("preview_apps_list_input_hint")}
		>
			<DelimitedListInput
				placeholder={t("preview_apps_matches_extensions_placeholder")}
				values={app.extensions}
				formatValue={formatPreviewAppsDelimitedInput}
				parseValue={parsePreviewAppsDelimitedInput}
				onValueChange={(extensions) =>
					updateApp(index, (current) => ({
						...current,
						extensions,
					}))
				}
			/>
		</EditorField>
	);
}

function TableDelimiterField({
	app,
	index,
	t,
	updateApp,
}: PreviewAppFieldGroupProps) {
	const delimiterOptions = TABLE_PREVIEW_DELIMITERS.map((delimiter) => ({
		label: getTablePreviewDelimiterLabel(delimiter, t),
		value: delimiter,
	}));

	return (
		<EditorField label={t("preview_apps_table_delimiter")}>
			<Select
				items={delimiterOptions}
				value={normalizeTablePreviewDelimiter(app.config.delimiter)}
				onValueChange={(delimiter) =>
					updateApp(index, (current) => ({
						...current,
						config: {
							...current.config,
							delimiter: normalizeTablePreviewDelimiter(delimiter),
						},
					}))
				}
			>
				<SelectTrigger size="sm" aria-label={t("preview_apps_table_delimiter")}>
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					{delimiterOptions.map((option) => (
						<SelectItem key={option.value} value={option.value}>
							{option.label}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
		</EditorField>
	);
}

function UrlTemplateFields({
	app,
	index,
	onOpenUrlTemplateVariables,
	t,
	updateApp,
}: PreviewAppFieldGroupProps & {
	onOpenUrlTemplateVariables: () => void;
}) {
	return (
		<>
			<PreviewModeField
				ariaLabel={t("preview_apps_url_template_mode")}
				index={index}
				label={t("preview_apps_url_template_mode")}
				options={[
					{
						label: t("preview_apps_url_template_mode_iframe"),
						value: "iframe",
					},
					{
						label: t("preview_apps_url_template_mode_new_tab"),
						value: "new_tab",
					},
				]}
				value={app.config.mode}
				updateApp={updateApp}
			/>
			<EditorField
				className="md:col-span-2 xl:col-span-2"
				label={t("preview_apps_url_template_url")}
				description={
					<div className="space-y-2">
						<p>{t("preview_apps_url_template_variables_hint")}</p>
						<button
							type="button"
							className="w-fit text-left text-primary underline-offset-4 transition-colors hover:text-primary/80 hover:underline"
							onClick={onOpenUrlTemplateVariables}
						>
							{t("preview_apps_url_template_variables_link")}
						</button>
					</div>
				}
			>
				<Input
					value={
						typeof app.config.url_template === "string"
							? app.config.url_template
							: ""
					}
					onChange={(event) =>
						updateApp(index, (current) => ({
							...current,
							config: {
								...current.config,
								url_template: event.target.value,
							},
						}))
					}
				/>
			</EditorField>
			<EditorField
				className="md:col-span-2 xl:col-span-3"
				label={t("preview_apps_url_template_allowed_origins")}
			>
				<DelimitedListInput
					values={
						Array.isArray(app.config.allowed_origins)
							? app.config.allowed_origins.filter(
									(value): value is string => typeof value === "string",
								)
							: []
					}
					formatValue={formatPreviewAppsDelimitedInput}
					parseValue={parsePreviewAppsDelimitedInput}
					onValueChange={(allowedOrigins) =>
						updateApp(index, (current) => ({
							...current,
							config: {
								...current.config,
								allowed_origins: allowedOrigins,
							},
						}))
					}
				/>
			</EditorField>
		</>
	);
}

function WopiFields({ app, index, t, updateApp }: PreviewAppFieldGroupProps) {
	return (
		<>
			<PreviewModeField
				ariaLabel={t("preview_apps_wopi_mode")}
				description={t("preview_apps_wopi_mode_desc")}
				index={index}
				label={t("preview_apps_wopi_mode")}
				options={[
					{
						label: t("preview_apps_wopi_mode_iframe"),
						value: "iframe",
					},
					{
						label: t("preview_apps_wopi_mode_new_tab"),
						value: "new_tab",
					},
				]}
				value={app.config.mode}
				updateApp={updateApp}
			/>
			<ConfigTextField
				app={app}
				configKey="action_url"
				description={t("preview_apps_wopi_action_url_desc")}
				index={index}
				label={t("preview_apps_wopi_action_url")}
				updateApp={updateApp}
			/>
			<ConfigTextField
				app={app}
				configKey="discovery_url"
				description={t("preview_apps_wopi_discovery_url_desc")}
				index={index}
				label={t("preview_apps_wopi_discovery_url")}
				updateApp={updateApp}
			/>
			<EditorField
				className="md:col-span-2 xl:col-span-2"
				label={t("preview_apps_wopi_hint_title")}
				description={t("preview_apps_wopi_hint_desc")}
			>
				<div className="rounded-xl border border-border/50 bg-muted/20 px-3 py-2 text-sm text-muted-foreground">
					{t("preview_apps_wopi_hint_body")}
				</div>
			</EditorField>
		</>
	);
}

interface PreviewModeFieldProps {
	ariaLabel: string;
	description?: ReactNode;
	index: number;
	label: string;
	options: { label: string; value: string }[];
	value: unknown;
	updateApp: UpdateApp;
}

function PreviewModeField({
	ariaLabel,
	description,
	index,
	label,
	options,
	value,
	updateApp,
}: PreviewModeFieldProps) {
	return (
		<EditorField label={label} description={description}>
			<Select
				items={options}
				value={typeof value === "string" ? value : "iframe"}
				onValueChange={(mode) =>
					updateApp(index, (current) => ({
						...current,
						config: {
							...current.config,
							mode: mode ?? "iframe",
						},
					}))
				}
			>
				<SelectTrigger size="sm" aria-label={ariaLabel}>
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					{options.map((option) => (
						<SelectItem key={option.value} value={option.value}>
							{option.label}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
		</EditorField>
	);
}

interface ConfigTextFieldProps {
	app: PreviewAppsEditorApp;
	configKey: "action_url" | "discovery_url";
	description: string;
	index: number;
	label: string;
	updateApp: UpdateApp;
}

function ConfigTextField({
	app,
	configKey,
	description,
	index,
	label,
	updateApp,
}: ConfigTextFieldProps) {
	return (
		<EditorField
			className="md:col-span-2 xl:col-span-2"
			label={label}
			description={description}
		>
			<Input
				value={
					typeof app.config[configKey] === "string" ? app.config[configKey] : ""
				}
				onChange={(event) =>
					updateApp(index, (current) => ({
						...current,
						config: {
							...current.config,
							[configKey]: event.target.value,
						},
					}))
				}
			/>
		</EditorField>
	);
}
