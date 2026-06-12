import {
	Fragment,
	type ReactNode,
	type RefObject,
	useCallback,
	useEffect,
	useMemo,
} from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/common/EmptyState";
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import { ViewToggle } from "@/components/common/ViewToggle";
import {
	type FileBrowserContextValue,
	FileBrowserProvider,
} from "@/components/files/FileBrowserContext";
import { FileGrid } from "@/components/files/FileGrid";
import { FileTable } from "@/components/files/FileTable";
import {
	Breadcrumb,
	BreadcrumbItem,
	BreadcrumbLink,
	BreadcrumbList,
	BreadcrumbPage,
	BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Icon } from "@/components/ui/icon";
import { PAGE_SECTION_PADDING_CLASS } from "@/lib/constants";
import type { FileBrowserSelectionToolbarState } from "@/pages/file-browser/types";
import { useFileBrowserBatchActions } from "@/pages/file-browser/useFileBrowserBatchActions";
import { shareService } from "@/services/shareService";
import { useFileStore } from "@/stores/fileStore";
import type {
	FileInfo,
	FileListItem,
	FolderContents,
	SharePublicInfo,
} from "@/types/api";
import { ShareFolderContentSkeleton } from "./ShareFolderSkeleton";
import { ShareMetaLine, SharePageShell } from "./ShareViewShell";
import {
	getAccessSummary,
	getDownloadSummary,
	getExpirySummary,
} from "./shareViewSummaries";
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
	token: string;
	viewMode: "grid" | "list";
	onFileDownload: (file: FileListItem) => void;
	onFilePreview: (file: FileInfo | FileListItem) => void;
	onNavigateToFolder: (folderId: number | null, folderName?: string) => void;
	onViewModeChange: (viewMode: "grid" | "list") => void;
}

function ShareSelectionToolbar({
	selectionToolbar,
}: {
	selectionToolbar: FileBrowserSelectionToolbarState | null;
}) {
	const { t } = useTranslation(["core", "files", "tasks"]);

	if (!selectionToolbar) return null;

	const selectVisibleLabel = selectionToolbar.allDisplayedSelected
		? t("files:selection_clear")
		: t("files:selection_select_all_visible");
	const downloadLabel =
		selectionToolbar.downloadAction?.kind === "file"
			? t("files:download")
			: t("tasks:archive_download_action");

	return (
		<div
			data-testid="share-selection-toolbar"
			className="flex items-center gap-2 rounded-lg border border-border/70 bg-background/70 px-2 py-1.5 shadow-xs"
		>
			<span className="px-1 text-sm font-medium text-foreground">
				{t("core:selected_count", { count: selectionToolbar.count })}
			</span>
			{selectionToolbar.downloadAction ? (
				<button
					type="button"
					className="flex size-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/65 hover:text-foreground"
					onClick={selectionToolbar.downloadAction.onClick}
					aria-label={downloadLabel}
					title={downloadLabel}
				>
					<Icon name="Download" className="size-4" />
				</button>
			) : null}
			<button
				type="button"
				className="flex size-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/65 hover:text-foreground disabled:pointer-events-none disabled:opacity-45"
				onClick={selectionToolbar.onToggleDisplayedSelection}
				disabled={!selectionToolbar.hasDisplayedItems}
				aria-label={selectVisibleLabel}
				title={selectVisibleLabel}
			>
				<Icon name="Check" className="size-4" />
			</button>
			<button
				type="button"
				className="flex size-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/65 hover:text-foreground"
				onClick={selectionToolbar.onClearSelection}
				aria-label={t("files:selection_clear")}
				title={t("files:selection_clear")}
			>
				<Icon name="X" className="size-4" />
			</button>
		</div>
	);
}

function ShareFolderBreadcrumb({
	breadcrumb,
	onNavigateToFolder,
}: {
	breadcrumb: ShareBreadcrumbItem[];
	onNavigateToFolder: (folderId: number | null, folderName?: string) => void;
}) {
	if (breadcrumb.length <= 1) return null;

	return (
		<Breadcrumb>
			<BreadcrumbList className="gap-2">
				{breadcrumb.map((item, i) => (
					<Fragment key={item.id ?? "root"}>
						{i > 0 && (
							<BreadcrumbSeparator className="mx-0.5 text-muted-foreground/45" />
						)}
						<BreadcrumbItem>
							{i < breadcrumb.length - 1 ? (
								<BreadcrumbLink
									className="cursor-pointer rounded-md px-1.5 py-0.5 text-muted-foreground"
									onClick={() => onNavigateToFolder(item.id, item.name)}
								>
									{item.name}
								</BreadcrumbLink>
							) : (
								<BreadcrumbPage className="text-base font-semibold text-foreground">
									{item.name}
								</BreadcrumbPage>
							)}
						</BreadcrumbItem>
					</Fragment>
				))}
			</BreadcrumbList>
		</Breadcrumb>
	);
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
	onViewModeChange,
	previewElement,
	sentinelRef,
	shareOwnerText,
	token,
	viewMode,
}: ShareFolderViewProps) {
	const { t } = useTranslation(["core", "share", "files", "errors"]);
	const clearSelection = useFileStore((state) => state.clearSelection);
	const handleArchiveDownload = useCallback(
		(fileIds: number[], folderIds: number[]) =>
			shareService.streamArchiveDownload(token, fileIds, folderIds),
		[token],
	);
	const { dialogs: batchActionDialogs, selectionToolbar } =
		useFileBrowserBatchActions({
			allowCopyMove: false,
			allowDelete: false,
			allowTagManagement: false,
			displayFiles: folderContents?.files ?? [],
			displayFolders: folderContents?.folders ?? [],
			onArchiveDownload: handleArchiveDownload,
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
	const breadcrumbElement = (
		<ShareFolderBreadcrumb
			breadcrumb={breadcrumb}
			onNavigateToFolder={onNavigateToFolder}
		/>
	);
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
			breadcrumbPathIds,
			folderContents,
			onFileDownload,
			onFilePreview,
			onNavigateToFolder,
			selectionToolbar,
			token,
		]);

	return (
		<SharePageShell>
			{batchActionDialogs}
			<main className="flex min-h-0 flex-1 flex-col overflow-hidden">
				<div
					className={`border-b border-border/65 bg-card/55 ${PAGE_SECTION_PADDING_CLASS}`}
				>
					<div className="mx-auto flex w-full max-w-7xl flex-col gap-3 py-3 sm:flex-row sm:items-center sm:justify-between">
						<div className="flex min-w-0 items-center gap-3">
							<div className="flex size-12 shrink-0 items-center justify-center rounded-lg bg-amber-500/12 text-amber-600 dark:text-amber-300">
								<Icon name="Folder" className="size-6" />
							</div>
							<div className="min-w-0">
								<h1 className="truncate text-lg font-semibold leading-tight sm:text-xl">
									{info.name}
								</h1>
								<div className="mt-1 flex min-w-0 items-center gap-2">
									<UserAvatarImage
										avatar={info.shared_by.avatar}
										name={info.shared_by.name}
										size="sm"
										className="size-5 rounded-md text-[10px]"
									/>
									<ShareMetaLine
										items={[
											shareOwnerText,
											getDownloadSummary(info, t),
											getExpirySummary(info, t),
											getAccessSummary(info, t),
										]}
										className="min-w-0 text-xs"
									/>
								</div>
							</div>
						</div>
						<div className="flex items-center gap-2">
							<ShareSelectionToolbar selectionToolbar={selectionToolbar} />
							<ViewToggle value={viewMode} onChange={onViewModeChange} />
						</div>
					</div>
				</div>
				<div className={`min-h-0 flex-1 py-3 ${PAGE_SECTION_PADDING_CLASS}`}>
					<section className="mx-auto flex h-full w-full max-w-7xl flex-col overflow-hidden rounded-lg border border-border/70 bg-card/70 shadow-xs dark:bg-card/40 dark:shadow-none">
						{breadcrumb.length > 1 ? (
							<div className="flex flex-col gap-3 border-b border-border/65 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
								<div className="flex min-w-0 items-center gap-2">
									<Icon
										name="FolderOpen"
										className="size-5 shrink-0 text-amber-500"
									/>
									<div className="min-w-0 flex-1">{breadcrumbElement}</div>
								</div>
							</div>
						) : null}
						<div className="min-h-0 flex-1 overflow-auto">
							{navigating ? (
								<ShareFolderContentSkeleton viewMode={viewMode} />
							) : folderContents ? (
								<>
									{isFolderEmpty ? (
										<EmptyState
											icon={<Icon name="FolderOpen" className="size-12" />}
											title={t("empty_folder")}
											description={t("files:folder_empty_desc")}
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
						</div>
					</section>
				</div>
			</main>
			{previewElement}
		</SharePageShell>
	);
}
