export type SupportedArchiveFormat = "zip" | "7z";

type ArchiveFileLike = {
	name: string;
	mime_type?: string | null;
};

const archiveFormatByExtension: Record<string, SupportedArchiveFormat> = {
	zip: "zip",
	"7z": "7z",
};

const archivePreviewFormatByMime: Record<string, SupportedArchiveFormat> = {
	"application/zip": "zip",
	"application/x-zip-compressed": "zip",
	"application/x-7z": "7z",
	"application/x-7z-compressed": "7z",
};

function fileExtension(name: string) {
	const lower = name.trim().toLowerCase();
	const dot = lower.lastIndexOf(".");
	if (dot < 0) return "";
	return lower.slice(dot + 1);
}

export function isSupportedArchiveFormat(
	format: string,
): format is SupportedArchiveFormat {
	return Object.values(archiveFormatByExtension).includes(
		format as SupportedArchiveFormat,
	);
}

export function detectArchiveFormatByName(
	fileName: string,
): SupportedArchiveFormat | null {
	return archiveFormatByExtension[fileExtension(fileName)] ?? null;
}

export function isExtractableArchiveFileName(fileName: string) {
	return detectArchiveFormatByName(fileName) !== null;
}

export function isSupportedArchivePreviewFile(file: ArchiveFileLike) {
	const extensionFormat = detectArchiveFormatByName(file.name);
	if (extensionFormat) return true;

	const mime = file.mime_type?.toLowerCase() ?? "";
	return archivePreviewFormatByMime[mime] != null;
}
