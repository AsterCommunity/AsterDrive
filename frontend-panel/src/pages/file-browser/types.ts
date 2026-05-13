import type { FileBrowserSelectionDownloadAction } from "@/components/files/FileBrowserContext";
import type {
	FileInfo,
	FileListItem,
	FolderInfo,
	FolderListItem,
} from "@/types/api";

export interface FileBrowserPreviewState {
	file: FileInfo | FileListItem;
	openMode: "auto" | "direct" | "picker";
}

export interface FileBrowserShareTarget {
	fileId?: number;
	folderId?: number;
	name: string;
	initialMode?: "page" | "direct";
}

export interface FileBrowserCopyTarget {
	type: "file" | "folder";
	id: number;
}

export interface FileBrowserMoveTarget {
	fileIds: number[];
	folderIds: number[];
}

export type FileBrowserArchiveTaskTarget =
	| {
			mode: "compress";
			fileIds: number[];
			folderIds: number[];
			initialName: string;
			clearSelectionOnSuccess: boolean;
	  }
	| {
			mode: "extract";
			fileId: number;
			initialName: string;
	  };

export interface FileBrowserVersionTarget {
	fileId: number;
	fileName: string;
	mimeType: string;
}

export interface FileBrowserRenameTarget {
	type: "file" | "folder";
	id: number;
	name: string;
}

export interface FileBrowserInfoTarget {
	file?: FileInfo | FileListItem;
	folder?: FolderInfo | FolderListItem;
}

export interface FileBrowserSelectionToolbarState {
	count: number;
	allDisplayedSelected: boolean;
	downloadAction?: FileBrowserSelectionDownloadAction;
	hasDisplayedItems: boolean;
	onArchiveCompress?: () => void;
	onClearSelection: () => void;
	onCopy: () => void;
	onDelete: () => void;
	onMove: () => void;
	onToggleDisplayedSelection: () => void;
}
