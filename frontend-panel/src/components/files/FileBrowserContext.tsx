import { createContext, type ReactNode, use } from "react";
import type { BrowserOpenMode } from "@/stores/fileStore";
import type { FileListItem, FolderListItem } from "@/types/api";

export interface FileBrowserSelectionDownloadAction {
	kind: "file" | "selection" | "archive";
	onClick: () => void;
}

export interface FileBrowserShareTarget {
	fileId?: number;
	folderId?: number;
	name: string;
	initialMode?: "page" | "direct";
}

export interface FileBrowserBatchSelectionActions {
	count: number;
	downloadAction?: FileBrowserSelectionDownloadAction;
	onArchiveCompress?: () => void;
	onCopy?: () => void;
	onDelete?: () => void;
	onManageTags?: () => void;
	onMove?: () => void;
}

export interface FileBrowserContextValue {
	folders: FolderListItem[];
	files: FileListItem[];
	browserOpenMode: BrowserOpenMode;
	readOnly?: boolean;
	selectionEnabled?: boolean;
	breadcrumbPathIds: number[];
	batchSelectionActions?: FileBrowserBatchSelectionActions | null;
	getThumbnailPath?: (file: FileListItem) => string;
	onFolderOpen: (id: number, name: string) => void;
	onFileClick: (file: FileListItem) => void;
	onFileOpen?: (file: FileListItem) => void;
	onFileChooseOpenMethod?: (file: FileListItem) => void;
	onShare: (target: FileBrowserShareTarget) => void;
	onDownload: (fileId: number, fileName: string) => void;
	onArchiveDownload?: (folderId: number) => void;
	onArchiveCompress?: (type: "file" | "folder", id: number) => void;
	onArchiveExtract?: (fileId: number) => void;
	onCopy?: (type: "file" | "folder", id: number) => void;
	onManageTags?: (type: "file" | "folder", id: number) => void;
	onMove?: (type: "file" | "folder", id: number) => void;
	onFolderPolicy?: (folder: FolderListItem) => void;
	onGoToLocation?: (file: FileListItem) => void;
	onToggleLock: (
		type: "file" | "folder",
		id: number,
		locked: boolean,
	) => boolean | Promise<boolean> | undefined;
	onDelete: (type: "file" | "folder", id: number) => Promise<void> | undefined;
	onRename?: (type: "file" | "folder", id: number, name: string) => void;
	onVersions?: (fileId: number) => void;
	onInfo?: (type: "file" | "folder", id: number) => void;
	onMoveToFolder?: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => void | Promise<void>;
	fadingFileIds?: Set<number>;
	fadingFolderIds?: Set<number>;
}

const FileBrowserContext = createContext<FileBrowserContextValue | null>(null);

export function FileBrowserProvider({
	children,
	value,
}: {
	children: ReactNode;
	value: FileBrowserContextValue;
}) {
	return (
		<FileBrowserContext.Provider value={value}>
			{children}
		</FileBrowserContext.Provider>
	);
}

export function useFileBrowserContext() {
	const context = use(FileBrowserContext);

	if (context == null) {
		throw new Error(
			"useFileBrowserContext must be used within a FileBrowserProvider",
		);
	}

	return context;
}
