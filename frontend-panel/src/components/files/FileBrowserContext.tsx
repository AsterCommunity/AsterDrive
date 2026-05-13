import { createContext, type ReactNode, useContext } from "react";
import type { BrowserOpenMode } from "@/stores/fileStore";
import type { FileListItem, FolderListItem } from "@/types/api";

export interface FileBrowserShareTarget {
	fileId?: number;
	folderId?: number;
	name: string;
	initialMode?: "page" | "direct";
}

export interface FileBrowserBatchSelectionActions {
	count: number;
	onArchiveCompress?: () => void;
	onArchiveDownload?: () => void;
	onCopy: () => void;
	onDelete: () => void;
	onMove: () => void;
}

export interface FileBrowserContextValue {
	folders: FolderListItem[];
	files: FileListItem[];
	browserOpenMode: BrowserOpenMode;
	breadcrumbPathIds: number[];
	batchSelectionActions?: FileBrowserBatchSelectionActions | null;
	onFolderOpen: (id: number, name: string) => void;
	onFileClick: (file: FileListItem) => void;
	onFileOpen?: (file: FileListItem) => void;
	onFileChooseOpenMethod?: (file: FileListItem) => void;
	onShare: (target: FileBrowserShareTarget) => void;
	onDownload: (fileId: number, fileName: string) => void;
	onArchiveDownload?: (folderId: number) => void;
	onArchiveCompress?: (type: "file" | "folder", id: number) => void;
	onArchiveExtract?: (fileId: number) => void;
	onCopy: (type: "file" | "folder", id: number) => void;
	onMove?: (type: "file" | "folder", id: number) => void;
	onToggleLock: (type: "file" | "folder", id: number, locked: boolean) => void;
	onDelete: (type: "file" | "folder", id: number) => void;
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
	const context = useContext(FileBrowserContext);

	if (context == null) {
		throw new Error(
			"useFileBrowserContext must be used within a FileBrowserProvider",
		);
	}

	return context;
}
