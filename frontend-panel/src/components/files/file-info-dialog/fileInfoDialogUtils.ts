import type {
	FileInfo,
	FileListItem,
	FolderInfo,
	FolderListItem,
} from "@/types/api";

export function hasFileDetails(
	file: FileInfo | FileListItem,
): file is FileInfo {
	return "blob_id" in file && "created_at" in file && "storage_used" in file;
}

export function hasFolderDetails(
	folder: FolderInfo | FolderListItem,
): folder is FolderInfo {
	return "created_at" in folder && "storage_used" in folder;
}

export function formatValueOrFallback(
	value: string | null | undefined,
	loading: boolean,
	loadingText: string,
) {
	if (value != null) {
		return value;
	}
	return loading ? loadingText : "—";
}
