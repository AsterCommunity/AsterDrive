import type { FileBrowserTrashMeta } from "@/components/files/FileBrowserContext";
import { getFileTypeInfo } from "@/components/files/preview/capabilities/file-capabilities";
import type { FileCategory as PreviewFileCategory } from "@/components/files/preview/capabilities/types";
import type {
	FileCategory,
	FileListItem,
	FolderListItem,
	TrashContents,
	TrashFileItem,
	TrashFolderItem,
} from "@/types/api";

/**
 * 回收站条目 → 文件浏览器列表项的映射。
 * trash 接口不返回 file_category/extension/tags/is_shared，
 * 这里在前端推导补齐，让回收站直接复用 FileGrid/FileTable（trashMode）。
 */

const PREVIEW_TO_API_CATEGORY: Record<PreviewFileCategory, FileCategory> = {
	image: "image",
	video: "video",
	audio: "audio",
	pdf: "document",
	markdown: "code",
	csv: "spreadsheet",
	tsv: "spreadsheet",
	json: "code",
	xml: "code",
	text: "code",
	archive: "archive",
	document: "document",
	spreadsheet: "spreadsheet",
	presentation: "presentation",
	unknown: "other",
};

function deriveFileCategory(mimeType: string, name: string): FileCategory {
	return PREVIEW_TO_API_CATEGORY[
		getFileTypeInfo({ mime_type: mimeType, name }).category
	];
}

function deriveExtension(name: string): string {
	const dot = name.lastIndexOf(".");
	if (dot <= 0 || dot === name.length - 1) return "";
	return name.slice(dot + 1).toLowerCase();
}

function byExpiresAtDesc(
	a: { expires_at: string },
	b: { expires_at: string },
): number {
	return new Date(b.expires_at).getTime() - new Date(a.expires_at).getTime();
}

export function toBrowserFolders(folders: TrashFolderItem[]): FolderListItem[] {
	return [...folders].sort(byExpiresAtDesc).map((folder) => ({
		id: folder.id,
		is_shared: false,
		lock_state: folder.lock_state,
		name: folder.name,
		tags: [],
		updated_at: folder.updated_at,
	}));
}

export function toBrowserFiles(files: TrashFileItem[]): FileListItem[] {
	return [...files].sort(byExpiresAtDesc).map((file) => ({
		id: file.id,
		is_shared: false,
		lock_state: file.lock_state,
		mime_type: file.mime_type,
		name: file.name,
		size: file.size,
		tags: [],
		updated_at: file.updated_at,
		extension: deriveExtension(file.name),
		file_category: deriveFileCategory(file.mime_type, file.name),
	}));
}

export function buildTrashMetaMap(
	contents: Pick<TrashContents, "files" | "folders">,
): Map<string, FileBrowserTrashMeta> {
	const map = new Map<string, FileBrowserTrashMeta>();
	for (const folder of contents.folders) {
		map.set(`folder:${folder.id}`, {
			expiresAt: folder.expires_at,
			originalPath: folder.original_path,
		});
	}
	for (const file of contents.files) {
		map.set(`file:${file.id}`, {
			expiresAt: file.expires_at,
			originalPath: file.original_path,
		});
	}
	return map;
}
