import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useParams, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { getImagePreviewNavigation } from "@/components/files/preview/imagePreviewNavigation";
import { TagLibraryManagerDialog } from "@/components/files/TagLibraryManagerDialog";
import { TagManagerDialog } from "@/components/files/TagManagerDialog";
import {
	UploadArea,
	type UploadAreaHandle,
} from "@/components/files/UploadArea";
import { AppLayout } from "@/components/layout/AppLayout";
import { handleApiError } from "@/hooks/useApiError";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { usePageTitle } from "@/hooks/usePageTitle";
import { startAuthenticatedDownload } from "@/lib/authenticatedDownload";
import { runWhenIdle } from "@/lib/idleTask";
import { workspaceKey } from "@/lib/workspace";
import { FileBrowserDialogs } from "@/pages/file-browser/FileBrowserDialogs";
import { FileBrowserToolbar } from "@/pages/file-browser/FileBrowserToolbar";
import { FileBrowserWorkspace } from "@/pages/file-browser/FileBrowserWorkspace";
import {
	FILE_BROWSER_LAZY_PRELOADERS,
	OfflineDownloadDialog as OfflineDownloadDialogPreloader,
} from "@/pages/file-browser/fileBrowserLazy";
import { useFileBrowserArchiveActions } from "@/pages/file-browser/useFileBrowserArchiveActions";
import { useFileBrowserBatchActions } from "@/pages/file-browser/useFileBrowserBatchActions";
import { useFileBrowserContextValue } from "@/pages/file-browser/useFileBrowserContextValue";
import { useFileBrowserDragAndDrop } from "@/pages/file-browser/useFileBrowserDragAndDrop";
import { useFileBrowserPageState } from "@/pages/file-browser/useFileBrowserPageState";
import { useMediaQuery } from "@/pages/file-browser/useMediaQuery";
import { fileService } from "@/services/fileService";
import { useFileStore } from "@/stores/fileStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";
import { useUploadAreaControlsStore } from "@/stores/uploadAreaControlsStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";

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
	const uploadPanelPresence = useUploadAreaControlsStore(
		(s) => s.uploadPanelPresence,
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
	const [uploadReady, setUploadReady] = useState(false);
	const [offlineDownloadOpen, setOfflineDownloadOpen] = useState(false);
	const [tagLibraryManagerOpen, setTagLibraryManagerOpen] = useState(false);
	const sentinelRef = useRef<HTMLDivElement | null>(null);
	const [scrollViewport, setScrollViewport] = useState<HTMLDivElement | null>(
		null,
	);

	useEffect(() => {
		return runWhenIdle(() => {
			for (const preloader of FILE_BROWSER_LAZY_PRELOADERS) {
				void preloader.preload();
			}
		});
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

	const handleDownload = useCallback((fileId: number, _fileName: string) => {
		void startAuthenticatedDownload(fileService.downloadPath(fileId)).catch(
			handleApiError,
		);
	}, []);

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
			onArchiveDownload: startArchiveDownload,
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
		async (targetFolderId: number | null) => {
			if (!moveTarget) return;
			await handleMoveToFolder(
				moveTarget.fileIds,
				moveTarget.folderIds,
				targetFolderId,
			);
			setMoveTarget(null);
		},
		[handleMoveToFolder, moveTarget, setMoveTarget],
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
			handleArchiveDownload,
			handleArchiveExtract,
			handleCopy,
			handleDelete,
			handleDownload,
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
	const bottomOverlayOffset = uploadPanelPresence.open
		? "expanded"
		: uploadPanelPresence.visible
			? "upload-compact"
			: selectionToolbar
				? "selection-compact"
				: "none";
	const handleUploadAreaReady = useCallback(
		(instance: UploadAreaHandle | null) => {
			uploadAreaRef.current = instance;
			setUploadReady(instance !== null);
		},
		[],
	);
	const handleScrollViewportRef = useCallback((node: HTMLDivElement | null) => {
		setScrollViewport(node);
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
		setOfflineDownloadOpen(true);
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
