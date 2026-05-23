import type { IconName } from "@/components/ui/icon";
import { absoluteAppUrl } from "@/lib/publicSiteUrl";
import type { PreviewLinkInfo } from "@/types/api";
import type { OpenWithOption, PreviewableFileLike } from "./types";

export type VideoBrowserMode = "iframe" | "new_tab";

export interface VideoBrowserConfig {
	label: string;
	mode: VideoBrowserMode;
	urlTemplate: string;
	allowedOrigins: string[];
}

export interface ResolvedVideoBrowserTarget {
	label: string;
	mode: VideoBrowserMode;
	url: string;
}

export interface UrlTemplatePreviewConfig {
	mode: VideoBrowserMode;
	urlTemplate: string;
	allowedOrigins: string[];
}

interface UrlTemplateTokenOverrides {
	filePreviewUrl?: string;
}

export interface VideoBrowserEnv {
	VITE_VIDEO_BROWSER_URL_TEMPLATE?: string;
	VITE_VIDEO_BROWSER_LABEL?: string;
	VITE_VIDEO_BROWSER_MODE?: string;
	VITE_VIDEO_BROWSER_ALLOWED_ORIGINS?: string;
}

export interface VideoBrowserFileContext extends PreviewableFileLike {
	id?: number;
	size?: number;
}

const DEFAULT_LABEL = "Custom Video Browser";
const TOKEN_PATTERN = /{{\s*([a-zA-Z0-9_]+)\s*}}/g;
const FILE_PREVIEW_URL_TOKENS = new Set(["file_preview_url"]);

function normalizeOrigins(value?: string) {
	return (value ?? "").split(",").flatMap((item) => {
		const origin = item.trim();
		if (!origin) {
			return [];
		}

		try {
			return [new URL(origin).origin];
		} catch {
			return [];
		}
	});
}

function normalizeOriginList(values: unknown) {
	if (!Array.isArray(values)) return [];
	return values.flatMap((value) => {
		if (typeof value !== "string") {
			return [];
		}

		const origin = value.trim();
		if (!origin) {
			return [];
		}

		try {
			return [new URL(origin).origin];
		} catch {
			return [];
		}
	});
}

export function parseVideoBrowserConfig(
	env: VideoBrowserEnv,
): VideoBrowserConfig | null {
	const urlTemplate = env.VITE_VIDEO_BROWSER_URL_TEMPLATE?.trim();
	if (!urlTemplate) return null;

	const label = env.VITE_VIDEO_BROWSER_LABEL?.trim() || DEFAULT_LABEL;
	const mode =
		env.VITE_VIDEO_BROWSER_MODE?.trim().toLowerCase() === "new_tab"
			? "new_tab"
			: "iframe";

	return {
		label,
		mode,
		urlTemplate,
		allowedOrigins: normalizeOrigins(env.VITE_VIDEO_BROWSER_ALLOWED_ORIGINS),
	};
}

const runtimeVideoBrowserConfig = parseVideoBrowserConfig({
	VITE_VIDEO_BROWSER_URL_TEMPLATE: import.meta.env
		.VITE_VIDEO_BROWSER_URL_TEMPLATE,
	VITE_VIDEO_BROWSER_LABEL: import.meta.env.VITE_VIDEO_BROWSER_LABEL,
	VITE_VIDEO_BROWSER_MODE: import.meta.env.VITE_VIDEO_BROWSER_MODE,
	VITE_VIDEO_BROWSER_ALLOWED_ORIGINS: import.meta.env
		.VITE_VIDEO_BROWSER_ALLOWED_ORIGINS,
});

export function getVideoBrowserConfig() {
	return runtimeVideoBrowserConfig;
}

export function getVideoBrowserOpenWithOption(
	config = runtimeVideoBrowserConfig,
): OpenWithOption | null {
	if (!config) return null;

	const icon: IconName = config.mode === "new_tab" ? "ArrowSquareOut" : "Globe";

	return {
		key: "videoBrowser",
		mode: "url_template",
		labelKey: "open_with_custom_video_browser",
		label: config.label,
		config: {
			allowed_origins: config.allowedOrigins,
			mode: config.mode,
			url_template: config.urlTemplate,
		},
		icon,
	};
}

export function parseUrlTemplatePreviewConfig(
	rawConfig: Record<string, unknown> | null | undefined,
): UrlTemplatePreviewConfig | null {
	if (!rawConfig) return null;

	const urlTemplate =
		typeof rawConfig.url_template === "string"
			? rawConfig.url_template.trim()
			: "";
	if (!urlTemplate) return null;

	const rawMode =
		typeof rawConfig.mode === "string"
			? rawConfig.mode.trim().toLowerCase()
			: "";

	return {
		mode: rawMode === "new_tab" ? "new_tab" : "iframe",
		urlTemplate,
		allowedOrigins: normalizeOriginList(rawConfig.allowed_origins),
	};
}

function collectTemplateTokens(template: string) {
	const tokens = new Set<string>();
	template.replace(TOKEN_PATTERN, (_match, token: string) => {
		tokens.add(token);
		return "";
	});
	return tokens;
}

function buildTokenMap(
	file: VideoBrowserFileContext,
	downloadPath: string,
	overrides: UrlTemplateTokenOverrides = {},
) {
	const origin =
		typeof window === "undefined" ? "http://localhost" : window.location.origin;
	const absoluteDownloadUrl = new URL(downloadPath, origin).toString();
	const filePreviewUrl = overrides.filePreviewUrl ?? "";

	return {
		file_id: file.id != null ? String(file.id) : "",
		file_name: file.name,
		mime_type: file.mime_type,
		size: file.size != null ? String(file.size) : "",
		download_path: downloadPath,
		download_url: absoluteDownloadUrl,
		file_preview_url: filePreviewUrl,
	};
}

function resolveTemplate(
	template: string,
	values: Record<string, string>,
): string {
	return template.replace(TOKEN_PATTERN, (_match, token: string) =>
		encodeURIComponent(values[token] ?? ""),
	);
}

export function resolveVideoBrowserTarget(
	file: VideoBrowserFileContext,
	downloadPath: string,
	config = runtimeVideoBrowserConfig,
	overrides?: UrlTemplateTokenOverrides,
): ResolvedVideoBrowserTarget | null {
	if (!config || typeof window === "undefined") return null;

	const resolvedUrl = resolveTemplate(
		config.urlTemplate,
		buildTokenMap(file, downloadPath, overrides),
	);

	let parsed: URL;
	try {
		parsed = new URL(resolvedUrl, window.location.origin);
	} catch {
		return null;
	}

	if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
		return null;
	}

	const isSameOrigin = parsed.origin === window.location.origin;
	const isAllowedOrigin = config.allowedOrigins.includes(parsed.origin);

	if (!isSameOrigin && !isAllowedOrigin) {
		return null;
	}

	return {
		label: config.label,
		mode: config.mode,
		url: parsed.toString(),
	};
}

export async function resolveUrlTemplateTarget(
	file: VideoBrowserFileContext,
	downloadPath: string,
	label: string,
	rawConfig: Record<string, unknown> | null | undefined,
	createPreviewLink?: (() => Promise<PreviewLinkInfo>) | undefined,
): Promise<ResolvedVideoBrowserTarget | null> {
	const config = parseUrlTemplatePreviewConfig(rawConfig);
	if (!config) return null;

	let filePreviewUrl: string | undefined;
	const templateTokens = collectTemplateTokens(config.urlTemplate);
	if (
		Array.from(templateTokens).some((token) =>
			FILE_PREVIEW_URL_TOKENS.has(token),
		)
	) {
		if (!createPreviewLink) {
			return null;
		}

		try {
			const previewLink = await createPreviewLink();
			filePreviewUrl = absoluteAppUrl(previewLink.path);
		} catch {
			return null;
		}
	}

	return resolveVideoBrowserTarget(
		file,
		downloadPath,
		{
			label,
			mode: config.mode,
			urlTemplate: config.urlTemplate,
			allowedOrigins: config.allowedOrigins,
		},
		{
			filePreviewUrl,
		},
	);
}
