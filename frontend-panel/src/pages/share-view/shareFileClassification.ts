import type { FileCategory } from "@/types/api";

const COMPOUND_EXTENSIONS = [
	"tar.gz",
	"tar.bz2",
	"tar.xz",
	"tar.zst",
	"tar.br",
	"tar.lz",
	"tar.lzma",
	"tar.lzo",
] as const;
const IMAGE_EXTENSIONS = new Set([
	"jpg",
	"jpeg",
	"png",
	"gif",
	"webp",
	"bmp",
	"tif",
	"tiff",
	"svg",
	"ico",
	"avif",
	"heic",
	"heif",
	"raw",
	"cr2",
	"nef",
	"orf",
	"rw2",
]);
const VIDEO_EXTENSIONS = new Set([
	"mp4",
	"m4v",
	"mov",
	"avi",
	"mkv",
	"webm",
	"flv",
	"wmv",
	"mpeg",
	"mpg",
	"3gp",
	"ts",
	"m2ts",
	"ogv",
]);
const AUDIO_EXTENSIONS = new Set([
	"mp3",
	"wav",
	"flac",
	"aac",
	"m4a",
	"ogg",
	"oga",
	"opus",
	"wma",
	"aiff",
	"alac",
	"mid",
	"midi",
]);
const DOCUMENT_EXTENSIONS = new Set([
	"pdf",
	"txt",
	"md",
	"markdown",
	"rtf",
	"doc",
	"docx",
	"odt",
	"pages",
	"epub",
	"tex",
]);
const SPREADSHEET_EXTENSIONS = new Set([
	"xls",
	"xlsx",
	"ods",
	"csv",
	"tsv",
	"numbers",
]);
const PRESENTATION_EXTENSIONS = new Set(["ppt", "pptx", "odp", "key"]);
const ARCHIVE_EXTENSIONS = new Set([
	"zip",
	"rar",
	"7z",
	"tar",
	"gz",
	"bz2",
	"xz",
	"zst",
	"br",
	"tgz",
	"tbz",
	"tbz2",
	"txz",
	"lz",
	"lzma",
	"lzo",
	"cab",
	"iso",
	"dmg",
]);
const CODE_EXTENSIONS = new Set([
	"rs",
	"ts",
	"tsx",
	"js",
	"jsx",
	"mjs",
	"cjs",
	"json",
	"jsonc",
	"yaml",
	"yml",
	"toml",
	"xml",
	"html",
	"htm",
	"css",
	"scss",
	"sass",
	"less",
	"sql",
	"sh",
	"bash",
	"zsh",
	"fish",
	"ps1",
	"py",
	"rb",
	"go",
	"java",
	"kt",
	"kts",
	"swift",
	"c",
	"h",
	"cpp",
	"cc",
	"cxx",
	"hpp",
	"cs",
	"php",
	"lua",
	"dart",
	"vue",
	"svelte",
	"lock",
	"ini",
	"conf",
	"dockerfile",
	"makefile",
]);

export function extensionFromName(name: string) {
	const trimmed = name.trim();
	const dot = trimmed.lastIndexOf(".");
	if (dot === -1) return trimmed.toLowerCase();
	if (dot === 0) return trimmed.slice(1).toLowerCase();
	if (dot + 1 >= trimmed.length) return "";
	return trimmed.slice(dot + 1).toLowerCase();
}

export function compoundExtensionFromName(name: string) {
	const normalized = name.trim().toLowerCase();
	return (
		COMPOUND_EXTENSIONS.find((extension) =>
			normalized.endsWith(`.${extension}`),
		) ?? null
	);
}

export function classifySharedFile(
	name: string,
	mimeType: string,
	compoundExtension: string | null,
): FileCategory {
	const extension = extensionFromName(name);
	const mime = mimeType.trim().toLowerCase();
	if (compoundExtension || ARCHIVE_EXTENSIONS.has(extension)) return "archive";
	if (SPREADSHEET_EXTENSIONS.has(extension)) return "spreadsheet";
	if (PRESENTATION_EXTENSIONS.has(extension)) return "presentation";
	if (CODE_EXTENSIONS.has(extension)) return "code";
	if (IMAGE_EXTENSIONS.has(extension)) return "image";
	if (VIDEO_EXTENSIONS.has(extension)) return "video";
	if (AUDIO_EXTENSIONS.has(extension)) return "audio";
	if (DOCUMENT_EXTENSIONS.has(extension)) return "document";
	if (mime.startsWith("image/")) return "image";
	if (mime.startsWith("video/")) return "video";
	if (mime.startsWith("audio/")) return "audio";
	if (
		mime.includes("spreadsheet") ||
		mime.includes("excel") ||
		mime.endsWith("/csv")
	) {
		return "spreadsheet";
	}
	if (mime.includes("presentation") || mime.includes("powerpoint")) {
		return "presentation";
	}
	if (
		mime.includes("zip") ||
		mime.includes("compressed") ||
		mime.includes("x-tar") ||
		mime.includes("x-7z") ||
		mime.includes("x-rar")
	) {
		return "archive";
	}
	if (mime.includes("json") || mime.includes("xml")) return "code";
	if (mime === "application/pdf" || mime.startsWith("text/")) return "document";
	return "other";
}
