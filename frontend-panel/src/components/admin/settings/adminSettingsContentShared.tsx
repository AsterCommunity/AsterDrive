import {
	type ReactNode,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { CodePreviewEditor } from "@/components/files/preview/CodePreviewEditor";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import type {
	SystemConfig,
	SystemConfigSource,
	SystemConfigValueType,
} from "@/types/api";

const TEMPLATE_GROUP_EXPAND_DURATION_MS = 280;
const TEMPLATE_GROUP_COLLAPSE_DURATION_MS = 240;
const TEMPLATE_GROUP_EXPAND_EASING = "cubic-bezier(0.22, 1, 0.36, 1)";
const TEMPLATE_GROUP_COLLAPSE_EASING = "cubic-bezier(0.32, 0, 0.67, 0.96)";

const SIZE_CONFIG_KEYS = new Set(["default_storage_quota"]);

export type BrandingAssetPreviewAppearance = {
	fallbackLabel: string;
	frameClassName: string;
	imageClassName: string;
	validClassName: string;
	validHoverClassName: string;
};

const BRANDING_ASSET_PREVIEW_APPEARANCES: Record<
	string,
	BrandingAssetPreviewAppearance
> = {
	branding_favicon_url: {
		fallbackLabel: "/favicon.svg",
		frameClassName: "w-12",
		imageClassName: "max-h-8 max-w-8 object-contain",
		validClassName: "border-neutral-200 bg-white",
		validHoverClassName: "hover:border-primary/40 hover:bg-neutral-50",
	},
	branding_wordmark_dark_url: {
		fallbackLabel: "/static/asterdrive/asterdrive-dark.svg",
		frameClassName: "w-36 px-4",
		imageClassName: "max-h-7 w-full object-contain",
		validClassName: "border-neutral-200 bg-white",
		validHoverClassName: "hover:border-primary/40 hover:bg-neutral-50",
	},
	branding_wordmark_light_url: {
		fallbackLabel: "/static/asterdrive/asterdrive-light.svg",
		frameClassName: "w-36 px-4",
		imageClassName: "max-h-7 w-full object-contain",
		validClassName: "border-neutral-700 bg-black",
		validHoverClassName: "hover:border-primary/50 hover:bg-neutral-950",
	},
};

export type ConfigDraftValue = string | string[];
export type DraftValues = Record<string, ConfigDraftValue>;

type TimeConfigBaseUnit = "seconds" | "hours" | "days";
export type TimeDisplayUnitValue =
	| "seconds"
	| "minutes"
	| "hours"
	| "days"
	| "weeks";
export type SizeDisplayUnitValue =
	| "bytes"
	| "kilobytes"
	| "megabytes"
	| "gigabytes"
	| "terabytes";

export type DisplayUnit = {
	labelKey: string;
	multiplier: number;
	value: string;
};

export type NewCustomDraft = {
	id: string;
	key: string;
	value: string;
};

type CategoryPath = {
	category: string;
	subcategory?: string;
};

export type SystemSubcategoryGroup = {
	category: string;
	subcategory?: string;
	configs: SystemConfig[];
};

export const TIME_DISPLAY_UNITS: Record<
	TimeConfigBaseUnit,
	readonly DisplayUnit[]
> = {
	seconds: [
		{
			value: "days",
			labelKey: "settings_time_unit_days",
			multiplier: 24 * 60 * 60,
		},
		{
			value: "hours",
			labelKey: "settings_time_unit_hours",
			multiplier: 60 * 60,
		},
		{
			value: "minutes",
			labelKey: "settings_time_unit_minutes",
			multiplier: 60,
		},
		{
			value: "seconds",
			labelKey: "settings_time_unit_seconds",
			multiplier: 1,
		},
	],
	hours: [
		{
			value: "days",
			labelKey: "settings_time_unit_days",
			multiplier: 24,
		},
		{
			value: "hours",
			labelKey: "settings_time_unit_hours",
			multiplier: 1,
		},
	],
	days: [
		{
			value: "weeks",
			labelKey: "settings_time_unit_weeks",
			multiplier: 7,
		},
		{
			value: "days",
			labelKey: "settings_time_unit_days",
			multiplier: 1,
		},
	],
};

export const SIZE_DISPLAY_UNITS: readonly DisplayUnit[] = [
	{
		value: "terabytes",
		labelKey: "settings_size_unit_terabytes",
		multiplier: 1024 ** 4,
	},
	{
		value: "gigabytes",
		labelKey: "settings_size_unit_gigabytes",
		multiplier: 1024 ** 3,
	},
	{
		value: "megabytes",
		labelKey: "settings_size_unit_megabytes",
		multiplier: 1024 ** 2,
	},
	{
		value: "kilobytes",
		labelKey: "settings_size_unit_kilobytes",
		multiplier: 1024,
	},
	{
		value: "bytes",
		labelKey: "settings_size_unit_bytes",
		multiplier: 1,
	},
];

export function getConfigDescription(config: SystemConfig) {
	return config.description || undefined;
}

export function getConfigIsSensitive(config: SystemConfig) {
	return config.is_sensitive ?? false;
}

export function getConfigRequiresRestart(config: SystemConfig) {
	return config.requires_restart ?? false;
}

export function getConfigValueType(config: SystemConfig) {
	return config.value_type ?? "string";
}

export function isBooleanType(valueType: SystemConfigValueType) {
	return valueType === "boolean";
}

export function isNumberType(valueType: SystemConfigValueType) {
	return valueType === "number";
}

export function isMultilineType(valueType: SystemConfigValueType) {
	return valueType === "multiline";
}

export function isStringArrayType(valueType: SystemConfigValueType) {
	return valueType === "string_array";
}

export function isStringEnumSetType(valueType: SystemConfigValueType) {
	return valueType === "string_enum_set";
}

export function configValueToString(value: ConfigDraftValue | undefined) {
	return typeof value === "string" ? value : "";
}

export function configValueToStringArray(value: ConfigDraftValue | undefined) {
	return Array.isArray(value) ? value : [];
}

export function configDraftValuesEqual(
	left: ConfigDraftValue | undefined,
	right: ConfigDraftValue | undefined,
) {
	if (Array.isArray(left) || Array.isArray(right)) {
		if (!Array.isArray(left) || !Array.isArray(right)) {
			return false;
		}
		return (
			left.length === right.length &&
			left.every((value, index) => value === right[index])
		);
	}

	return (left ?? "") === (right ?? "");
}

export function isSystemConfigSource(source: SystemConfigSource) {
	return source === "system";
}

export function isBrandingAssetConfig(config: SystemConfig) {
	return config.key in BRANDING_ASSET_PREVIEW_APPEARANCES;
}

export function getBrandingAssetPreviewAppearance(config: SystemConfig) {
	return BRANDING_ASSET_PREVIEW_APPEARANCES[config.key];
}

export function getTimeConfigBaseUnit(
	config: SystemConfig,
): TimeConfigBaseUnit | null {
	if (!isNumberType(getConfigValueType(config))) {
		return null;
	}
	if (config.key.endsWith("_secs")) {
		return "seconds";
	}
	if (config.key.endsWith("_hours")) {
		return "hours";
	}
	if (config.key.endsWith("_days")) {
		return "days";
	}
	return null;
}

export function isSizeConfig(config: SystemConfig) {
	return (
		isNumberType(getConfigValueType(config)) &&
		(config.key.endsWith("_bytes") || SIZE_CONFIG_KEYS.has(config.key))
	);
}

export function parseWholeNumber(value: string) {
	const trimmed = value.trim();
	if (!trimmed) {
		return null;
	}
	if (!/^-?\d+$/.test(trimmed)) {
		return null;
	}

	const parsed = Number(trimmed);
	return Number.isSafeInteger(parsed) ? parsed : null;
}

export function getAvailableDisplayUnits<T extends DisplayUnit>(
	units: readonly T[],
	value: string,
) {
	const parsed = parseWholeNumber(value);

	if (parsed === null) {
		return units;
	}

	return units.filter(
		(unit) => unit.multiplier === 1 || parsed % unit.multiplier === 0,
	);
}

export function getPreferredDisplayUnit<T extends DisplayUnit>(
	units: readonly T[],
	value: string,
) {
	const parsed = parseWholeNumber(value);
	if (parsed === 0) {
		return units[units.length - 1];
	}

	return getAvailableDisplayUnits(units, value)[0] ?? units[units.length - 1];
}

export function formatDisplayValue(value: string, unit: DisplayUnit) {
	if (!value.trim()) {
		return "";
	}

	const parsed = parseWholeNumber(value);
	if (parsed === null) {
		return value;
	}

	return String(parsed / unit.multiplier);
}

function normalizeAssetPreviewUrl(value: string) {
	const normalized = value.trim();
	if (!normalized || normalized.includes(" ")) {
		return null;
	}
	if (normalized.startsWith("/") && !normalized.startsWith("//")) {
		return normalized;
	}

	try {
		const resolved = new URL(normalized);
		if (resolved.protocol === "http:" || resolved.protocol === "https:") {
			return resolved.toString();
		}
	} catch {
		return null;
	}

	return null;
}

export function UrlAssetPreview({
	url,
	appearance,
}: {
	url: string;
	appearance: BrandingAssetPreviewAppearance;
}) {
	const [debouncedUrl, setDebouncedUrl] = useState(url);

	useEffect(() => {
		const timer = window.setTimeout(() => {
			setDebouncedUrl(url);
		}, 300);
		return () => window.clearTimeout(timer);
	}, [url]);

	const normalizedUrl = useMemo(
		() => normalizeAssetPreviewUrl(debouncedUrl),
		[debouncedUrl],
	);
	const isInvalid = debouncedUrl.trim().length > 0 && !normalizedUrl;
	const previewClassName = cn(
		"group flex h-12 shrink-0 items-center justify-center overflow-hidden rounded-xl border transition-colors",
		appearance.frameClassName,
		normalizedUrl
			? [appearance.validClassName, appearance.validHoverClassName]
			: isInvalid
				? "border-amber-300/70 bg-amber-50/70"
				: appearance.validClassName,
	);

	const previewContent = normalizedUrl ? (
		<UrlAssetPreviewImage
			key={normalizedUrl}
			data-testid="branding-asset-preview-image"
			className={appearance.imageClassName}
			url={normalizedUrl}
		/>
	) : (
		<Icon
			name={isInvalid ? "Warning" : "LinkSimple"}
			className={cn(
				"size-4",
				isInvalid ? "text-amber-600" : "text-muted-foreground",
			)}
		/>
	);

	return (
		<div data-testid="branding-asset-preview" className="shrink-0">
			{normalizedUrl ? (
				<a
					href={normalizedUrl}
					target="_blank"
					rel="noreferrer"
					className={previewClassName}
					title={normalizedUrl}
					aria-label={normalizedUrl}
				>
					{previewContent}
				</a>
			) : (
				<div
					role="img"
					className={previewClassName}
					title={debouncedUrl.trim() || appearance.fallbackLabel}
					aria-label={debouncedUrl.trim() || appearance.fallbackLabel}
				>
					{previewContent}
				</div>
			)}
		</div>
	);
}

function UrlAssetPreviewImage({
	className,
	url,
	...props
}: {
	className?: string;
	url: string;
	"data-testid"?: string;
}) {
	const [hasLoadError, setHasLoadError] = useState(false);

	if (hasLoadError) {
		return <Icon name="Warning" className="size-5 text-amber-600" />;
	}

	return (
		<img
			{...props}
			src={url}
			alt=""
			className={className}
			onError={() => setHasLoadError(true)}
		/>
	);
}

function splitCategoryPath(category?: string): CategoryPath {
	const normalized = category?.trim() || "other";
	const [root, ...rest] = normalized.split(".");
	const subcategory = rest.join(".").trim();

	return {
		category: root || "other",
		subcategory: subcategory || undefined,
	};
}

export function normalizeCategory(category?: string) {
	return splitCategoryPath(category).category;
}

export function normalizeSubcategory(category?: string) {
	return splitCategoryPath(category).subcategory;
}

export function formatSubcategoryLabel(segment: string) {
	return segment
		.split(/[._-]+/)
		.filter(Boolean)
		.map((part) => part[0]?.toUpperCase() + part.slice(1))
		.join(" ");
}

export function getSubcategoryGroupKey(category: string, subcategory?: string) {
	return `${category}:${subcategory ?? "__default__"}`;
}

const MAIL_TEMPLATE_GROUP_ORDER = [
	"register_activation",
	"contact_change_confirmation",
	"password_reset",
	"password_reset_notice",
	"contact_change_notice",
	"external_auth_email_verification",
] as const;

export function getMailTemplateGroupOrderIndex(groupId: string) {
	const index = MAIL_TEMPLATE_GROUP_ORDER.indexOf(
		groupId as (typeof MAIL_TEMPLATE_GROUP_ORDER)[number],
	);
	return index === -1 ? Number.MAX_SAFE_INTEGER : index;
}

export function getMailTemplateGroupId(configKey: string) {
	return configKey
		.replace(/^mail_template_/, "")
		.replace(/_(subject|html)$/, "");
}

export function getMailTemplateFieldOrder(configKey: string) {
	if (configKey.endsWith("_subject")) {
		return 0;
	}
	if (configKey.endsWith("_html")) {
		return 1;
	}
	return 2;
}

export function getConfigEditorLanguage(config: SystemConfig) {
	if (config.key.endsWith("_html")) {
		return "html";
	}
	if (config.key.endsWith("_json") || config.key.endsWith(".json")) {
		return "json";
	}
	return "plaintext";
}

function getEditorLanguageLabel(language: string) {
	switch (language) {
		case "html":
			return "HTML";
		case "json":
			return "JSON";
		default:
			return "TEXT";
	}
}

export function ConfigCodeEditor({
	language,
	onChange,
	theme,
	value,
}: {
	language: string;
	onChange: (value: string) => void;
	theme: "vs" | "vs-dark";
	value: string;
}) {
	return (
		<div className="max-w-5xl overflow-hidden rounded-xl border bg-background shadow-sm">
			<div className="flex items-center gap-2 border-b bg-muted/40 px-4 py-2">
				<Icon name="FileCode" className="size-4 text-muted-foreground" />
				<span className="text-sm font-medium">
					{getEditorLanguageLabel(language)}
				</span>
			</div>
			<div className="h-80 min-h-80 bg-background">
				<CodePreviewEditor
					language={language}
					theme={theme}
					value={value}
					onChange={onChange}
					options={{
						domReadOnly: false,
						fontSize: 13,
						lineNumbers: "on",
						padding: { top: 12 },
						readOnly: false,
						renderLineHighlight: "line",
						scrollBeyondLastLine: false,
						wordWrap: "off",
					}}
				/>
			</div>
		</div>
	);
}

export function AnimatedCollapsible({
	children,
	className,
	contentClassName,
	open,
}: {
	children: ReactNode;
	className?: string;
	contentClassName?: string;
	open: boolean;
}) {
	const containerRef = useRef<HTMLDivElement | null>(null);
	const contentRef = useRef<HTMLDivElement | null>(null);
	const [mounted, setMounted] = useState(open);

	useEffect(() => {
		if (typeof window === "undefined") {
			setMounted(open);
			return;
		}

		if (open) {
			setMounted(true);
		}
	}, [open]);

	useLayoutEffect(() => {
		if (typeof window === "undefined" || !mounted) {
			return;
		}

		const container = containerRef.current;
		const content = contentRef.current;
		if (!container || !content) {
			return;
		}

		const prefersReducedMotion =
			typeof window.matchMedia === "function" &&
			window.matchMedia("(prefers-reduced-motion: reduce)").matches;
		const duration = prefersReducedMotion
			? 0
			: open
				? TEMPLATE_GROUP_EXPAND_DURATION_MS
				: TEMPLATE_GROUP_COLLAPSE_DURATION_MS;
		let frameA: number | null = null;
		let frameB: number | null = null;
		let timer: number | null = null;
		const fullHeight = `${content.scrollHeight}px`;

		container.style.overflow = "hidden";
		container.style.transitionProperty = "max-height, opacity";
		container.style.transitionDuration = `${duration}ms`;
		container.style.transitionTimingFunction = open
			? TEMPLATE_GROUP_EXPAND_EASING
			: TEMPLATE_GROUP_COLLAPSE_EASING;

		if (open) {
			container.style.maxHeight = "0px";
			container.style.opacity = "0";
			frameA = window.requestAnimationFrame(() => {
				frameB = window.requestAnimationFrame(() => {
					container.style.maxHeight = fullHeight;
					container.style.opacity = "1";
				});
			});
			timer = window.setTimeout(() => {
				container.style.maxHeight = "none";
				container.style.opacity = "1";
			}, duration);
		} else {
			container.style.maxHeight = fullHeight;
			container.style.opacity = "1";
			frameA = window.requestAnimationFrame(() => {
				container.style.maxHeight = "0px";
				container.style.opacity = "0";
			});
			timer = window.setTimeout(() => {
				setMounted(false);
			}, duration);
		}

		return () => {
			if (frameA !== null) {
				window.cancelAnimationFrame(frameA);
			}
			if (frameB !== null) {
				window.cancelAnimationFrame(frameB);
			}
			if (timer !== null) {
				window.clearTimeout(timer);
			}
		};
	}, [mounted, open]);

	if (!mounted) {
		return null;
	}

	return (
		<div
			ref={containerRef}
			aria-hidden={!open}
			className={cn("overflow-hidden", className)}
		>
			<div ref={contentRef} className={cn("min-h-0", contentClassName)}>
				{children}
			</div>
		</div>
	);
}

export function sortConfigsByKey(a: SystemConfig, b: SystemConfig) {
	return a.key.localeCompare(b.key);
}

export function buildDraftValues(configs: SystemConfig[]) {
	return Object.fromEntries(
		configs.map((config) => [
			config.key,
			Array.isArray(config.value) ? [...config.value] : config.value,
		]),
	) as DraftValues;
}
