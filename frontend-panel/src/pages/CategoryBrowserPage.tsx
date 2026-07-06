import {
	type ComponentProps,
	useCallback,
	useEffect,
	useMemo,
	useReducer,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";
import type { FileBrowserContextValue } from "@/components/files/FileBrowserContext";
import { FILE_BROWSER_BATCH_ACTION_POLICIES } from "@/components/files/fileActionPolicy";
import { getImagePreviewNavigation } from "@/components/files/preview/navigation/imagePreviewNavigation";
import { TagLibraryManagerDialog } from "@/components/files/TagLibraryManagerDialog";
import { TagManagerDialog } from "@/components/files/TagManagerDialog";
import { AppLayout } from "@/components/layout/AppLayout";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useSelectionShortcuts } from "@/hooks/useSelectionShortcuts";
import { startAuthenticatedDownload } from "@/lib/authenticatedDownload";
import { subscribeStorageChange } from "@/lib/storageChangeBus";
import {
	beginLocalStorageDeleteMutation,
	decideVirtualStorageViewRefresh,
} from "@/lib/storageMutationCoordinator";
import {
	FILE_CATEGORY_BY_ROUTE_SEGMENT,
	workspaceFolderPath,
	workspaceRootPath,
} from "@/lib/workspace";
import { FileBrowserDialogs } from "@/pages/file-browser/FileBrowserDialogs";
import { FileBrowserToolbar } from "@/pages/file-browser/FileBrowserToolbar";
import { FileBrowserWorkspace } from "@/pages/file-browser/FileBrowserWorkspace";
import type {
	FileBrowserInfoTarget,
	FileBrowserPreviewState,
	FileBrowserShareTarget,
	FileBrowserVersionTarget,
} from "@/pages/file-browser/types";
import { useFileBrowserBatchActions } from "@/pages/file-browser/useFileBrowserBatchActions";
import { useMediaQuery } from "@/pages/file-browser/useMediaQuery";
import { batchService } from "@/services/batchService";
import { fileService } from "@/services/fileService";
import { searchService } from "@/services/searchService";
import { useFileStore } from "@/stores/fileStore";
import { usePreviewAppStore } from "@/stores/previewAppStore";
import { useThumbnailSupportStore } from "@/stores/thumbnailSupportStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { FileCategory, FileInfo, FileListItem } from "@/types/api";

const CATEGORY_PAGE_LIMIT = 100;

interface CategoryResultsState {
	error: string | null;
	files: FileListItem[];
	loading: boolean;
	loadingMore: boolean;
	totalFiles: number;
}

type CategoryResultsAction =
	| { type: "load-start"; mode: "replace" | "append" }
	| {
			type: "load-success";
			files: FileListItem[];
			mode: "replace" | "append";
			totalFiles: number;
	  }
	| { type: "load-error"; error: string }
	| { type: "load-empty" };

const CATEGORY_RESULTS_INITIAL_STATE: CategoryResultsState = {
	error: null,
	files: [],
	loading: true,
	loadingMore: false,
	totalFiles: 0,
};

function categoryResultsReducer(
	state: CategoryResultsState,
	action: CategoryResultsAction,
): CategoryResultsState {
	switch (action.type) {
		case "load-start":
			return action.mode === "append"
				? { ...state, loadingMore: true }
				: {
						...CATEGORY_RESULTS_INITIAL_STATE,
						loading: true,
					};
		case "load-success":
			return {
				error: null,
				files:
					action.mode === "append"
						? [...state.files, ...action.files]
						: action.files,
				loading: false,
				loadingMore: false,
				totalFiles: action.totalFiles,
			};
		case "load-error":
			return {
				...state,
				error: action.error,
				loading: false,
				loadingMore: false,
			};
		case "load-empty":
			return {
				...CATEGORY_RESULTS_INITIAL_STATE,
				loading: false,
			};
		default:
			return state;
	}
}

function getCategoryLabelKey(category: FileCategory) {
	return `search:category_${category}`;
}

export default function CategoryBrowserPage() {
	const { t } = useTranslation(["core", "files", "search", "tasks"]);
	const params = useParams<{ category?: string }>();
	const navigate = useNavigate();
	const workspace = useWorkspaceStore((s) => s.workspace);
	const category = params.category
		? FILE_CATEGORY_BY_ROUTE_SEGMENT[params.category]
		: undefined;
	const categoryLabel = category ? t(getCategoryLabelKey(category)) : "";
	const pageTitle = category
		? t("search:category_view_title", { category: categoryLabel })
		: t("core:all_files");
	const searchErrorText = t("search:search_error");
	const isCompactBreadcrumb = useMediaQuery("(max-width: 639px)");
	const browserOpenMode = useFileStore((s) => s.browserOpenMode);
	const viewMode = useFileStore((s) => s.viewMode);
	const setViewMode = useFileStore((s) => s.setViewMode);
	const sortBy = useFileStore((s) => s.sortBy);
	const sortOrder = useFileStore((s) => s.sortOrder);
	const setSortBy = useFileStore((s) => s.setSortBy);
	const setSortOrder = useFileStore((s) => s.setSortOrder);
	const clearSelection = useFileStore((s) => s.clearSelection);
	const selectItems = useFileStore((s) => s.selectItems);
	const previewAppsLoaded = usePreviewAppStore((s) => s.isLoaded);
	const loadPreviewApps = usePreviewAppStore((s) => s.load);
	const thumbnailSupport = useThumbnailSupportStore((s) => s.config);
	const [{ error, files, loading, loadingMore, totalFiles }, dispatchResults] =
		useReducer(categoryResultsReducer, CATEGORY_RESULTS_INITIAL_STATE);
	const [previewState, setPreviewState] =
		useState<FileBrowserPreviewState | null>(null);
	const [infoPanelOpen, setInfoPanelOpen] = useState(false);
	const [infoTarget, setInfoTarget] = useState<FileBrowserInfoTarget | null>(
		null,
	);
	const [shareTarget, setShareTarget] = useState<FileBrowserShareTarget | null>(
		null,
	);
	const [versionTarget, setVersionTarget] =
		useState<FileBrowserVersionTarget | null>(null);
	const [tagManagerOpen, setTagManagerOpen] = useState(false);
	const [tagManagerTarget, setTagManagerTarget] =
		useState<ComponentProps<typeof TagManagerDialog>["target"]>(null);
	const [tagLibraryManagerOpen, setTagLibraryManagerOpen] = useState(false);
	const [scrollViewport, setScrollViewport] = useState<HTMLDivElement | null>(
		null,
	);
	const sentinelRef = useRef<HTMLDivElement | null>(null);
	const requestIdRef = useRef(0);

	usePageTitle(pageTitle);
	const selectDisplayedFiles = useCallback(() => {
		selectItems(
			files.map((file) => file.id),
			[],
		);
	}, [files, selectItems]);
	useSelectionShortcuts({
		selectAll: selectDisplayedFiles,
		clearSelection,
		enabled: category != null,
	});

	useEffect(() => {
		if (!category) return;
		clearSelection();
	}, [category, clearSelection]);

	useEffect(() => {
		if (previewAppsLoaded) return;
		void loadPreviewApps();
	}, [loadPreviewApps, previewAppsLoaded]);

	const loadCategory = useCallback(
		async (offset: number, mode: "replace" | "append") => {
			if (!category) return;
			const requestId = requestIdRef.current + 1;
			requestIdRef.current = requestId;
			dispatchResults({ type: "load-start", mode });

			try {
				const results = await searchService.search({
					type: "file",
					category,
					sort_by: sortBy,
					sort_order: sortOrder,
					limit: CATEGORY_PAGE_LIMIT,
					offset,
				});
				if (requestIdRef.current === requestId) {
					dispatchResults({
						type: "load-success",
						files: results.files,
						mode,
						totalFiles: results.total_files,
					});
				}
			} catch (loadError) {
				if (requestIdRef.current === requestId) {
					dispatchResults({
						type: "load-error",
						error:
							loadError instanceof Error ? loadError.message : searchErrorText,
					});
				}
			}
		},
		[category, searchErrorText, sortBy, sortOrder],
	);

	useEffect(() => {
		if (!category) return;
		setInfoPanelOpen(false);
		setInfoTarget(null);
		void loadCategory(0, "replace");
	}, [category, loadCategory]);

	useEffect(() => {
		if (!category) return;
		return subscribeStorageChange((event) => {
			if (
				!decideVirtualStorageViewRefresh(event, {
					currentWorkspace: workspace,
					view: "category",
				})
			) {
				return;
			}
			void loadCategory(0, "replace");
		});
	}, [category, loadCategory, workspace]);

	const activeInfoTarget = useMemo<FileBrowserInfoTarget | null>(() => {
		if (!infoTarget?.file) return null;
		const nextFile = files.find((entry) => entry.id === infoTarget.file?.id);
		return nextFile ? { file: nextFile } : null;
	}, [files, infoTarget]);

	const hasMoreFiles = files.length < totalFiles;
	useEffect(() => {
		if (!hasMoreFiles || loading || loadingMore) return;
		const sentinel = sentinelRef.current;
		if (!sentinel) return;

		const observer = new IntersectionObserver(
			(entries) => {
				if (entries[0]?.isIntersecting) {
					void loadCategory(files.length, "append");
				}
			},
			{ root: scrollViewport, rootMargin: "200px" },
		);
		observer.observe(sentinel);
		return () => observer.disconnect();
	}, [
		files.length,
		hasMoreFiles,
		loadCategory,
		loading,
		loadingMore,
		scrollViewport,
	]);

	const handleDownload = useCallback((fileId: number, _fileName: string) => {
		void startAuthenticatedDownload(fileService.downloadPath(fileId)).catch(
			handleApiError,
		);
	}, []);

	const handleArchiveDownload = useCallback(
		(fileIds: number[], folderIds: number[]) =>
			batchService.streamArchiveDownload(fileIds, folderIds),
		[],
	);

	const { dialogs: batchActionDialogs, selectionToolbar } =
		useFileBrowserBatchActions({
			...FILE_BROWSER_BATCH_ACTION_POLICIES.virtual,
			displayFiles: files,
			displayFolders: [],
			onChanged: () => loadCategory(0, "replace"),
			onArchiveDownload: handleArchiveDownload,
			onDownload: handleDownload,
		});

	const openPreview = useCallback(
		(file: FileListItem, openMode: "auto" | "direct" | "picker") => {
			setPreviewState({ file, openMode });
		},
		[],
	);

	const navigatePreviewFile = useCallback((file: FileInfo | FileListItem) => {
		setPreviewState((current) =>
			current ? { ...current, file } : { file, openMode: "auto" },
		);
	}, []);

	const handleShare = useCallback((target: FileBrowserShareTarget) => {
		setShareTarget(target);
	}, []);

	const handleInfo = useCallback(
		(type: "file" | "folder", id: number) => {
			if (type !== "file") return;
			const file = files.find((entry) => entry.id === id);
			if (!file) return;
			setInfoTarget({ file });
			setInfoPanelOpen(true);
		},
		[files],
	);

	const handleManageTags = useCallback(
		(type: "file" | "folder", id: number) => {
			if (type !== "file") return;
			const file = files.find((entry) => entry.id === id);
			if (!file) return;

			setTagManagerTarget({
				mode: "entity",
				entityId: file.id,
				entityType: "file",
				initialTags: file.tags ?? [],
				name: file.name,
				onChanged: () => loadCategory(0, "replace"),
			});
			setTagManagerOpen(true);
		},
		[files, loadCategory],
	);

	const handleToggleLock = useCallback(
		async (type: "file" | "folder", id: number, locked: boolean) => {
			if (type !== "file") return false;
			try {
				await fileService.setFileLock(id, !locked);
				toast.success(
					!locked ? t("files:lock_success") : t("files:unlock_success"),
				);
				void loadCategory(0, "replace");
				return true;
			} catch (lockError) {
				handleApiError(lockError);
				return false;
			}
		},
		[loadCategory, t],
	);

	const handleDelete = useCallback(
		async (type: "file" | "folder", id: number) => {
			if (type !== "file") return;
			const mutation = beginLocalStorageDeleteMutation({
				workspace,
				fileIds: [id],
			});
			try {
				await fileService.deleteFile(id);
				toast.success(t("files:delete_success"));
				void loadCategory(0, "replace");
			} catch (deleteError) {
				mutation.rollback();
				handleApiError(deleteError);
			}
		},
		[loadCategory, t, workspace],
	);

	const handleVersions = useCallback(
		(fileId: number) => {
			const file = files.find((entry) => entry.id === fileId);
			if (!file) return;
			setVersionTarget({
				fileId,
				fileName: file.name,
				mimeType: file.mime_type,
			});
		},
		[files],
	);

	const handleGoToLocation = useCallback(
		async (file: FileListItem) => {
			try {
				const info = await fileService.getFile(file.id);
				navigate(workspaceFolderPath(workspace, info.folder_id ?? null), {
					viewTransition: false,
				});
			} catch (locationError) {
				handleApiError(locationError);
			}
		},
		[navigate, workspace],
	);

	const fileBrowserContextValue = useMemo<FileBrowserContextValue>(
		() => ({
			folders: [],
			files,
			browserOpenMode,
			breadcrumbPathIds: [],
			batchSelectionActions: selectionToolbar,
			onFolderOpen: () => undefined,
			onFileClick: (file) => openPreview(file, "auto"),
			onFileOpen: (file) => openPreview(file, "direct"),
			onFileChooseOpenMethod: (file) => openPreview(file, "picker"),
			onShare: handleShare,
			onDownload: handleDownload,
			onManageTags: handleManageTags,
			onGoToLocation: handleGoToLocation,
			onInfo: handleInfo,
			onToggleLock: handleToggleLock,
			onDelete: handleDelete,
			onVersions: handleVersions,
			fadingFileIds: new Set<number>(),
			fadingFolderIds: new Set<number>(),
		}),
		[
			browserOpenMode,
			files,
			handleDelete,
			handleDownload,
			handleGoToLocation,
			handleInfo,
			handleManageTags,
			handleShare,
			handleToggleLock,
			handleVersions,
			openPreview,
			selectionToolbar,
		],
	);

	const previewImageNavigation = useMemo(
		() =>
			previewState
				? getImagePreviewNavigation(files, previewState.file, thumbnailSupport)
				: {},
		[files, previewState, thumbnailSupport],
	);

	if (!category) {
		return <Navigate to={workspaceRootPath(workspace)} replace />;
	}

	return (
		<AppLayout>
			<FileBrowserToolbar
				breadcrumb={[{ id: null, name: categoryLabel }]}
				currentFolderActions="refresh-only"
				dragOverBreadcrumbIndex={null}
				isCompactBreadcrumb={isCompactBreadcrumb}
				isRootFolder
				isSearching={false}
				searchQuery={null}
				selectionToolbar={selectionToolbar}
				sortBy={sortBy}
				sortOrder={sortOrder}
				uploadReady={false}
				viewMode={viewMode}
				onBreadcrumbDragLeave={() => undefined}
				onBreadcrumbDragOver={() => undefined}
				onBreadcrumbDrop={async () => undefined}
				onCreateFile={() => undefined}
				onCreateFolder={() => undefined}
				onManageTagLibrary={() => setTagLibraryManagerOpen(true)}
				onNavigateToFolder={() => navigate(workspaceRootPath(workspace))}
				onOfflineDownload={() => undefined}
				onRefresh={() => loadCategory(0, "replace")}
				onSetSortBy={setSortBy}
				onSetSortOrder={setSortOrder}
				onSetViewMode={setViewMode}
				onTriggerFileUpload={() => undefined}
				onTriggerFolderUpload={() => undefined}
			/>
			<FileBrowserWorkspace
				breadcrumb={[{ id: null, name: categoryLabel }]}
				contentDragOver={false}
				currentFolderActions="refresh-only"
				emptyDescription={t("search:category_view_empty_desc", {
					category: categoryLabel,
				})}
				emptyTitle={t("search:category_view_empty", {
					category: categoryLabel,
				})}
				error={error}
				fileBrowserContextValue={fileBrowserContextValue}
				hasMoreFiles={hasMoreFiles}
				infoPanelOpen={infoPanelOpen && activeInfoTarget !== null}
				infoTarget={activeInfoTarget}
				isEmpty={!loading && files.length === 0}
				loading={loading}
				loadingMore={loadingMore}
				scrollViewport={scrollViewport}
				sentinelRef={sentinelRef}
				uploadReady={false}
				viewMode={viewMode}
				onContentDragLeave={() => undefined}
				onContentDragOver={(event) => event.preventDefault()}
				onContentDrop={async () => undefined}
				onCreateFile={() => undefined}
				onCreateFolder={() => undefined}
				onDownload={handleDownload}
				onInfoPanelOpenChange={setInfoPanelOpen}
				onOpenInfoFolder={() => undefined}
				onOfflineDownload={() => undefined}
				onPreview={(file) => setPreviewState({ file, openMode: "auto" })}
				onRefresh={() => loadCategory(0, "replace")}
				onRename={() => undefined}
				onScrollViewportRef={setScrollViewport}
				onShare={handleShare}
				onToggleLock={handleToggleLock}
				onTriggerFileUpload={() => undefined}
				onTriggerFolderUpload={() => undefined}
				onVersions={handleVersions}
			/>
			<TagManagerDialog
				open={tagManagerOpen}
				onOpenChange={setTagManagerOpen}
				target={tagManagerTarget}
			/>
			<TagLibraryManagerDialog
				open={tagLibraryManagerOpen}
				onOpenChange={setTagLibraryManagerOpen}
			/>
			{batchActionDialogs}
			<FileBrowserDialogs
				archiveTaskTarget={null}
				breadcrumb={[]}
				copyTarget={null}
				createFileOpen={false}
				createFolderOpen={false}
				currentFolderId={null}
				currentFolderName={null}
				folderPolicyTarget={null}
				moveTarget={null}
				offlineDownloadOpen={false}
				previewImageNavigation={previewImageNavigation}
				previewState={previewState}
				renameTarget={null}
				shareTarget={shareTarget}
				versionTarget={versionTarget}
				onArchiveTaskClose={() => undefined}
				onArchiveTaskSubmit={async () => undefined}
				onCopyClose={() => undefined}
				onCopyConfirm={async () => undefined}
				onCreateFileOpenChange={() => undefined}
				onCreateFolderOpenChange={() => undefined}
				onFolderPolicyClose={() => undefined}
				onMoveClose={() => undefined}
				onMoveConfirm={async () => undefined}
				onOfflineDownloadOpenChange={() => undefined}
				onPreviewClose={() => setPreviewState(null)}
				onPreviewFileUpdated={() => loadCategory(0, "replace")}
				onPreviewNavigate={navigatePreviewFile}
				onRenameClose={() => undefined}
				onShareClose={() => setShareTarget(null)}
				onVersionClose={() => setVersionTarget(null)}
				onVersionRestored={() => loadCategory(0, "replace")}
			/>
		</AppLayout>
	);
}
