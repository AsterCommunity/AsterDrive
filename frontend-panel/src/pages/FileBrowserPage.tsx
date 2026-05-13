import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useParams, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import {
	UploadArea,
	type UploadAreaHandle,
} from "@/components/files/UploadArea";
import { AppLayout } from "@/components/layout/AppLayout";
import { handleApiError } from "@/hooks/useApiError";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { usePageTitle } from "@/hooks/usePageTitle";
import { runWhenIdle } from "@/lib/idleTask";
import { FileBrowserDialogs } from "@/pages/file-browser/FileBrowserDialogs";
import { FileBrowserToolbar } from "@/pages/file-browser/FileBrowserToolbar";
import { FileBrowserWorkspace } from "@/pages/file-browser/FileBrowserWorkspace";
import { FILE_BROWSER_LAZY_PRELOADERS } from "@/pages/file-browser/fileBrowserLazy";
import { useFileBrowserArchiveActions } from "@/pages/file-browser/useFileBrowserArchiveActions";
import { useFileBrowserBatchActions } from "@/pages/file-browser/useFileBrowserBatchActions";
import { useFileBrowserContextValue } from "@/pages/file-browser/useFileBrowserContextValue";
import { useFileBrowserDragAndDrop } from "@/pages/file-browser/useFileBrowserDragAndDrop";
import { useFileBrowserPageState } from "@/pages/file-browser/useFileBrowserPageState";
import { useMediaQuery } from "@/pages/file-browser/useMediaQuery";
import { fileService } from "@/services/fileService";
import { useFileStore } from "@/stores/fileStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";

export default function FileBrowserPage() {
	const { t } = useTranslation(["files", "tasks"]);
	const params = useParams<{ folderId?: string }>();
	const [searchParams] = useSearchParams();
	const folderId = params.folderId ? Number(params.folderId) : null;
	const folderName = searchParams.get("name") ?? undefined;

	const navigateTo = useFileStore((s) => s.navigateTo);
	const refresh = useFileStore((s) => s.refresh);
	const moveToFolder = useFileStore((s) => s.moveToFolder);
	const search = useFileStore((s) => s.search);
	const previewAppsLoaded = usePreviewAppStore((s) => s.isLoaded);
	const loadPreviewApps = usePreviewAppStore((s) => s.load);
	const breadcrumb = useFileStore((s) => s.breadcrumb);
	const folders = useFileStore((s) => s.folders);
	const files = useFileStore((s) => s.files);
	const loading = useFileStore((s) => s.loading);
	const viewMode = useFileStore((s) => s.viewMode);
	const browserOpenMode = useFileStore((s) => s.browserOpenMode);
	const setViewMode = useFileStore((s) => s.setViewMode);
	const searchQuery = useFileStore((s) => s.searchQuery);
	const searchFolders = useFileStore((s) => s.searchFolders);
	const searchFiles = useFileStore((s) => s.searchFiles);
	const error = useFileStore((s) => s.error);
	const clearSelection = useFileStore((s) => s.clearSelection);
	const loadMoreFiles = useFileStore((s) => s.loadMoreFiles);
	const loadingMore = useFileStore((s) => s.loadingMore);
	const sortBy = useFileStore((s) => s.sortBy);
	const sortOrder = useFileStore((s) => s.sortOrder);
	const setSortBy = useFileStore((s) => s.setSortBy);
	const setSortOrder = useFileStore((s) => s.setSortOrder);
	const hasMoreFiles = useFileStore((s) => s.hasMoreFiles());

	const isSearching = searchQuery !== null;
	const displayFolders = isSearching ? searchFolders : folders;
	const displayFiles = isSearching ? searchFiles : files;
	const currentBreadcrumbItem = breadcrumb[breadcrumb.length - 1];
	const currentFolderName = currentBreadcrumbItem?.name;
	const isRootFolder =
		currentBreadcrumbItem != null
			? currentBreadcrumbItem.id == null
			: folderId == null;
	const isCompactBreadcrumb = useMediaQuery("(max-width: 639px)");
	const pageTitle = isSearching
		? `${t("core:search")}: ${searchQuery}`
		: folderId == null
			? t("core:all_files")
			: (currentFolderName ?? t("core:all_files"));

	usePageTitle(pageTitle);
	useKeyboardShortcuts();

	const uploadAreaRef = useRef<UploadAreaHandle | null>(null);
	const [uploadReady, setUploadReady] = useState(false);
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
		if (isSearching || !hasMoreFiles || loadingMore) return;
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
	}, [hasMoreFiles, isSearching, loadingMore, loadMoreFiles, scrollViewport]);

	const {
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
	} = useFileBrowserPageState({
		displayFiles,
		displayFolders,
		folderId,
		folderName,
		loadPreviewApps,
		navigateTo,
		previewAppsLoaded,
		refresh,
		t,
	});

	const handleDownload = useCallback((fileId: number, _fileName: string) => {
		const anchor = document.createElement("a");
		anchor.href = fileService.downloadUrl(fileId);
		anchor.download = "";
		anchor.click();
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
		isSearching,
		moveToFolder,
		refresh,
		search,
		searchQuery,
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
	const pageCore = (
		<>
			<FileBrowserToolbar
				breadcrumb={breadcrumb}
				dragOverBreadcrumbIndex={dragOverBreadcrumbIndex}
				isCompactBreadcrumb={isCompactBreadcrumb}
				isRootFolder={isRootFolder}
				isSearching={isSearching}
				searchQuery={searchQuery}
				selectionToolbar={selectionToolbar}
				sortBy={sortBy}
				sortOrder={sortOrder}
				viewMode={viewMode}
				onBreadcrumbDragLeave={handleBreadcrumbDragLeave}
				onBreadcrumbDragOver={handleBreadcrumbDragOver}
				onBreadcrumbDrop={handleBreadcrumbDrop}
				onNavigateToFolder={handleNavigateToFolder}
				onRefresh={refresh}
				onSetSortBy={setSortBy}
				onSetSortOrder={setSortOrder}
				onSetViewMode={setViewMode}
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
				isSearching={isSearching}
				loading={loading}
				loadingMore={loadingMore}
				scrollViewport={scrollViewport}
				sentinelRef={sentinelRef}
				uploadReady={uploadReady}
				viewMode={viewMode}
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

			<FileBrowserDialogs
				archiveTaskTarget={archiveTaskTarget}
				breadcrumb={breadcrumb}
				copyTarget={copyTarget}
				createFileOpen={createFileOpen}
				createFolderOpen={createFolderOpen}
				currentFolderId={folderId}
				moveTarget={moveTarget}
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
				onPreviewClose={() => setPreviewState(null)}
				onPreviewFileUpdated={refresh}
				onRenameClose={() => setRenameTarget(null)}
				onShareClose={() => setShareTarget(null)}
				onVersionClose={() => setVersionTarget(null)}
				onVersionRestored={handleVersionRestored}
			/>
		</AppLayout>
	);
}
