import type { IconName } from "@/components/ui/icon";
import type {
	FileCategory,
	FileListItem,
	FolderListItem,
	SearchResults,
} from "@/types/api";

export type SearchFilter = "all" | "file" | "folder";
export type SearchCategoryFilter = FileCategory | null;

export type SearchEntry =
	| { key: string; kind: "folder"; item: FolderListItem }
	| { key: string; kind: "file"; item: FileListItem };

export interface SearchPreviewLocationState {
	searchPreviewFile?: FileListItem;
}

export const SEARCH_FILTER_OPTIONS: Array<{
	labelKey: string;
	value: SearchFilter;
}> = [
	{ value: "all", labelKey: "all" },
	{ value: "file", labelKey: "files_only" },
	{ value: "folder", labelKey: "folders_only" },
];

export const SEARCH_CATEGORY_OPTIONS: Array<{
	icon: IconName;
	labelKey: string;
	value: FileCategory;
}> = [
	{ value: "image", labelKey: "category_image", icon: "FileImage" },
	{ value: "video", labelKey: "category_video", icon: "FileVideo" },
	{ value: "audio", labelKey: "category_audio", icon: "FileAudio" },
	{ value: "document", labelKey: "category_document", icon: "FileText" },
	{ value: "spreadsheet", labelKey: "category_spreadsheet", icon: "Table" },
	{
		value: "presentation",
		labelKey: "category_presentation",
		icon: "Presentation",
	},
	{ value: "archive", labelKey: "category_archive", icon: "FileZip" },
	{ value: "code", labelKey: "category_code", icon: "FileCode" },
	{ value: "other", labelKey: "category_other", icon: "Folder" },
];

export const EMPTY_RESULTS: SearchResults = {
	files: [],
	folders: [],
	total_files: 0,
	total_folders: 0,
};
