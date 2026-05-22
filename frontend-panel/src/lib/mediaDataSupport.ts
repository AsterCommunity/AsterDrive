import type {
	FileInfo,
	FileListItem,
	MediaMetadataKind,
	PublicMediaDataSupport,
} from "@/types/api";

export type MediaDataFileLike = Pick<
	FileInfo | FileListItem,
	"mime_type" | "name"
> & {
	file_category?: string;
	size?: number;
};

export function getMediaDataExtension(fileName: string) {
	const trimmed = fileName.trim().toLowerCase();
	const dot = trimmed.lastIndexOf(".");
	if (dot <= 0 || dot === trimmed.length - 1) {
		return "";
	}
	return trimmed.slice(dot + 1);
}

export function mediaDataKindForFile(
	file: Pick<MediaDataFileLike, "file_category" | "mime_type">,
): MediaMetadataKind | null {
	if (
		file.file_category === "image" ||
		file.file_category === "audio" ||
		file.file_category === "video"
	) {
		return file.file_category;
	}
	const mimeType = file.mime_type.toLowerCase();
	if (mimeType.startsWith("image/")) return "image";
	if (mimeType.startsWith("audio/")) return "audio";
	if (mimeType.startsWith("video/")) return "video";
	return null;
}

function supportsConfiguredExtension(
	fileName: string,
	extensions: string[] | undefined,
) {
	const extension = getMediaDataExtension(fileName);
	if (!extension || !extensions?.length) {
		return false;
	}

	return extensions.some(
		(candidate) =>
			candidate.trim().replace(/^\./, "").toLowerCase() === extension,
	);
}

export function supportsMediaData(
	file: MediaDataFileLike,
	support: PublicMediaDataSupport | null | undefined,
) {
	if (!support?.enabled) return false;
	if (typeof file.size === "number" && file.size > support.max_source_bytes) {
		return false;
	}

	const kind = mediaDataKindForFile(file);
	if (!kind) return false;

	const kindSupport = support.kinds[kind];
	if (!kindSupport?.enabled) return false;
	if (kindSupport.match === "any") return true;
	return supportsConfiguredExtension(file.name, kindSupport.extensions);
}

export function supportsAudioMediaData(
	file: MediaDataFileLike,
	support: PublicMediaDataSupport | null | undefined,
) {
	return (
		mediaDataKindForFile(file) === "audio" && supportsMediaData(file, support)
	);
}
