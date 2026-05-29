import { PREVIEW_APP_ICON_URLS } from "@/components/common/previewAppIconUrls";
import {
	BUILTIN_TABLE_PREVIEW_APP_KEY,
	isTablePreviewAppKey,
	normalizeTablePreviewDelimiter,
} from "@/lib/tablePreview";
import type { PreviewAppProvider } from "@/types/api";

export { isTablePreviewAppKey } from "@/lib/tablePreview";

export const PREVIEW_APPS_CONFIG_KEY = "frontend_preview_apps_json";
export const PREVIEW_APPS_CONFIG_VERSION = 2;
export const PREVIEW_APP_PROTECTED_BUILTIN_KEYS = [
	"builtin.image",
	"builtin.video",
	"builtin.audio",
	"builtin.pdf",
	"builtin.markdown",
	BUILTIN_TABLE_PREVIEW_APP_KEY,
	"builtin.formatted",
	"builtin.code",
	"builtin.try_text",
	"builtin.archive",
] as const;

export interface PreviewAppsEditorApp {
	config: Record<string, unknown>;
	enabled: boolean;
	extensions: string[];
	icon: string;
	key: string;
	labels: Record<string, string>;
	provider: PreviewAppProviderValue;
}

export interface PreviewAppsEditorConfig {
	apps: PreviewAppsEditorApp[];
	version: number;
}

export interface PreviewAppsValidationIssue {
	key: string;
	values?: Record<string, number | string>;
}

export type PreviewAppProviderValue = PreviewAppProvider | "";

const PREVIEW_APP_KEY_META: Record<string, { icon: string; labelKey: string }> =
	{
		"builtin.audio": {
			icon: PREVIEW_APP_ICON_URLS.audio,
			labelKey: "preview_apps_provider_audio",
		},
		"builtin.archive": {
			icon: PREVIEW_APP_ICON_URLS.archive,
			labelKey: "preview_apps_provider_archive",
		},
		"builtin.code": {
			icon: PREVIEW_APP_ICON_URLS.code,
			labelKey: "preview_apps_provider_code",
		},
		"builtin.formatted": {
			icon: PREVIEW_APP_ICON_URLS.json,
			labelKey: "preview_apps_provider_formatted",
		},
		"builtin.image": {
			icon: PREVIEW_APP_ICON_URLS.image,
			labelKey: "preview_apps_provider_image",
		},
		"builtin.markdown": {
			icon: PREVIEW_APP_ICON_URLS.markdown,
			labelKey: "preview_apps_provider_markdown",
		},
		"builtin.office_google": {
			icon: PREVIEW_APP_ICON_URLS.googleDrive,
			labelKey: "preview_apps_provider_url_template",
		},
		"builtin.office_microsoft": {
			icon: PREVIEW_APP_ICON_URLS.microsoftOnedrive,
			labelKey: "preview_apps_provider_url_template",
		},
		"builtin.pdf": {
			icon: PREVIEW_APP_ICON_URLS.pdf,
			labelKey: "preview_apps_provider_pdf",
		},
		[BUILTIN_TABLE_PREVIEW_APP_KEY]: {
			icon: PREVIEW_APP_ICON_URLS.table,
			labelKey: "preview_apps_provider_table",
		},
		"builtin.try_text": {
			icon: PREVIEW_APP_ICON_URLS.file,
			labelKey: "preview_apps_provider_code",
		},
		"builtin.video": {
			icon: PREVIEW_APP_ICON_URLS.video,
			labelKey: "preview_apps_provider_video",
		},
	};

const ICON_URL_PATTERN =
	/^(https?:\/\/|\/\/|\/(?!\/)|\.\/|\.\.\/|data:image\/|blob:)/i;

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readString(value: unknown) {
	return typeof value === "string" ? value : "";
}

function readBoolean(value: unknown, fallback = false) {
	return typeof value === "boolean" ? value : fallback;
}

function readStringList(value: unknown) {
	if (!Array.isArray(value)) {
		return [];
	}

	const items: string[] = [];
	const seen = new Set<string>();
	for (const valueItem of value) {
		const item = readString(valueItem).trim();
		if (item.length === 0 || seen.has(item)) {
			continue;
		}
		seen.add(item);
		items.push(item);
	}
	return items;
}

function readStringMap(value: unknown) {
	if (!isRecord(value)) {
		return {};
	}

	const next: Record<string, string> = {};
	for (const [key, item] of Object.entries(value)) {
		const normalizedKey = key.trim().toLowerCase().replaceAll("_", "-");
		const normalizedValue = readString(item).trim();
		if (!normalizedKey || !normalizedValue) {
			continue;
		}
		next[normalizedKey] = normalizedValue;
	}

	return next;
}

function cloneConfigMap(value: unknown) {
	if (!isRecord(value)) {
		return {};
	}

	return { ...value };
}

function isPreviewAppIconUrl(value: string) {
	return ICON_URL_PATTERN.test(value.trim());
}

function readPreviewAppProvider(value: unknown): PreviewAppProviderValue {
	const normalized = readString(value).trim().toLowerCase();
	if (normalized === "builtin") {
		return "builtin";
	}
	if (normalized === "url_template") {
		return "url_template";
	}
	if (normalized === "wopi") {
		return "wopi";
	}
	return "";
}

function normalizePreviewAppIconOverride(
	key: string,
	value: unknown,
	provider?: unknown,
) {
	const icon = readString(value).trim();
	if (!icon || !isPreviewAppIconUrl(icon)) {
		return "";
	}

	return icon === getPreviewAppDefaultIcon(key, provider) ? "" : icon;
}

function normalizeApp(value: unknown): PreviewAppsEditorApp {
	if (!isRecord(value)) {
		return {
			config: {},
			enabled: true,
			extensions: [],
			icon: "",
			key: "",
			labels: {},
			provider: "",
		};
	}

	const key = readString(value.key);
	const provider = getPreviewAppProvider(value.provider);
	const labels = readStringMap(value.labels);
	const config = cloneConfigMap(value.config);

	if (isTablePreviewAppKey(key)) {
		config.delimiter = normalizeTablePreviewDelimiter(config.delimiter);
	}

	return {
		config,
		enabled: readBoolean(value.enabled, true),
		extensions: readStringList(value.extensions),
		icon: normalizePreviewAppIconOverride(key, value.icon, provider),
		key,
		labels,
		provider,
	};
}

function createProtectedBuiltinPreviewAppDraft(
	key: (typeof PREVIEW_APP_PROTECTED_BUILTIN_KEYS)[number],
): PreviewAppsEditorApp {
	const base: PreviewAppsEditorApp = {
		config: {},
		enabled: true,
		extensions: [],
		icon: "",
		key,
		labels: protectedBuiltinLabels(key),
		provider: "builtin",
	};

	if (isTablePreviewAppKey(key)) {
		return {
			...base,
			config: { delimiter: "auto" },
			extensions: ["csv", "tsv"],
		};
	}

	if (key === "builtin.pdf") {
		return { ...base, extensions: ["pdf"] };
	}

	if (key === "builtin.markdown") {
		return { ...base, extensions: ["md", "markdown"] };
	}

	if (key === "builtin.formatted") {
		return { ...base, extensions: ["json", "xml"] };
	}

	if (key === "builtin.archive") {
		return { ...base, extensions: ["zip", "7z"] };
	}

	return base;
}

function protectedBuiltinLabels(key: string): Record<string, string> {
	switch (key) {
		case "builtin.image":
			return { en: "Image preview", zh: "图片预览" };
		case "builtin.video":
			return { en: "Video preview", zh: "视频预览" };
		case "builtin.audio":
			return { en: "Audio preview", zh: "音频预览" };
		case "builtin.pdf":
			return { en: "PDF preview", zh: "PDF 预览" };
		case "builtin.markdown":
			return { en: "Markdown preview", zh: "Markdown 预览" };
		case BUILTIN_TABLE_PREVIEW_APP_KEY:
			return { en: "Table preview", zh: "表格预览" };
		case "builtin.formatted":
			return { en: "Formatted view", zh: "格式化视图" };
		case "builtin.code":
			return { en: "Source view", zh: "源码视图" };
		case "builtin.try_text":
			return { en: "Open as text", zh: "以文本方式打开" };
		case "builtin.archive":
			return { en: "Archive preview", zh: "压缩包预览" };
		default:
			return {};
	}
}

function restoreMissingProtectedBuiltinPreviewApps(
	apps: PreviewAppsEditorApp[],
) {
	const existingKeys = new Set<string>();
	for (const app of apps) {
		const key = app.key.trim();
		if (key.length > 0) {
			existingKeys.add(key);
		}
	}

	const restored = [...apps];
	for (const key of PREVIEW_APP_PROTECTED_BUILTIN_KEYS) {
		if (!existingKeys.has(key)) {
			restored.push(createProtectedBuiltinPreviewAppDraft(key));
		}
	}
	return restored;
}

function pruneConfigValue(value: unknown): unknown {
	if (typeof value === "string") {
		const trimmed = value.trim();
		return trimmed.length > 0 ? trimmed : undefined;
	}

	if (Array.isArray(value)) {
		const items: unknown[] = [];
		for (const item of value) {
			const pruned = pruneConfigValue(item);
			if (pruned !== undefined) {
				items.push(pruned);
			}
		}
		return items.length > 0 ? items : undefined;
	}

	if (value === null || value === undefined) {
		return undefined;
	}

	return value;
}

function pruneConfigMap(config: Record<string, unknown>) {
	const next: Record<string, unknown> = {};

	for (const [key, value] of Object.entries(config)) {
		const normalized = pruneConfigValue(value);
		if (normalized !== undefined) {
			next[key] = normalized;
		}
	}

	return next;
}

function pruneStringMap(values: Record<string, string>) {
	const next: Record<string, string> = {};

	for (const [key, value] of Object.entries(values)) {
		const normalizedKey = key.trim().toLowerCase().replaceAll("_", "-");
		const normalizedValue = value.trim();
		if (!normalizedKey || !normalizedValue) {
			continue;
		}
		next[normalizedKey] = normalizedValue;
	}

	return next;
}

export function parsePreviewAppsDelimitedInput(value: string) {
	const items: string[] = [];
	const seen = new Set<string>();
	for (const rawItem of value.split(",")) {
		const item = rawItem.trim();
		if (item.length === 0 || seen.has(item)) {
			continue;
		}
		seen.add(item);
		items.push(item);
	}
	return items;
}

export function formatPreviewAppsDelimitedInput(values: string[]) {
	return values.join(", ");
}

export function getPreviewAppProvider(
	provider?: unknown,
): PreviewAppProviderValue {
	return readPreviewAppProvider(provider);
}

export function getPreviewAppDefaultIcon(key: string, provider?: unknown) {
	if (getPreviewAppProvider(provider) === "wopi") {
		return PREVIEW_APP_ICON_URLS.web;
	}

	return PREVIEW_APP_KEY_META[key.trim()]?.icon ?? PREVIEW_APP_ICON_URLS.web;
}

export function getPreviewAppKindLabelKey(key: string, provider?: unknown) {
	const resolvedProvider = getPreviewAppProvider(provider);
	if (resolvedProvider === "wopi") {
		return "preview_apps_provider_wopi";
	}

	return (
		PREVIEW_APP_KEY_META[key.trim()]?.labelKey ??
		"preview_apps_provider_url_template"
	);
}

export function isProtectedBuiltinPreviewAppKey(key: string) {
	return PREVIEW_APP_PROTECTED_BUILTIN_KEYS.includes(
		key.trim() as (typeof PREVIEW_APP_PROTECTED_BUILTIN_KEYS)[number],
	);
}

export function isExternalPreviewAppKey(key: string) {
	return !isProtectedBuiltinPreviewAppKey(key);
}

export function isUrlTemplatePreviewApp(app: PreviewAppsEditorApp) {
	return app.provider === "url_template";
}

export function isWopiPreviewApp(app: PreviewAppsEditorApp) {
	return app.provider === "wopi";
}

export function parsePreviewAppsConfig(value: string): PreviewAppsEditorConfig {
	const parsed = JSON.parse(value) as unknown;
	if (!isRecord(parsed)) {
		throw new Error("preview apps config must be an object");
	}

	return {
		apps: restoreMissingProtectedBuiltinPreviewApps(
			Array.isArray(parsed.apps) ? parsed.apps.map(normalizeApp) : [],
		),
		version:
			typeof parsed.version === "number"
				? parsed.version
				: PREVIEW_APPS_CONFIG_VERSION,
	};
}

export function serializePreviewAppsConfig(config: PreviewAppsEditorConfig) {
	return JSON.stringify(
		{
			version: config.version,
			apps: config.apps.map((app) => {
				const key = app.key.trim();
				const nextConfig = pruneConfigMap(app.config);
				const nextIcon = normalizePreviewAppIconOverride(
					key,
					app.icon,
					app.provider,
				);
				const nextLabels = pruneStringMap(app.labels);
				const extensions: string[] = [];
				const seenExtensions = new Set<string>();
				for (const rawExtension of app.extensions) {
					const extension = rawExtension.trim();
					if (extension.length === 0 || seenExtensions.has(extension)) {
						continue;
					}
					seenExtensions.add(extension);
					extensions.push(extension);
				}

				return {
					...(Object.keys(nextConfig).length > 0 ? { config: nextConfig } : {}),
					enabled: app.enabled,
					...(extensions.length > 0 ? { extensions } : {}),
					icon: nextIcon,
					key,
					provider: app.provider,
					...(Object.keys(nextLabels).length > 0 ? { labels: nextLabels } : {}),
				};
			}),
		},
		null,
		2,
	);
}

export function getPreviewAppsConfigIssues(
	config: PreviewAppsEditorConfig,
): PreviewAppsValidationIssue[] {
	const issues: PreviewAppsValidationIssue[] = [];

	if (config.version !== PREVIEW_APPS_CONFIG_VERSION) {
		issues.push({
			key: "preview_apps_error_version_mismatch",
			values: { version: PREVIEW_APPS_CONFIG_VERSION },
		});
	}

	const keyCounts = new Map<string, number>();
	for (const app of config.apps) {
		const key = app.key.trim();
		if (!key) {
			continue;
		}
		keyCounts.set(key, (keyCounts.get(key) ?? 0) + 1);
	}

	for (const [index, app] of config.apps.entries()) {
		const appNumber = index + 1;
		const key = app.key.trim();
		if (!key) {
			issues.push({
				key: "preview_apps_error_app_key_required",
				values: { index: appNumber },
			});
		} else if ((keyCounts.get(key) ?? 0) > 1) {
			issues.push({
				key: "preview_apps_error_app_key_duplicate",
				values: { index: appNumber, key },
			});
		}

		if (Object.values(app.labels).every((value) => value.trim().length === 0)) {
			issues.push({
				key: "preview_apps_error_app_label_required",
				values: { index: appNumber },
			});
		}

		const provider = getPreviewAppProvider(app.provider);
		if (!provider) {
			issues.push({
				key: "preview_apps_error_app_provider_required",
				values: { index: appNumber },
			});
		}

		if (provider === "url_template") {
			const mode =
				typeof app.config.mode === "string" ? app.config.mode.trim() : "";
			if (!mode) {
				issues.push({
					key: "preview_apps_error_url_template_mode_required",
					values: { index: appNumber },
				});
			}

			const urlTemplate =
				typeof app.config.url_template === "string"
					? app.config.url_template.trim()
					: "";
			if (!urlTemplate) {
				issues.push({
					key: "preview_apps_error_url_template_required",
					values: { index: appNumber },
				});
			}
		}

		if (provider === "wopi") {
			const mode =
				typeof app.config.mode === "string" ? app.config.mode.trim() : "";
			if (!mode) {
				issues.push({
					key: "preview_apps_error_wopi_mode_required",
					values: { index: appNumber },
				});
			}

			const actionUrl =
				typeof app.config.action_url === "string"
					? app.config.action_url.trim()
					: "";
			const actionUrlTemplate =
				typeof app.config.action_url_template === "string"
					? app.config.action_url_template.trim()
					: "";
			const discoveryUrl =
				typeof app.config.discovery_url === "string"
					? app.config.discovery_url.trim()
					: "";
			if (!actionUrl && !actionUrlTemplate && !discoveryUrl) {
				issues.push({
					key: "preview_apps_error_wopi_target_required",
					values: { index: appNumber },
				});
			}
		}
	}

	return issues;
}

export function getPreviewAppsConfigIssuesFromString(value: string) {
	try {
		return getPreviewAppsConfigIssues(parsePreviewAppsConfig(value));
	} catch {
		return [{ key: "preview_apps_error_parse" }];
	}
}

function getNextCustomKey(existingKeys: string[]) {
	let index = 1;
	let candidate = `custom.app_${index}`;
	const existingKeySet = new Set(existingKeys);

	while (existingKeySet.has(candidate)) {
		index += 1;
		candidate = `custom.app_${index}`;
	}

	return candidate;
}

export function createPreviewAppDraft(
	existingKeys: string[],
): PreviewAppsEditorApp {
	return {
		config: {
			allowed_origins: [],
			mode: "iframe",
			url_template: "",
		},
		enabled: true,
		extensions: [],
		icon: "",
		key: getNextCustomKey(existingKeys),
		labels: {},
		provider: "url_template",
	};
}

export function movePreviewEditorItem<T>(
	items: T[],
	index: number,
	direction: -1 | 1,
) {
	const targetIndex = index + direction;
	if (targetIndex < 0 || targetIndex >= items.length) {
		return items;
	}

	const nextItems = [...items];
	const [item] = nextItems.splice(index, 1);
	if (item === undefined) {
		return items;
	}
	nextItems.splice(targetIndex, 0, item);
	return nextItems;
}
