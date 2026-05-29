import {
	isSupportedArchiveFormat,
	type SupportedArchiveFormat,
} from "@/lib/archiveFormats";
import type { ArchivePreviewManifest } from "@/types/api";

type ArchivePreviewFormat = ArchivePreviewManifest["format"];

type ArchivePreviewFormatCapabilities = {
	filenameEncoding: boolean;
};

const defaultArchivePreviewFormatCapabilities: ArchivePreviewFormatCapabilities =
	{
		filenameEncoding: false,
	};

const pendingArchivePreviewFormatCapabilities: ArchivePreviewFormatCapabilities =
	{
		filenameEncoding: true,
	};

const archivePreviewFormatCapabilities = {
	zip: {
		filenameEncoding: true,
	},
	"7z": {
		filenameEncoding: false,
	},
} satisfies Record<SupportedArchiveFormat, ArchivePreviewFormatCapabilities>;

export function getArchivePreviewFormatCapabilities(
	format: ArchivePreviewFormat | null | undefined,
): ArchivePreviewFormatCapabilities {
	if (format == null) return pendingArchivePreviewFormatCapabilities;
	if (!isSupportedArchiveFormat(format)) {
		return defaultArchivePreviewFormatCapabilities;
	}
	return archivePreviewFormatCapabilities[format];
}
