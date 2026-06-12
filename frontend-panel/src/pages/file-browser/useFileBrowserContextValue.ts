import { useCallback, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import type { FileBrowserContextValue } from "@/components/files/FileBrowserContext";
import {
	buildDirectMusicQueue,
	hydrateMusicQueueForPlayback,
	isMusicFile,
} from "@/lib/musicPlayer";
import { workspaceFolderPath } from "@/lib/workspace";
import type { BreadcrumbItem, BrowserOpenMode } from "@/stores/fileStore";
import { useMusicPlayerStore } from "@/stores/musicPlayerStore";
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
	handleFolderPolicy?: (folder: FolderListItem) => void;
	handleInfo: (type: "file" | "folder", id: number) => void;
	handleManageTags: (type: "file" | "folder", id: number) => void;
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
	handleFolderPolicy,
	handleInfo,
	handleManageTags,
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
	const playTracks = useMusicPlayerStore((s) => s.playTracks);

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

	const playFileAsMusic = useCallback(
		(file: FileListItem) => {
			const queue = buildDirectMusicQueue(displayFiles);
			const activeTrack = queue.find((track) => track.id === `file:${file.id}`);
			if (!activeTrack) return;

			void hydrateMusicQueueForPlayback(queue, activeTrack.id).then(
				(hydratedQueue) => {
					playTracks(hydratedQueue, activeTrack.id);
				},
			);
		},
		[displayFiles, playTracks],
	);

	const handleFileClick = useCallback(
		(file: FileListItem) => {
			if (isMusicFile(file)) {
				playFileAsMusic(file);
				return;
			}
			openPreview(file, "auto");
		},
		[openPreview, playFileAsMusic],
	);

	const handleFileOpen = useCallback(
		(file: FileListItem) => {
			if (isMusicFile(file)) {
				playFileAsMusic(file);
				return;
			}
			openPreview(file, "direct");
		},
		[openPreview, playFileAsMusic],
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
						downloadAction: selectionToolbar.downloadAction,
						onArchiveCompress: selectionToolbar.onArchiveCompress,
						onCopy: selectionToolbar.onCopy,
						onDelete: selectionToolbar.onDelete,
						onManageTags: selectionToolbar.onManageTags,
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
			onFolderPolicy: handleFolderPolicy,
			onArchiveDownload: handleArchiveDownload,
			onArchiveCompress: handleArchiveCompress,
			onArchiveExtract: handleArchiveExtract,
			onCopy: handleCopy,
			onManageTags: handleManageTags,
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
			handleFolderPolicy,
			handleArchiveDownload,
			handleArchiveCompress,
			handleArchiveExtract,
			handleCopy,
			handleManageTags,
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
