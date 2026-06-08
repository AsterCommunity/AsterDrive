import { type DragEvent, type RefObject, Suspense } from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonFileGrid } from "@/components/common/SkeletonFileGrid";
import { SkeletonFileTable } from "@/components/common/SkeletonFileTable";
import {
	type FileBrowserContextValue,
	FileBrowserProvider,
} from "@/components/files/FileBrowserContext";
import { FileGrid } from "@/components/files/FileGrid";
import { FileTable } from "@/components/files/FileTable";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";
import { Icon } from "@/components/ui/icon";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
	type BottomOverlayOffset,
	getBottomOverlayPaddingClass,
} from "@/lib/constants";
import { cn } from "@/lib/utils";
import { CurrentFolderContextMenuContent } from "@/pages/file-browser/CurrentFolderActionsMenu";
import { FileInfoDialog } from "@/pages/file-browser/fileBrowserLazy";
import type { FileBrowserInfoTarget } from "@/pages/file-browser/types";
import type {
	FileInfo,
	FileListItem,
	FolderInfo,
	FolderListItem,
} from "@/types/api";

interface FileBrowserWorkspaceProps {
	breadcrumb: Array<{
		id: number | null;
		name: string;
	}>;
	contentDragOver: boolean;
	error: string | null;
	fileBrowserContextValue: FileBrowserContextValue;
	hasMoreFiles: boolean;
	infoPanelOpen: boolean;
	infoTarget: FileBrowserInfoTarget | null;
	isEmpty: boolean;
	isSearching: boolean;
	loading: boolean;
	loadingMore: boolean;
	scrollViewport: HTMLDivElement | null;
	sentinelRef: RefObject<HTMLDivElement | null>;
	uploadReady: boolean;
	viewMode: "grid" | "list";
	bottomOverlayOffset?: BottomOverlayOffset;
	onContentDragLeave: (event: DragEvent<HTMLElement>) => void;
	onContentDragOver: (event: DragEvent<HTMLElement>) => void;
	onContentDrop: (event: DragEvent<HTMLElement>) => Promise<void>;
	onCreateFile: () => void;
	onCreateFolder: () => void;
	onDownload: (fileId: number, fileName: string) => void;
	onInfoPanelOpenChange: (open: boolean) => void;
	onOpenInfoFolder: (folder: FolderInfo | FolderListItem) => void;
	onOfflineDownload: () => void;
	onPreview: (file: FileInfo | FileListItem) => void;
	onRefresh: () => void | Promise<void>;
	onRename: (type: "file" | "folder", id: number, name: string) => void;
	onScrollViewportRef: (node: HTMLDivElement | null) => void;
	onShare: (target: {
		fileId?: number;
		folderId?: number;
		name: string;
		initialMode?: "page" | "direct";
	}) => void;
	onToggleLock: (
		type: "file" | "folder",
		id: number,
		locked: boolean,
	) => Promise<boolean>;
	onTriggerFileUpload: () => void;
	onTriggerFolderUpload: () => void;
	onVersions: (fileId: number) => void;
}

export function FileBrowserWorkspace({
	breadcrumb,
	contentDragOver,
	error,
	fileBrowserContextValue,
	hasMoreFiles,
	infoPanelOpen,
	infoTarget,
	isEmpty,
	isSearching,
	loading,
	loadingMore,
	scrollViewport,
	sentinelRef,
	uploadReady,
	viewMode,
	bottomOverlayOffset = "none",
	onContentDragLeave,
	onContentDragOver,
	onContentDrop,
	onCreateFile,
	onCreateFolder,
	onDownload,
	onInfoPanelOpenChange,
	onOpenInfoFolder,
	onOfflineDownload,
	onPreview,
	onRefresh,
	onRename,
	onScrollViewportRef,
	onShare,
	onToggleLock,
	onTriggerFileUpload,
	onTriggerFolderUpload,
	onVersions,
}: FileBrowserWorkspaceProps) {
	const { t } = useTranslation(["files", "tasks"]);

	return (
		<div className="min-h-0 flex flex-1">
			<section
				aria-label={t("file_drop_zone")}
				className={cn(
					"relative flex min-h-0 min-w-0 flex-1 flex-col transition-colors",
					contentDragOver && "bg-accent/10",
				)}
				onDragOver={onContentDragOver}
				onDragLeave={onContentDragLeave}
				onDrop={(event) => {
					void onContentDrop(event);
				}}
			>
				{contentDragOver && (
					<div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center bg-background/35 backdrop-blur-[2px]">
						<div className="flex items-center gap-3 rounded-2xl bg-card/90 px-4 py-3 shadow-lg shadow-black/8 ring-1 ring-border/60 backdrop-blur-md dark:shadow-none">
							<div className="flex size-10 items-center justify-center rounded-xl bg-primary/10 text-primary">
								<Icon name="FolderOpen" className="size-5" />
							</div>
							<div className="space-y-0.5">
								<div className="text-sm font-semibold text-foreground">
									{t("move_to_current_folder")}
								</div>
								<div className="max-w-56 truncate text-xs text-muted-foreground">
									{breadcrumb[breadcrumb.length - 1]?.name ?? t("root")}
								</div>
							</div>
						</div>
					</div>
				)}
				<ContextMenu>
					<ContextMenuTrigger className="flex min-h-0 flex-1 flex-col">
						<ScrollArea
							ref={onScrollViewportRef}
							className="min-h-0 flex-1"
							viewportProps={{
								className: cn(
									getBottomOverlayPaddingClass(bottomOverlayOffset),
								),
							}}
						>
							{loading ? (
								viewMode === "grid" ? (
									<SkeletonFileGrid />
								) : (
									<SkeletonFileTable />
								)
							) : error ? (
								<EmptyState
									icon={<Icon name="Warning" className="size-12" />}
									title={t("core:error")}
									description={error}
								/>
							) : isEmpty ? (
								<EmptyState
									icon={<Icon name="FolderOpen" className="size-12" />}
									title={t("folder_empty")}
									description={t("folder_empty_desc")}
								/>
							) : (
								<FileBrowserProvider value={fileBrowserContextValue}>
									{viewMode === "grid" ? (
										<FileGrid scrollElement={scrollViewport} />
									) : (
										<FileTable scrollElement={scrollViewport} />
									)}
								</FileBrowserProvider>
							)}
							{!isSearching && hasMoreFiles && (
								<div ref={sentinelRef} className="flex justify-center py-4">
									{loadingMore && (
										<div className="size-5 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-muted-foreground" />
									)}
								</div>
							)}
						</ScrollArea>
					</ContextMenuTrigger>
					<CurrentFolderContextMenuContent
						uploadReady={uploadReady}
						onCreateFile={onCreateFile}
						onCreateFolder={onCreateFolder}
						onOfflineDownload={onOfflineDownload}
						onRefresh={onRefresh}
						onTriggerFileUpload={onTriggerFileUpload}
						onTriggerFolderUpload={onTriggerFolderUpload}
					/>
				</ContextMenu>
			</section>
			<Suspense fallback={null}>
				<FileInfoDialog
					open={infoPanelOpen}
					onOpenChange={onInfoPanelOpenChange}
					file={infoTarget?.file}
					folder={infoTarget?.folder}
					onPreview={(targetFile) => onPreview(targetFile)}
					onOpenFolder={onOpenInfoFolder}
					onShare={onShare}
					onDownload={onDownload}
					onRename={onRename}
					onVersions={onVersions}
					onToggleLock={onToggleLock}
				/>
			</Suspense>
		</div>
	);
}
