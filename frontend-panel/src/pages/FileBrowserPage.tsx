import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useParams, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import type { BatchTargetFolderSelection } from "@/components/files/BatchTargetFolderDialog";
import { getImagePreviewNavigation } from "@/components/files/preview/navigation/imagePreviewNavigation";
import { TagLibraryManagerDialog } from "@/components/files/TagLibraryManagerDialog";
import { TagManagerDialog } from "@/components/files/TagManagerDialog";
import {
	UploadArea,
	type UploadAreaHandle,
} from "@/components/files/UploadArea";
import { AppLayout } from "@/components/layout/AppLayout";
import { handleApiError } from "@/hooks/useApiError";
import { useBottomOverlayOffset } from "@/hooks/useBottomOverlayOffset";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { usePageTitle } from "@/hooks/usePageTitle";
import { formatBatchToast } from "@/lib/formatBatchToast";
import { runWhenIdle } from "@/lib/idleTask";
import { workspaceKey } from "@/lib/workspace";
import { FileBrowserDialogs } from "@/pages/file-browser/FileBrowserDialogs";
import { FileBrowserToolbar } from "@/pages/file-browser/FileBrowserToolbar";
import { FileBrowserWorkspace } from "@/pages/file-browser/FileBrowserWorkspace";
import {
	FILE_BROWSER_LAZY_PRELOADERS,
	FolderPolicyDialog as FolderPolicyDialogPreloader,
	OfflineDownloadDialog as OfflineDownloadDialogPreloader,
} from "@/pages/file-browser/fileBrowserLazy";
import { useFileBrowserArchiveActions } from "@/pages/file-browser/useFileBrowserArchiveActions";
import { useFileBrowserBatchActions } from "@/pages/file-browser/useFileBrowserBatchActions";
import { useFileBrowserContextValue } from "@/pages/file-browser/useFileBrowserContextValue";
import { useFileBrowserDragAndDrop } from "@/pages/file-browser/useFileBrowserDragAndDrop";
import { useFileBrowserPageState } from "@/pages/file-browser/useFileBrowserPageState";
import { useMediaQuery } from "@/pages/file-browser/useMediaQuery";
import {
	createBatchService,
	resolveMoveDispatch,
	singleOperationResult,
} from "@/services/batchService";
import { createFileService, fileService } from "@/services/fileService";
import { useAuthStore } from "@/stores/authStore";
import { requestDownloadSelection } from "@/stores/downloadStore";
import { useFileStore } from "@/stores/fileStore";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { FolderListItem } from "@/types/api";

interface FileBrowserPageUiState {
	folderPolicyTarget: FolderListItem | null;
	offlineDownloadOpen: boolean;
	scrollViewport: HTMLDivElement | null;
	tagLibraryManagerOpen: boolean;
	uploadReady: boolean;
}

type FileBrowserPageUiAction =
	| { type: "set_folder_policy_target"; target: FolderListItem | null }
	| { type: "set_offline_download_open"; open: boolean }
	| { type: "set_scroll_viewport"; viewport: HTMLDivElement | null }
	| { type: "set_tag_library_manager_open"; open: boolean }
	| { type: "set_upload_ready"; ready: boolean };

const initialPageUiState: FileBrowserPageUiState = {
	folderPolicyTarget: null,
	offlineDownloadOpen: false,
	scrollViewport: null,
	tagLibraryManagerOpen: false,
	uploadReady: false,
};

function fileBrowserPageUiReducer(
	state: FileBrowserPageUiState,
	action: FileBrowserPageUiAction,
): FileBrowserPageUiState {
	switch (action.type) {
		case "set_folder_policy_target":
			if (state.folderPolicyTarget === action.target) return state;
			return { ...state, folderPolicyTarget: action.target };
		case "set_offline_download_open":
			if (state.offlineDownloadOpen === action.open) return state;
			return { ...state, offlineDownloadOpen: action.open };
		case "set_scroll_viewport":
			if (state.scrollViewport === action.viewport) return state;
			return { ...state, scrollViewport: action.viewport };
		case "set_tag_library_manager_open":
			if (state.tagLibraryManagerOpen === action.open) return state;
			return { ...state, tagLibraryManagerOpen: action.open };
		case "set_upload_ready":
			if (state.uploadReady === action.ready) return state;
			return { ...state, uploadReady: action.ready };
	}
}

export default function FileBrowserPage() {
	const { t } = useTranslation(["files", "tasks"]);
	const params = useParams<{ folderId?: string }>();
	const [searchParams] = useSearchParams();
	const folderId = params.folderId ? Number(params.folderId) : null;
	const folderName = searchParams.get("name") ?? undefined;
	const currentWorkspaceKey = useWorkspaceStore((s) =>
		workspaceKey(s.workspace),
	);
	const navigationTarget = useMemo(
		() => ({
			folderId,
			folderName,
			workspaceKey: currentWorkspaceKey,
		}),
		[currentWorkspaceKey, folderId, folderName],
	);

	const navigateTo = useFileStore((s) => s.navigateTo);
	const refresh = useFileStore((s) => s.refresh);
	const moveToFolder = useFileStore((s) => s.moveToFolder);
	const previewAppsLoaded = usePreviewAppStore((s) => s.isLoaded);
	const loadPreviewApps = usePreviewAppStore((s) => s.load);
	const thumbnailSupport = useThumbnailSupportStore((s) => s.config);
	const breadcrumb = useFileStore((s) => s.breadcrumb);
	const folders = useFileStore((s) => s.folders);
	const files = useFileStore((s) => s.files);
	const loading = useFileStore((s) => s.loading);
	const viewMode = useFileStore((s) => s.viewMode);
	const browserOpenMode = useFileStore((s) => s.browserOpenMode);
	const setViewMode = useFileStore((s) => s.setViewMode);
	const error = useFileStore((s) => s.error);
	const clearSelection = useFileStore((s) => s.clearSelection);
	const loadMoreFiles = useFileStore((s) => s.loadMoreFiles);
	const loadingMore = useFileStore((s) => s.loadingMore);
	const sortBy = useFileStore((s) => s.sortBy);
	const sortOrder = useFileStore((s) => s.sortOrder);
	const setSortBy = useFileStore((s) => s.setSortBy);
	const setSortOrder = useFileStore((s) => s.setSortOrder);
	const hasMoreFiles = useFileStore((s) => s.hasMoreFiles());
	const isAdmin = useAuthStore((s) => s.user?.role === "admin");
	const archiveDownloadEnabled = useFrontendConfigStore(
		(state) => state.isLoaded && state.archiveDownloadUserEnabled,
	);

	const displayFolders = folders;
	const displayFiles = files;
	const currentBreadcrumbItem = breadcrumb[breadcrumb.length - 1];
	const currentFolderName = currentBreadcrumbItem?.name;
	const isRootFolder =
		currentBreadcrumbItem != null
			? currentBreadcrumbItem.id == null
			: folderId == null;
	const isCompactBreadcrumb = useMediaQuery("(max-width: 639px)");
	const pageTitle =
		folderId == null
			? t("core:all_files")
			: (currentFolderName ?? t("core:all_files"));

	usePageTitle(pageTitle);
	useKeyboardShortcuts();

	const uploadAreaRef = useRef<UploadAreaHandle | null>(null);
	const uploadReadyRef = useRef(false);
	const [pageUi, dispatchPageUi] = useReducer(
		fileBrowserPageUiReducer,
		initialPageUiState,
	);
	const sentinelRef = useRef<HTMLDivElement | null>(null);
	const {
		folderPolicyTarget,
		offlineDownloadOpen,
		scrollViewport,
		tagLibraryManagerOpen,
		uploadReady,
	} = pageUi;

	useEffect(() => {
		return runWhenIdle(() => {
			for (const preloader of FILE_BROWSER_LAZY_PRELOADERS) {
				void preloader.preload();
			}
		});
	}, []);

	useEffect(() => {
		return runWhenIdle(
			() => {
				void import("@/lib/pwaWarmup")
					.then(({ warmupPreviewEngines }) => {
						warmupPreviewEngines();
					})
					.catch(() => undefined);
			},
			{ fallbackDelayMs: 900, timeoutMs: 2_000 },
		);
	}, []);

	useEffect(() => {
		if (!hasMoreFiles || loadingMore) return;
		const el = sentinelRef.current;
		if (!el) return;
		const observer = new IntersectionObserver(
			(entries) => {
				if (entries[0].isIntersecting) {
					void loadMoreFiles();
				}
			},
			{ root: scrollViewport, rootMargin: "200px" },
		);
		observer.observe(el);
		return () => observer.disconnect();
	}, [hasMoreFiles, loadingMore, loadMoreFiles, scrollViewport]);

	const {
		copyTarget,
		createFileOpen,
		createFolderOpen,
		handleCopy,
		handleCopyConfirm,
		handleDelete,
		handleInfo,
		handleManageTags,
		handleMove,
		handleVersionRestored,
		handleVersions,
		infoPanelOpen,
		infoTarget,
		moveTarget,
		navigatePreviewFile,
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
		setTagManagerOpen,
		setVersionTarget,
		shareTarget,
		tagManagerOpen,
		tagManagerTarget,
		versionTarget,
	} = useFileBrowserPageState({
		displayFiles,
		displayFolders,
		loadPreviewApps,
		navigationTarget,
		navigateTo,
		previewAppsLoaded,
		refresh,
		t,
	});

	const handleDownload = useCallback(
		(fileId: number, fileName: string) => {
			const file = displayFiles.find((entry) => entry.id === fileId);
			requestDownloadSelection({
				workspace: useWorkspaceStore.getState().workspace,
				files: [{ id: fileId, name: fileName, size: file?.size }],
				folders: [],
			});
		},
		[displayFiles],
	);

	const handleToggleLock = useCallback(
		async (type: "file" | "folder", id: number, locked: boolean) => {
			try {
				if (type === "file") await fileService.setFileLock(id, !locked);
				else await fileService.setFolderLock(id, !locked);
				toast.success(!locked ? t("lock_success") : t("unlock_success"));
				refresh();
				return true;
			} catch (err) {
				handleApiError(err);
				return false;
			}
		},
		[refresh, t],
	);

	const {
		archiveTaskTarget,
		closeArchiveTask,
		handleArchiveCompress,
		handleArchiveDownload,
		handleArchiveExtract,
		handleBatchArchiveCompress,
		startArchiveDownload,
		submitArchiveTask,
	} = useFileBrowserArchiveActions({
		clearSelection,
		displayFiles,
		displayFolders,
		t,
	});
	const { dialogs: batchActionDialogs, selectionToolbar } =
		useFileBrowserBatchActions({
			displayFiles,
			displayFolders,
			onArchiveCompress: handleBatchArchiveCompress,
			onArchiveDownload: archiveDownloadEnabled
				? startArchiveDownload
				: undefined,
			onDownload: handleDownload,
		});

	const {
		contentDragOver,
		dragOverBreadcrumbIndex,
		fadingFileIds,
		fadingFolderIds,
		handleBreadcrumbDragLeave,
		handleBreadcrumbDragOver,
		handleBreadcrumbDrop,
		handleContentDragLeave,
		handleContentDragOver,
		handleContentDrop,
		handleMoveToFolder,
		handleTrashDrop,
	} = useFileBrowserDragAndDrop({
		breadcrumb,
		clearSelection,
		folderId,
		isSearching: false,
		moveToFolder,
		refresh,
		t,
	});

	const handleMoveConfirm = useCallback(
		async ({
			workspace: targetWorkspace,
			folderId: targetFolderId,
		}: BatchTargetFolderSelection) => {
			if (!moveTarget) return;
			try {
				const sourceWorkspace = moveTarget.sourceWorkspace;
				const sourceBatchService = createBatchService(sourceWorkspace);
				const sourceFileService = createFileService(sourceWorkspace);
				const result = await resolveMoveDispatch({
					currentWorkspace: sourceWorkspace,
					targetWorkspace,
					fileIds: moveTarget.fileIds,
					folderIds: moveTarget.folderIds,
					targetFolderId,
					dispatcher: {
						batchMove: sourceBatchService.batchMove,
						singleFileMove: (fileId, folderId) =>
							singleOperationResult(
								sourceFileService.moveFile(fileId, folderId),
							),
						singleFolderMove: (folderId, parentId) =>
							singleOperationResult(
								sourceFileService.moveFolder(folderId, parentId),
							),
						moveToWorkspace: sourceBatchService.moveToWorkspace,
					},
				});
				const batchToast = formatBatchToast(t, "move", result);
				if (batchToast.variant === "error") {
					toast.error(batchToast.title, {
						description: batchToast.description,
					});
				} else {
					toast.success(batchToast.title, {
						description: batchToast.description,
					});
				}
				setMoveTarget(null);
				await refresh();
			} catch (err) {
				handleApiError(err);
			}
		},
		[moveTarget, refresh, setMoveTarget, t],
	);
	const handleFolderPolicy = useCallback(
		(folder: FolderListItem) => {
			if (!isAdmin) return;
			void FolderPolicyDialogPreloader.preload();
			dispatchPageUi({ type: "set_folder_policy_target", target: folder });
		},
		[isAdmin],
	);
	const { fileBrowserContextValue, handleNavigateToFolder } =
		useFileBrowserContextValue({
			breadcrumb,
			browserOpenMode,
			displayFiles,
			displayFolders,
			fadingFileIds,
			fadingFolderIds,
			selectionToolbar,
			handleArchiveCompress,
			handleArchiveDownload: archiveDownloadEnabled
				? handleArchiveDownload
				: undefined,
			handleArchiveExtract,
			handleCopy,
			handleDelete,
			handleDownload,
			handleFolderPolicy: isAdmin ? handleFolderPolicy : undefined,
			handleInfo,
			handleManageTags,
			handleMove,
			handleMoveToFolder,
			handleToggleLock,
			handleVersions,
			openPreview,
			openRenameDialog,
			openShareDialog,
		});

	const isEmpty =
		!loading && displayFolders.length === 0 && displayFiles.length === 0;
	const bottomOverlayOffset = useBottomOverlayOffset(selectionToolbar !== null);
	const handleUploadAreaReady = useCallback(
		(instance: UploadAreaHandle | null) => {
			uploadAreaRef.current = instance;
			const ready = instance !== null;
			if (uploadReadyRef.current === ready) return;
			uploadReadyRef.current = ready;
			dispatchPageUi({ type: "set_upload_ready", ready });
		},
		[],
	);
	const handleScrollViewportRef = useCallback((node: HTMLDivElement | null) => {
		dispatchPageUi({ type: "set_scroll_viewport", viewport: node });
	}, []);
	const setTagLibraryManagerOpen = useCallback((open: boolean) => {
		dispatchPageUi({ type: "set_tag_library_manager_open", open });
	}, []);
	const setOfflineDownloadOpen = useCallback((open: boolean) => {
		dispatchPageUi({ type: "set_offline_download_open", open });
	}, []);
	const closeFolderPolicyDialog = useCallback(() => {
		dispatchPageUi({ type: "set_folder_policy_target", target: null });
	}, []);

	const previewImageNavigation = useMemo(
		() =>
			previewState
				? getImagePreviewNavigation(
						displayFiles,
						previewState.file,
						thumbnailSupport,
					)
				: {},
		[displayFiles, previewState, thumbnailSupport],
	);
	const openOfflineDownloadDialog = useCallback(() => {
		void OfflineDownloadDialogPreloader.preload();
		dispatchPageUi({ type: "set_offline_download_open", open: true });
	}, []);
	const pageCore = (
		<>
			<FileBrowserToolbar
				breadcrumb={breadcrumb}
				dragOverBreadcrumbIndex={dragOverBreadcrumbIndex}
				isCompactBreadcrumb={isCompactBreadcrumb}
				isRootFolder={isRootFolder}
				isSearching={false}
				searchQuery={null}
				selectionToolbar={selectionToolbar}
				sortBy={sortBy}
				sortOrder={sortOrder}
				uploadReady={uploadReady}
				viewMode={viewMode}
				onBreadcrumbDragLeave={handleBreadcrumbDragLeave}
				onBreadcrumbDragOver={handleBreadcrumbDragOver}
				onBreadcrumbDrop={handleBreadcrumbDrop}
				onCreateFile={() => setCreateFileOpen(true)}
				onCreateFolder={() => setCreateFolderOpen(true)}
				onManageTagLibrary={() => setTagLibraryManagerOpen(true)}
				onNavigateToFolder={handleNavigateToFolder}
				onOfflineDownload={openOfflineDownloadDialog}
				onRefresh={refresh}
				onSetSortBy={setSortBy}
				onSetSortOrder={setSortOrder}
				onSetViewMode={setViewMode}
				onTriggerFileUpload={() => uploadAreaRef.current?.triggerFileUpload()}
				onTriggerFolderUpload={() =>
					uploadAreaRef.current?.triggerFolderUpload()
				}
			/>
			<FileBrowserWorkspace
				breadcrumb={breadcrumb}
				contentDragOver={contentDragOver}
				error={error}
				fileBrowserContextValue={fileBrowserContextValue}
				hasMoreFiles={hasMoreFiles}
				infoPanelOpen={infoPanelOpen}
				infoTarget={infoTarget}
				isEmpty={isEmpty}
				loading={loading}
				loadingMore={loadingMore}
				scrollViewport={scrollViewport}
				sentinelRef={sentinelRef}
				uploadReady={uploadReady}
				viewMode={viewMode}
				bottomOverlayOffset={bottomOverlayOffset}
				onContentDragLeave={handleContentDragLeave}
				onContentDragOver={handleContentDragOver}
				onContentDrop={handleContentDrop}
				onCreateFile={() => setCreateFileOpen(true)}
				onCreateFolder={() => setCreateFolderOpen(true)}
				onDownload={handleDownload}
				onInfoPanelOpenChange={setInfoPanelOpen}
				onOpenInfoFolder={(targetFolder) =>
					handleNavigateToFolder(targetFolder.id, targetFolder.name)
				}
				onOfflineDownload={openOfflineDownloadDialog}
				onPreview={(targetFile) => openPreview(targetFile, "auto")}
				onRefresh={refresh}
				onRename={openRenameDialog}
				onScrollViewportRef={handleScrollViewportRef}
				onShare={openShareDialog}
				onToggleLock={handleToggleLock}
				onTriggerFileUpload={() => uploadAreaRef.current?.triggerFileUpload()}
				onTriggerFolderUpload={() =>
					uploadAreaRef.current?.triggerFolderUpload()
				}
				onVersions={handleVersions}
			/>
		</>
	);

	return (
		<AppLayout
			onTrashDrop={handleTrashDrop}
			onMoveToFolder={handleMoveToFolder}
		>
			<UploadArea ref={handleUploadAreaReady}>{pageCore}</UploadArea>
			{batchActionDialogs}
			<TagManagerDialog
				open={tagManagerOpen}
				onOpenChange={setTagManagerOpen}
				target={tagManagerTarget}
			/>
			<TagLibraryManagerDialog
				open={tagLibraryManagerOpen}
				onOpenChange={setTagLibraryManagerOpen}
			/>

			<FileBrowserDialogs
				archiveTaskTarget={archiveTaskTarget}
				breadcrumb={breadcrumb}
				copyTarget={copyTarget}
				createFileOpen={createFileOpen}
				createFolderOpen={createFolderOpen}
				currentFolderId={folderId}
				currentFolderName={currentFolderName}
				folderPolicyTarget={folderPolicyTarget}
				moveTarget={moveTarget}
				offlineDownloadOpen={offlineDownloadOpen}
				previewImageNavigation={previewImageNavigation}
				previewState={previewState}
				renameTarget={renameTarget}
				shareTarget={shareTarget}
				versionTarget={versionTarget}
				onArchiveTaskClose={closeArchiveTask}
				onArchiveTaskSubmit={submitArchiveTask}
				onCopyClose={() => setCopyTarget(null)}
				onCopyConfirm={handleCopyConfirm}
				onCreateFileOpenChange={setCreateFileOpen}
				onCreateFolderOpenChange={setCreateFolderOpen}
				onFolderPolicyClose={closeFolderPolicyDialog}
				onFolderPolicyUpdated={refresh}
				onMoveClose={() => setMoveTarget(null)}
				onMoveConfirm={handleMoveConfirm}
				onOfflineDownloadOpenChange={setOfflineDownloadOpen}
				onPreviewClose={() => setPreviewState(null)}
				onPreviewFileUpdated={refresh}
				onPreviewNavigate={navigatePreviewFile}
				onRenameClose={() => setRenameTarget(null)}
				onShareClose={() => setShareTarget(null)}
				onShareCreated={refresh}
				onVersionClose={() => setVersionTarget(null)}
				onVersionRestored={handleVersionRestored}
			/>
		</AppLayout>
	);
}
