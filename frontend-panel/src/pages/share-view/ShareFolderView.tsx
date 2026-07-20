import {
	type ReactNode,
	type RefObject,
	useCallback,
	useEffect,
	useMemo,
} from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/common/EmptyState";
import { SortMenu } from "@/components/common/SortMenu";
import { ToolbarBar } from "@/components/common/ToolbarBar";
import { ViewToggle } from "@/components/common/ViewToggle";
import {
	type FileBrowserContextValue,
	FileBrowserProvider,
} from "@/components/files/FileBrowserContext";
import { FileGrid } from "@/components/files/FileGrid";
import { FileSelectionToolbarTransition } from "@/components/files/FileSelectionToolbar";
import { FileTable } from "@/components/files/FileTable";
import { FolderBreadcrumb } from "@/components/files/FolderBreadcrumb";
import { FILE_BROWSER_BATCH_ACTION_POLICIES } from "@/components/files/fileActionPolicy";
import { Icon } from "@/components/ui/icon";
import { useSelectionShortcuts } from "@/hooks/useSelectionShortcuts";
import { getBottomOverlayPaddingClass } from "@/lib/constants";
import { cn } from "@/lib/utils";
import { useFileBrowserBatchActions } from "@/pages/file-browser/useFileBrowserBatchActions";
import { useMediaQuery } from "@/pages/file-browser/useMediaQuery";
import { shareService } from "@/services/shareService";
import { useFileStore } from "@/stores/fileStore";
import type { SortBy, SortOrder } from "@/stores/fileStore/types";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import type {
	FileInfo,
	FileListItem,
	FolderContents,
	SharePublicInfo,
} from "@/types/api";
import { ShareFolderContentSkeleton } from "./ShareFolderSkeleton";
import { ShareFolderPageShell } from "./ShareViewShell";
import type { ShareBreadcrumbItem } from "./types";

const noopShare = () => {};
const denyToggleLock = () => false;
const noopDelete = async () => {};

interface ShareFolderViewProps {
	breadcrumb: ShareBreadcrumbItem[];
	folderContents: FolderContents | null;
	hasMoreFiles: boolean;
	info: SharePublicInfo;
	loadingMore: boolean;
	navigating: boolean;
	previewElement: ReactNode;
	sentinelRef: RefObject<HTMLDivElement | null>;
	shareOwnerText: string;
	selectionShortcutsEnabled?: boolean;
	sortBy: SortBy;
	sortOrder: SortOrder;
	token: string;
	viewMode: "grid" | "list";
	onFileDownload: (file: FileListItem) => void;
	onFilePreview: (file: FileInfo | FileListItem) => void;
	onNavigateToFolder: (folderId: number | null, folderName?: string) => void;
	onRefresh: () => void | Promise<void>;
	onSortByChange: (sortBy: SortBy) => void;
	onSortOrderChange: (sortOrder: SortOrder) => void;
	onViewModeChange: (viewMode: "grid" | "list") => void;
}

export function ShareFolderView({
	breadcrumb,
	folderContents,
	hasMoreFiles,
	info,
	loadingMore,
	navigating,
	onFileDownload,
	onFilePreview,
	onNavigateToFolder,
	onRefresh,
	onSortByChange,
	onSortOrderChange,
	onViewModeChange,
	previewElement,
	sentinelRef,
	shareOwnerText,
	selectionShortcutsEnabled = true,
	sortBy,
	sortOrder,
	token,
	viewMode,
}: ShareFolderViewProps) {
	const { t } = useTranslation(["core", "share", "files", "errors"]);
	const clearSelection = useFileStore((state) => state.clearSelection);
	const selectItems = useFileStore((state) => state.selectItems);
	const archiveDownloadEnabled = useFrontendConfigStore(
		(state) => state.isLoaded && state.archiveDownloadShareEnabled,
	);
	const handleArchiveDownload = useCallback(
		(fileIds: number[], folderIds: number[]) =>
			shareService.streamArchiveDownload(token, fileIds, folderIds),
		[token],
	);
	const { dialogs: batchActionDialogs, selectionToolbar } =
		useFileBrowserBatchActions({
			...FILE_BROWSER_BATCH_ACTION_POLICIES.publicShare,
			displayFiles: folderContents?.files ?? [],
			displayFolders: folderContents?.folders ?? [],
			onArchiveDownload: archiveDownloadEnabled
				? handleArchiveDownload
				: undefined,
			onDownload: (fileId) => {
				const file = folderContents?.files.find((item) => item.id === fileId);
				if (file) onFileDownload(file);
			},
		});
	const breadcrumbIdsKey = useMemo(
		() => breadcrumb.map((item) => item.id ?? "root").join("/"),
		[breadcrumb],
	);
	const selectionScopeKey = `${token}:${breadcrumbIdsKey}`;
	const breadcrumbPathIds = useMemo(
		() =>
			breadcrumbIdsKey
				.split("/")
				.filter((id) => id !== "" && id !== "root")
				.map((id) => Number(id)),
		[breadcrumbIdsKey],
	);
	const isCompactBreadcrumb = useMediaQuery("(max-width: 639px)");
	const currentFolder = breadcrumb[breadcrumb.length - 1];
	const isRootFolder = currentFolder?.id == null;
	const selectAllDisplayed = useCallback(() => {
		selectItems(
			(folderContents?.files ?? []).map((file) => file.id),
			(folderContents?.folders ?? []).map((folder) => folder.id),
		);
	}, [folderContents, selectItems]);
	useSelectionShortcuts({
		selectAll: selectAllDisplayed,
		clearSelection,
		enabled: selectionShortcutsEnabled && folderContents != null && !navigating,
	});
	useEffect(() => {
		if (selectionScopeKey.length === 0) return;
		clearSelection();
	}, [clearSelection, selectionScopeKey]);
	const isFolderEmpty =
		folderContents != null &&
		folderContents.folders.length === 0 &&
		folderContents.files.length === 0;
	const fileBrowserContextValue =
		useMemo<FileBrowserContextValue | null>(() => {
			if (!folderContents) return null;

			return {
				folders: folderContents.folders,
				files: folderContents.files,
				browserOpenMode: "single_click",
				readOnly: true,
				selectionEnabled: true,
				batchSelectionActions: selectionToolbar
					? {
							count: selectionToolbar.count,
							downloadAction: selectionToolbar.downloadAction,
							onDelete: selectionToolbar.onDelete,
						}
					: null,
				breadcrumbPathIds,
				getThumbnailPath: (file) => `/s/${token}/files/${file.id}/thumbnail`,
				onArchiveDownload: archiveDownloadEnabled
					? (folderId) => handleArchiveDownload([], [folderId])
					: undefined,
				onFolderOpen: (id, name) => onNavigateToFolder(id, name),
				onFileClick: onFilePreview,
				onDownload: (fileId) => {
					const file = folderContents.files.find((item) => item.id === fileId);
					if (file) onFileDownload(file);
				},
				onShare: noopShare,
				onToggleLock: denyToggleLock,
				onDelete: noopDelete,
			};
		}, [
			archiveDownloadEnabled,
			breadcrumbPathIds,
			folderContents,
			handleArchiveDownload,
			onFileDownload,
			onFilePreview,
			onNavigateToFolder,
			selectionToolbar,
			token,
		]);

	return (
		<ShareFolderPageShell
			breadcrumb={breadcrumb}
			folderContents={folderContents}
			info={info}
			shareOwnerText={shareOwnerText}
			token={token}
			onNavigate={onNavigateToFolder}
		>
			{batchActionDialogs}
			<main className="flex min-h-0 flex-1 flex-col overflow-hidden">
				<FileSelectionToolbarTransition
					defaultToolbar={
						<ToolbarBar
							left={
								<>
									<span className="flex size-7 shrink-0 items-center justify-center rounded-lg bg-accent/55 text-accent-foreground sm:h-8 sm:w-8">
										<Icon
											name={isRootFolder ? "House" : "FolderOpen"}
											className="size-4"
										/>
									</span>
									<div className="min-w-0 flex-1">
										<FolderBreadcrumb
											items={breadcrumb}
											compact={isCompactBreadcrumb}
											onNavigate={onNavigateToFolder}
										/>
									</div>
									<button
										type="button"
										className="flex size-7 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent/55 hover:text-accent-foreground sm:h-8 sm:w-8"
										onClick={() => void onRefresh()}
										aria-label={t("core:refresh")}
										title={t("core:refresh")}
									>
										<Icon name="ArrowsClockwise" className="size-4" />
									</button>
								</>
							}
							right={
								<>
									<SortMenu
										sortBy={sortBy}
										sortOrder={sortOrder}
										onSortBy={onSortByChange}
										onSortOrder={onSortOrderChange}
									/>
									<ViewToggle value={viewMode} onChange={onViewModeChange} />
								</>
							}
						/>
					}
					selectionToolbar={selectionToolbar}
				/>
				<section
					className={cn(
						"min-h-0 flex-1 overflow-auto",
						selectionToolbar &&
							getBottomOverlayPaddingClass("selection-compact"),
					)}
				>
					{navigating ? (
						<ShareFolderContentSkeleton viewMode={viewMode} />
					) : folderContents ? (
						<>
							{isFolderEmpty ? (
								<EmptyState
									icon={<Icon name="FolderOpen" className="size-12" />}
									title={t("empty_folder")}
									description={t("share:empty_folder_desc")}
								/>
							) : fileBrowserContextValue ? (
								<FileBrowserProvider value={fileBrowserContextValue}>
									{viewMode === "grid" ? <FileGrid /> : <FileTable />}
								</FileBrowserProvider>
							) : null}
							{hasMoreFiles && (
								<div ref={sentinelRef} className="flex justify-center py-4">
									{loadingMore && (
										<div className="size-5 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-muted-foreground" />
									)}
								</div>
							)}
						</>
					) : (
						<div className="p-6 text-sm text-muted-foreground">
							{t("loading_contents")}
						</div>
					)}
				</section>
			</main>
			{previewElement}
		</ShareFolderPageShell>
	);
}
