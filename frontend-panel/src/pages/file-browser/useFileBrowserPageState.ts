import type { TFunction } from "i18next";
import { useCallback, useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { handleApiError } from "@/hooks/useApiError";
import {
	BatchTargetFolderDialog,
	RenameDialog,
	ShareDialog,
	VersionHistoryDialog,
} from "@/pages/file-browser/fileBrowserLazy";
import type {
	FileBrowserCopyTarget,
	FileBrowserInfoTarget,
	FileBrowserMoveTarget,
	FileBrowserPreviewState,
	FileBrowserRenameTarget,
	FileBrowserShareTarget,
	FileBrowserVersionTarget,
} from "@/pages/file-browser/types";
import { fileService } from "@/services/fileService";
import { useFileStore } from "@/stores/fileStore";
import type { FileInfo, FileListItem, FolderListItem } from "@/types/api";

interface FileBrowserLocationState {
	searchPreviewFile?: FileListItem;
}

interface UseFileBrowserPageStateOptions {
	displayFiles: FileListItem[];
	displayFolders: FolderListItem[];
	loadPreviewApps: () => Promise<void>;
	navigationTarget: {
		folderId: number | null;
		folderName?: string;
		workspaceKey: string;
	};
	navigateTo: (folderId: number | null, folderName?: string) => Promise<void>;
	previewAppsLoaded: boolean;
	refresh: () => Promise<void>;
	t: TFunction;
}

export function useFileBrowserPageState({
	displayFiles,
	displayFolders,
	loadPreviewApps,
	navigationTarget,
	navigateTo,
	previewAppsLoaded,
	refresh,
	t,
}: UseFileBrowserPageStateOptions) {
	const location = useLocation();
	const navigate = useNavigate();
	const [createFolderOpen, setCreateFolderOpen] = useState(false);
	const [createFileOpen, setCreateFileOpen] = useState(false);
	const [previewState, setPreviewState] =
		useState<FileBrowserPreviewState | null>(null);
	const [shareTarget, setShareTarget] = useState<FileBrowserShareTarget | null>(
		null,
	);
	const [copyTarget, setCopyTarget] = useState<FileBrowserCopyTarget | null>(
		null,
	);
	const [moveTarget, setMoveTarget] = useState<FileBrowserMoveTarget | null>(
		null,
	);
	const [versionTarget, setVersionTarget] =
		useState<FileBrowserVersionTarget | null>(null);
	const [renameTarget, setRenameTarget] =
		useState<FileBrowserRenameTarget | null>(null);
	const [infoPanelOpen, setInfoPanelOpen] = useState(false);
	const [infoTarget, setInfoTarget] = useState<FileBrowserInfoTarget | null>(
		null,
	);

	useEffect(() => {
		setInfoPanelOpen(false);
		setInfoTarget(null);
		navigateTo(navigationTarget.folderId, navigationTarget.folderName).catch(
			handleApiError,
		);
	}, [navigateTo, navigationTarget]);

	useEffect(() => {
		if (previewAppsLoaded) return;
		void loadPreviewApps();
	}, [loadPreviewApps, previewAppsLoaded]);

	useEffect(() => {
		if (!infoPanelOpen || infoTarget == null) {
			return;
		}

		if (infoTarget.file) {
			const nextFile = displayFiles.find(
				(entry) => entry.id === infoTarget.file?.id,
			);
			if (nextFile && nextFile !== infoTarget.file) {
				setInfoTarget({ file: nextFile });
			}
			return;
		}

		if (infoTarget.folder) {
			const nextFolder = displayFolders.find(
				(entry) => entry.id === infoTarget.folder?.id,
			);
			if (nextFolder && nextFolder !== infoTarget.folder) {
				setInfoTarget({ folder: nextFolder });
			}
		}
	}, [displayFiles, displayFolders, infoPanelOpen, infoTarget]);

	useEffect(() => {
		const locationState = location.state as FileBrowserLocationState | null;
		const previewFile = locationState?.searchPreviewFile;
		if (!previewFile) {
			return;
		}

		setPreviewState({ file: previewFile, openMode: "auto" });
		navigate(
			{
				pathname: location.pathname,
				search: location.search,
			},
			{
				replace: true,
				state: null,
			},
		);
	}, [location.pathname, location.search, location.state, navigate]);

	useEffect(() => {
		function onRenameRequest(event: Event) {
			const { type, id, name } = (event as CustomEvent).detail as {
				type: "file" | "folder";
				id: number;
				name: string;
			};
			void RenameDialog.preload();
			setRenameTarget({ type, id, name });
		}
		document.addEventListener("rename-request", onRenameRequest);
		return () =>
			document.removeEventListener("rename-request", onRenameRequest);
	}, []);

	const openPreview = useCallback(
		(file: FileInfo | FileListItem, openMode: "auto" | "direct" | "picker") => {
			setPreviewState({ file, openMode });
		},
		[],
	);

	const openShareDialog = useCallback((target: FileBrowserShareTarget) => {
		void ShareDialog.preload();
		setShareTarget(target);
	}, []);

	const openRenameDialog = useCallback(
		(type: "file" | "folder", id: number, name: string) => {
			void RenameDialog.preload();
			setRenameTarget({ type, id, name });
		},
		[],
	);

	const handleCopy = useCallback((type: "file" | "folder", id: number) => {
		void BatchTargetFolderDialog.preload();
		setCopyTarget({ type, id });
	}, []);

	const handleCopyConfirm = useCallback(
		async (targetFolderId: number | null) => {
			if (!copyTarget) return;
			try {
				if (copyTarget.type === "file") {
					await fileService.copyFile(copyTarget.id, targetFolderId);
				} else {
					await fileService.copyFolder(copyTarget.id, targetFolderId);
				}
				toast.success(t("copy_success"));
				setCopyTarget(null);
				await refresh();
			} catch (err) {
				handleApiError(err);
			}
		},
		[copyTarget, refresh, t],
	);

	const handleMove = useCallback((type: "file" | "folder", id: number) => {
		void BatchTargetFolderDialog.preload();
		setMoveTarget(
			type === "file"
				? { fileIds: [id], folderIds: [] }
				: { fileIds: [], folderIds: [id] },
		);
	}, []);

	const handleVersions = useCallback(
		(fileId: number) => {
			const targetFile = displayFiles.find((entry) => entry.id === fileId);
			if (!targetFile) return;
			void VersionHistoryDialog.preload();
			setVersionTarget({
				fileId,
				fileName: targetFile.name,
				mimeType: targetFile.mime_type,
			});
		},
		[displayFiles],
	);

	const handleInfo = useCallback(
		(type: "file" | "folder", id: number) => {
			if (type === "file") {
				const file = displayFiles.find((entry) => entry.id === id);
				if (file) {
					setInfoTarget({ file });
					setInfoPanelOpen(true);
				}
				return;
			}

			const folder = displayFolders.find((entry) => entry.id === id);
			if (folder) {
				setInfoTarget({ folder });
				setInfoPanelOpen(true);
			}
		},
		[displayFiles, displayFolders],
	);

	const handleDelete = useCallback(
		async (type: "file" | "folder", id: number) => {
			try {
				if (type === "file") await useFileStore.getState().deleteFile(id);
				else await useFileStore.getState().deleteFolder(id);
				toast.success(t("delete_success"));
			} catch (err) {
				handleApiError(err);
			}
		},
		[t],
	);

	const handleVersionRestored = useCallback(() => {
		setVersionTarget(null);
		void refresh();
	}, [refresh]);

	return {
		copyTarget,
		createFileOpen,
		createFolderOpen,
		handleCopy,
		handleCopyConfirm,
		handleDelete,
		handleInfo,
		handleMove,
		handleVersionRestored,
		handleVersions,
		infoPanelOpen,
		infoTarget,
		moveTarget,
		openPreview,
		openRenameDialog,
		openShareDialog,
		previewState,
		renameTarget,
		setCopyTarget,
		setCreateFileOpen,
		setCreateFolderOpen,
		setInfoPanelOpen,
		setMoveTarget,
		setPreviewState,
		setRenameTarget,
		setShareTarget,
		setVersionTarget,
		shareTarget,
		versionTarget,
	};
}
