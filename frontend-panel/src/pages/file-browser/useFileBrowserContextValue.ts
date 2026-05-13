import { useCallback, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import type { FileBrowserContextValue } from "@/components/files/FileBrowserContext";
import { workspaceFolderPath } from "@/lib/workspace";
import type { BreadcrumbItem, BrowserOpenMode } from "@/stores/fileStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { FileListItem, FolderListItem } from "@/types/api";
import type { FileBrowserSelectionToolbarState } from "./types";

interface UseFileBrowserContextValueOptions {
	breadcrumb: BreadcrumbItem[];
	browserOpenMode: BrowserOpenMode;
	displayFiles: FileListItem[];
	displayFolders: FolderListItem[];
	fadingFileIds: Set<number>;
	fadingFolderIds: Set<number>;
	selectionToolbar: FileBrowserSelectionToolbarState | null;
	handleArchiveCompress: (type: "file" | "folder", id: number) => void;
	handleArchiveDownload: (folderId: number) => void;
	handleArchiveExtract: (fileId: number) => void;
	handleCopy: (type: "file" | "folder", id: number) => void;
	handleDelete: (type: "file" | "folder", id: number) => Promise<void>;
	handleDownload: (fileId: number, fileName: string) => void;
	handleInfo: (type: "file" | "folder", id: number) => void;
	handleMove: (type: "file" | "folder", id: number) => void;
	handleMoveToFolder: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<void>;
	handleToggleLock: (
		type: "file" | "folder",
		id: number,
		locked: boolean,
	) => Promise<boolean>;
	handleVersions: (fileId: number) => void;
	openPreview: (
		file: FileListItem,
		openMode: "auto" | "direct" | "picker",
	) => void;
	openRenameDialog: (type: "file" | "folder", id: number, name: string) => void;
	openShareDialog: FileBrowserContextValue["onShare"];
}

export function useFileBrowserContextValue({
	breadcrumb,
	browserOpenMode,
	displayFiles,
	displayFolders,
	fadingFileIds,
	fadingFolderIds,
	selectionToolbar,
	handleArchiveCompress,
	handleArchiveDownload,
	handleArchiveExtract,
	handleCopy,
	handleDelete,
	handleDownload,
	handleInfo,
	handleMove,
	handleMoveToFolder,
	handleToggleLock,
	handleVersions,
	openPreview,
	openRenameDialog,
	openShareDialog,
}: UseFileBrowserContextValueOptions) {
	const navigate = useNavigate();
	const workspace = useWorkspaceStore((s) => s.workspace);

	const breadcrumbPathIds = useMemo(
		() =>
			breadcrumb
				.map((item) => item.id)
				.filter((id): id is number => id !== null),
		[breadcrumb],
	);

	const handleNavigateToFolder = useCallback(
		(targetFolderId: number | null, targetFolderName: string) => {
			navigate(
				workspaceFolderPath(workspace, targetFolderId, targetFolderName),
			);
		},
		[navigate, workspace],
	);

	const handleFolderOpen = useCallback(
		(id: number, name: string) => {
			handleNavigateToFolder(id, name);
		},
		[handleNavigateToFolder],
	);

	const handleFileClick = useCallback(
		(file: FileListItem) => openPreview(file, "auto"),
		[openPreview],
	);

	const handleFileOpen = useCallback(
		(file: FileListItem) => openPreview(file, "direct"),
		[openPreview],
	);

	const handleFileChooseOpenMethod = useCallback(
		(file: FileListItem) => openPreview(file, "picker"),
		[openPreview],
	);
	const batchSelectionActions = useMemo(
		() =>
			selectionToolbar
				? {
						count: selectionToolbar.count,
						onArchiveCompress: selectionToolbar.onArchiveCompress,
						onArchiveDownload: selectionToolbar.onArchiveDownload,
						onCopy: selectionToolbar.onCopy,
						onDelete: selectionToolbar.onDelete,
						onMove: selectionToolbar.onMove,
					}
				: null,
		[selectionToolbar],
	);

	const fileBrowserContextValue = useMemo<FileBrowserContextValue>(
		() => ({
			folders: displayFolders,
			files: displayFiles,
			browserOpenMode,
			breadcrumbPathIds,
			batchSelectionActions,
			onFolderOpen: handleFolderOpen,
			onFileClick: handleFileClick,
			onFileOpen: handleFileOpen,
			onFileChooseOpenMethod: handleFileChooseOpenMethod,
			onShare: openShareDialog,
			onDownload: handleDownload,
			onArchiveDownload: handleArchiveDownload,
			onArchiveCompress: handleArchiveCompress,
			onArchiveExtract: handleArchiveExtract,
			onCopy: handleCopy,
			onMove: handleMove,
			onToggleLock: handleToggleLock,
			onDelete: handleDelete,
			onRename: openRenameDialog,
			onVersions: handleVersions,
			onInfo: handleInfo,
			onMoveToFolder: handleMoveToFolder,
			fadingFileIds,
			fadingFolderIds,
		}),
		[
			displayFolders,
			displayFiles,
			browserOpenMode,
			breadcrumbPathIds,
			batchSelectionActions,
			handleFolderOpen,
			handleFileClick,
			handleFileOpen,
			handleFileChooseOpenMethod,
			openShareDialog,
			handleDownload,
			handleArchiveDownload,
			handleArchiveCompress,
			handleArchiveExtract,
			handleCopy,
			handleMove,
			handleToggleLock,
			handleDelete,
			openRenameDialog,
			handleVersions,
			handleInfo,
			handleMoveToFolder,
			fadingFileIds,
			fadingFolderIds,
		],
	);

	return {
		fileBrowserContextValue,
		handleNavigateToFolder,
	};
}
