export const MEDIA_PROCESSING_CONFIG_KEY = "media_processing_registry_json";
export const MEDIA_PROCESSING_CONFIG_VERSION = 2;
export const MEDIA_PROCESSING_DEFAULT_VIPS_COMMAND = "vips";
export const MEDIA_PROCESSING_DEFAULT_FFMPEG_COMMAND = "ffmpeg";
export const MEDIA_PROCESSING_DEFAULT_FFPROBE_COMMAND = "ffprobe";
export const MEDIA_PROCESSING_DEFAULT_VIPS_EXTENSIONS = [
	"csv",
	"mat",
	"img",
	"hdr",
	"pbm",
	"pgm",
	"ppm",
	"pfm",
	"pnm",
	"svg",
	"svgz",
	"j2k",
	"jp2",
	"jpt",
	"j2c",
	"jpc",
	"gif",
	"png",
	"jpg",
	"jpeg",
	"jpe",
	"webp",
	"tif",
	"tiff",
	"fits",
	"fit",
	"fts",
	"exr",
	"jxl",
	"pdf",
	"heic",
	"heif",
	"avif",
	"svs",
	"vms",
	"vmu",
	"ndpi",
	"scn",
	"mrxs",
	"svslide",
	"bif",
	"raw",
] as const;
export const MEDIA_PROCESSING_DEFAULT_FFMPEG_EXTENSIONS = [
	"mp4",
	"m4v",
	"mov",
	"mkv",
	"webm",
	"avi",
	"mpg",
	"mpeg",
	"m2v",
	"ts",
	"m2ts",
	"mts",
	"3gp",
	"3g2",
	"ogv",
	"flv",
	"wmv",
] as const;
export const MEDIA_PROCESSING_DEFAULT_FFPROBE_EXTENSIONS =
	MEDIA_PROCESSING_DEFAULT_FFMPEG_EXTENSIONS;
export const MEDIA_PROCESSING_DEFAULT_LOFTY_EXTENSIONS = [
	"aac",
	"aiff",
	"aif",
	"ape",
	"flac",
	"m4a",
	"m4b",
	"m4p",
	"m4r",
	"mka",
	"mp3",
	"oga",
	"ogg",
	"opus",
	"wav",
	"wv",
] as const;
export const MEDIA_PROCESSING_PROCESSOR_ORDER = [
	"vips_cli",
	"ffmpeg_cli",
	"ffprobe_cli",
	"lofty",
	"images",
] as const satisfies readonly string[];
export type MediaProcessingEditorProcessorKind =
	(typeof MEDIA_PROCESSING_PROCESSOR_ORDER)[number];
export type MediaProcessingEditorUse =
	| "thumbnail:image"
	| "thumbnail:audio"
	| "thumbnail:video"
	| "metadata:image"
	| "metadata:audio"
	| "metadata:video";

export interface MediaProcessingEditorProcessorConfig {
	command: string;
}

export interface MediaProcessingEditorProcessor {
	config: MediaProcessingEditorProcessorConfig;
	enabled: boolean;
	extensions: string[];
	kind: MediaProcessingEditorProcessorKind;
	uses: MediaProcessingEditorUse[];
}

export interface MediaProcessingEditorConfig {
	processors: MediaProcessingEditorProcessor[];
	version: number;
}

export interface MediaProcessingValidationIssue {
	key: string;
	values?: Record<string, number | string>;
}

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

	return value
		.map((item) => readString(item).trim().replace(/^\./, "").toLowerCase())
		.filter(
			(item, index, items) => item.length > 0 && items.indexOf(item) === index,
		);
}

function readUseList(value: unknown) {
	if (!Array.isArray(value)) {
		return [];
	}

	return value
		.map((item) => readString(item).trim().toLowerCase())
		.filter(
			(item): item is MediaProcessingEditorUse =>
				item === "thumbnail:image" ||
				item === "thumbnail:audio" ||
				item === "thumbnail:video" ||
				item === "metadata:image" ||
				item === "metadata:audio" ||
				item === "metadata:video",
		)
		.filter(
			(item, index, items) => item.length > 0 && items.indexOf(item) === index,
		);
}

function normalizeUses(
	kind: MediaProcessingEditorProcessorKind,
	uses: MediaProcessingEditorUse[],
) {
	const normalized = uses.length > 0 ? [...uses] : [];
	for (const defaultUse of defaultUses(kind)) {
		if (!normalized.includes(defaultUse)) {
			normalized.push(defaultUse);
		}
	}
	return normalized;
}

function readProcessorKind(
	value: unknown,
): MediaProcessingEditorProcessorKind | "" {
	const normalized = readString(value).trim().toLowerCase();
	if (
		normalized === "images" ||
		normalized === "lofty" ||
		normalized === "vips_cli" ||
		normalized === "ffmpeg_cli" ||
		normalized === "ffprobe_cli"
	) {
		return normalized;
	}
	return "";
}

function defaultEnabled(kind: MediaProcessingEditorProcessorKind) {
	return kind === "images" || kind === "lofty";
}

function processorUsesCommand(kind: MediaProcessingEditorProcessorKind) {
	return kind === "vips_cli" || kind === "ffmpeg_cli" || kind === "ffprobe_cli";
}

function defaultCommand(kind: MediaProcessingEditorProcessorKind) {
	switch (kind) {
		case "vips_cli":
			return MEDIA_PROCESSING_DEFAULT_VIPS_COMMAND;
		case "ffmpeg_cli":
			return MEDIA_PROCESSING_DEFAULT_FFMPEG_COMMAND;
		case "ffprobe_cli":
			return MEDIA_PROCESSING_DEFAULT_FFPROBE_COMMAND;
		case "lofty":
		case "images":
			return "";
	}
}

export function defaultUses(
	kind: MediaProcessingEditorProcessorKind,
): MediaProcessingEditorUse[] {
	switch (kind) {
		case "vips_cli":
			return ["thumbnail:image"];
		case "ffmpeg_cli":
			return ["thumbnail:video"];
		case "ffprobe_cli":
			return ["metadata:video"];
		case "lofty":
			return ["thumbnail:audio", "metadata:audio"];
		case "images":
			return ["thumbnail:image", "metadata:image"];
	}
}

function defaultExtensions(kind: MediaProcessingEditorProcessorKind) {
	switch (kind) {
		case "vips_cli":
			return [...MEDIA_PROCESSING_DEFAULT_VIPS_EXTENSIONS];
		case "ffmpeg_cli":
			return [...MEDIA_PROCESSING_DEFAULT_FFMPEG_EXTENSIONS];
		case "ffprobe_cli":
			return [...MEDIA_PROCESSING_DEFAULT_FFPROBE_EXTENSIONS];
		case "lofty":
			return [...MEDIA_PROCESSING_DEFAULT_LOFTY_EXTENSIONS];
		case "images":
			return [];
	}
}

function createDefaultProcessor(
	kind: MediaProcessingEditorProcessorKind,
): MediaProcessingEditorProcessor {
	return {
		config: {
			command: defaultCommand(kind),
		},
		enabled: defaultEnabled(kind),
		extensions: defaultExtensions(kind),
		kind,
		uses: defaultUses(kind),
	};
}

function normalizeProcessor(
	value: unknown,
): MediaProcessingEditorProcessor | null {
	if (!isRecord(value)) {
		return null;
	}

	const kind = readProcessorKind(value.kind);
	if (!kind) {
		return null;
	}

	const runtimeConfig = isRecord(value.config) ? value.config : undefined;
	const uses = readUseList(value.uses);

	return {
		config: {
			command: processorUsesCommand(kind)
				? readString(runtimeConfig?.command).trim() || defaultCommand(kind)
				: "",
		},
		enabled: readBoolean(value.enabled, defaultEnabled(kind)),
		extensions: kind === "images" ? [] : readStringList(value.extensions),
		kind,
		uses: normalizeUses(kind, uses),
	};
}

function mergeProcessors(
	processors: MediaProcessingEditorProcessor[],
): MediaProcessingEditorProcessor[] {
	return MEDIA_PROCESSING_PROCESSOR_ORDER.map((kind) => {
		const matched = processors.find((processor) => processor.kind === kind);
		return matched ? { ...matched } : createDefaultProcessor(kind);
	});
}

export function parseMediaProcessingDelimitedInput(value: string) {
	return value
		.split(",")
		.map((item) => item.trim().replace(/^\./, "").toLowerCase())
		.filter(
			(item, index, items) => item.length > 0 && items.indexOf(item) === index,
		);
}

export function formatMediaProcessingDelimitedInput(values: string[]) {
	return values.join(", ");
}

export function parseMediaProcessingConfig(
	value: string,
): MediaProcessingEditorConfig {
	const parsed = JSON.parse(value) as unknown;
	if (!isRecord(parsed)) {
		throw new Error("media processing config must be an object");
	}

	const processors = Array.isArray(parsed.processors)
		? parsed.processors
				.map(normalizeProcessor)
				.filter((processor): processor is MediaProcessingEditorProcessor =>
					Boolean(processor),
				)
		: [];

	return {
		processors: mergeProcessors(processors),
		version:
			typeof parsed.version === "number"
				? parsed.version
				: MEDIA_PROCESSING_CONFIG_VERSION,
	};
}

export function serializeMediaProcessingConfig(
	config: MediaProcessingEditorConfig,
) {
	return JSON.stringify(
		{
			version: MEDIA_PROCESSING_CONFIG_VERSION,
			processors: mergeProcessors(config.processors).map((processor) => {
				const serialized = {
					enabled: processor.enabled,
					...(processor.kind !== "images" && processor.extensions.length > 0
						? { extensions: processor.extensions }
						: {}),
					kind: processor.kind,
					uses: processor.uses,
				} as Record<string, unknown>;
				if (processorUsesCommand(processor.kind)) {
					serialized.config = {
						command:
							processor.config.command.trim() || defaultCommand(processor.kind),
					};
				}
				return serialized;
			}),
		},
		null,
		2,
	);
}

export function getMediaProcessingConfigIssues(
	config: MediaProcessingEditorConfig,
): MediaProcessingValidationIssue[] {
	const issues: MediaProcessingValidationIssue[] = [];

	if (config.version !== MEDIA_PROCESSING_CONFIG_VERSION) {
		issues.push({
			key: "media_processing_error_version_mismatch",
			values: { version: MEDIA_PROCESSING_CONFIG_VERSION },
		});
	}

	if (
		!mergeProcessors(config.processors).some((processor) => processor.enabled)
	) {
		issues.push({ key: "media_processing_error_no_enabled_processors" });
	}

	return issues;
}

export function getMediaProcessingConfigIssuesFromString(value: string) {
	try {
		return getMediaProcessingConfigIssues(parseMediaProcessingConfig(value));
	} catch {
		return [{ key: "media_processing_error_parse" }];
	}
}
