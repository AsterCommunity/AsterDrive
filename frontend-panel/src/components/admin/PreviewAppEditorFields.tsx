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
			<EditorField label={t("preview_apps_key_label")}>
				<Input
					disabled={protectedBuiltin}
					value={app.key}
					onChange={(event) => {
						const nextKey = event.target.value;
						updateDraft((current) => {
							return {
								...current,
								apps: current.apps.map((candidate, appIndex) =>
									appIndex === index
										? { ...candidate, key: nextKey }
										: candidate,
								),
							};
						});
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
			{!protectedBuiltin ? (
				<EditorField label={t("preview_apps_provider_label")}>
					<Select
						items={[
							{
								label: t("preview_apps_provider_url_template"),
								value: "url_template",
							},
							{
								label: t("preview_apps_provider_wopi"),
								value: "wopi",
							},
						]}
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
						<SelectTrigger
							size="sm"
							aria-label={t("preview_apps_provider_label")}
						>
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							<SelectItem value="url_template">
								{t("preview_apps_provider_url_template")}
							</SelectItem>
							<SelectItem value="wopi">
								{t("preview_apps_provider_wopi")}
							</SelectItem>
						</SelectContent>
					</Select>
				</EditorField>
			) : null}
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
			{isTablePreviewAppKey(app.key) ? (
				<EditorField label={t("preview_apps_table_delimiter")}>
					<Select
						items={[
							{
								label: getTablePreviewDelimiterLabel("auto", t),
								value: "auto",
							},
							{
								label: getTablePreviewDelimiterLabel(",", t),
								value: ",",
							},
							{
								label: getTablePreviewDelimiterLabel("\t", t),
								value: "\t",
							},
							{
								label: getTablePreviewDelimiterLabel(";", t),
								value: ";",
							},
							{
								label: getTablePreviewDelimiterLabel("|", t),
								value: "|",
							},
						]}
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
						<SelectTrigger
							size="sm"
							aria-label={t("preview_apps_table_delimiter")}
						>
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{(["auto", ",", "\t", ";", "|"] as const).map((delimiter) => (
								<SelectItem key={delimiter} value={delimiter}>
									{getTablePreviewDelimiterLabel(delimiter, t)}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</EditorField>
			) : null}
			{isUrlTemplatePreviewApp(app) ? (
				<>
					<EditorField label={t("preview_apps_url_template_mode")}>
						<Select
							items={[
								{
									label: t("preview_apps_url_template_mode_iframe"),
									value: "iframe",
								},
								{
									label: t("preview_apps_url_template_mode_new_tab"),
									value: "new_tab",
								},
							]}
							value={
								typeof app.config.mode === "string" ? app.config.mode : "iframe"
							}
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
							<SelectTrigger
								size="sm"
								aria-label={t("preview_apps_url_template_mode")}
							>
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="iframe">
									{t("preview_apps_url_template_mode_iframe")}
								</SelectItem>
								<SelectItem value="new_tab">
									{t("preview_apps_url_template_mode_new_tab")}
								</SelectItem>
							</SelectContent>
						</Select>
					</EditorField>
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
			) : null}
			{isWopiPreviewApp(app) ? (
				<>
					<EditorField
						label={t("preview_apps_wopi_mode")}
						description={t("preview_apps_wopi_mode_desc")}
					>
						<Select
							items={[
								{
									label: t("preview_apps_wopi_mode_iframe"),
									value: "iframe",
								},
								{
									label: t("preview_apps_wopi_mode_new_tab"),
									value: "new_tab",
								},
							]}
							value={
								typeof app.config.mode === "string" ? app.config.mode : "iframe"
							}
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
							<SelectTrigger size="sm" aria-label={t("preview_apps_wopi_mode")}>
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="iframe">
									{t("preview_apps_wopi_mode_iframe")}
								</SelectItem>
								<SelectItem value="new_tab">
									{t("preview_apps_wopi_mode_new_tab")}
								</SelectItem>
							</SelectContent>
						</Select>
					</EditorField>
					<EditorField
						className="md:col-span-2 xl:col-span-2"
						label={t("preview_apps_wopi_action_url")}
						description={t("preview_apps_wopi_action_url_desc")}
					>
						<Input
							value={
								typeof app.config.action_url === "string"
									? app.config.action_url
									: ""
							}
							onChange={(event) =>
								updateApp(index, (current) => ({
									...current,
									config: {
										...current.config,
										action_url: event.target.value,
									},
								}))
							}
						/>
					</EditorField>
					<EditorField
						className="md:col-span-2 xl:col-span-2"
						label={t("preview_apps_wopi_discovery_url")}
						description={t("preview_apps_wopi_discovery_url_desc")}
					>
						<Input
							value={
								typeof app.config.discovery_url === "string"
									? app.config.discovery_url
									: ""
							}
							onChange={(event) =>
								updateApp(index, (current) => ({
									...current,
									config: {
										...current.config,
										discovery_url: event.target.value,
									},
								}))
							}
						/>
					</EditorField>
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
			) : null}
		</div>
	);
}
