import type { ArchiveFilenameEncoding } from "@/types/api";

const archiveFilenameEncodingOptionMap = {
	auto: true,
	utf8: true,
	gb18030: true,
	cp437: true,
	cp850: true,
	shift_jis: true,
	big5: true,
	euc_kr: true,
	windows_1252: true,
} satisfies Record<ArchiveFilenameEncoding, true>;

export const archiveFilenameEncodingOptions = Object.keys(
	archiveFilenameEncodingOptionMap,
) as ArchiveFilenameEncoding[];

export function isArchiveFilenameEncoding(
	value: string | null,
): value is ArchiveFilenameEncoding {
	return value !== null && value in archiveFilenameEncodingOptionMap;
}
