import type { DragEvent } from "react";
import { useTranslation } from "react-i18next";
import { SortMenu } from "@/components/common/SortMenu";
import { ToolbarBar } from "@/components/common/ToolbarBar";
import { ViewToggle } from "@/components/common/ViewToggle";
import { FileSelectionToolbarTransition } from "@/components/files/FileSelectionToolbar";
import { FolderBreadcrumb } from "@/components/files/FolderBreadcrumb";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Icon } from "@/components/ui/icon";
import { CurrentFolderDropdownMenuContent } from "@/pages/file-browser/CurrentFolderActionsMenu";
import type { FileBrowserSelectionToolbarState } from "@/pages/file-browser/types";
import type { SortBy, SortOrder } from "@/stores/fileStore/types";

interface FileBrowserToolbarProps {
	breadcrumb: Array<{
		id: number | null;
		name: string;
	}>;
	currentFolderActions?: "full" | "refresh-only";
	dragOverBreadcrumbIndex: number | null;
	isCompactBreadcrumb: boolean;
	isRootFolder: boolean;
	isSearching: boolean;
	searchQuery: string | null;
	selectionToolbar: FileBrowserSelectionToolbarState | null;
	sortBy: SortBy;
	sortOrder: SortOrder;
	uploadReady: boolean;
	viewMode: "grid" | "list";
	onBreadcrumbDragLeave: (event: DragEvent) => void;
	onBreadcrumbDragOver: (event: DragEvent, index: number) => void;
	onBreadcrumbDrop: (
		event: DragEvent,
		index: number,
		targetFolderId: number | null,
	) => Promise<void>;
	onCreateFile: () => void;
	onCreateFolder: () => void;
	onManageTagLibrary: () => void;
	onNavigateToFolder: (folderId: number | null, folderName: string) => void;
	onOfflineDownload: () => void;
	onRefresh: () => void | Promise<void>;
	onSetSortBy: (value: SortBy) => void;
	onSetSortOrder: (value: SortOrder) => void;
	onSetViewMode: (value: "grid" | "list") => void;
	onTriggerFileUpload: () => void;
	onTriggerFolderUpload: () => void;
}

export function FileBrowserToolbar({
	breadcrumb,
	currentFolderActions = "full",
	dragOverBreadcrumbIndex,
	isCompactBreadcrumb,
	isRootFolder,
	isSearching,
	searchQuery,
	selectionToolbar,
	sortBy,
	sortOrder,
	uploadReady,
	viewMode,
	onBreadcrumbDragLeave,
	onBreadcrumbDragOver,
	onBreadcrumbDrop,
	onCreateFile,
	onCreateFolder,
	onManageTagLibrary,
	onNavigateToFolder,
	onOfflineDownload,
	onRefresh,
	onSetSortBy,
	onSetSortOrder,
	onSetViewMode,
	onTriggerFileUpload,
	onTriggerFolderUpload,
}: FileBrowserToolbarProps) {
	const { t } = useTranslation(["files", "tasks"]);
	const defaultLeft = (
		<>
			<span className="flex size-7 shrink-0 items-center justify-center rounded-lg bg-accent/55 text-accent-foreground sm:h-8 sm:w-8">
				<Icon name={isRootFolder ? "House" : "FolderOpen"} className="size-4" />
			</span>
			<div className="min-w-0 flex-1">
				{isSearching ? (
					<span className="block truncate text-xs text-muted-foreground sm:text-sm">
						{t("core:search")}: &quot;{searchQuery}&quot;
					</span>
				) : (
					<FolderBreadcrumb
						items={breadcrumb}
						compact={isCompactBreadcrumb}
						dragOverIndex={dragOverBreadcrumbIndex}
						onDragLeave={onBreadcrumbDragLeave}
						onDragOver={onBreadcrumbDragOver}
						onDrop={onBreadcrumbDrop}
						onNavigate={onNavigateToFolder}
					/>
				)}
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
	);
	const defaultRight = (
		<>
			<DropdownMenu>
				<DropdownMenuTrigger
					render={
						<Button
							type="button"
							size="icon-sm"
							variant="ghost"
							className="sm:hidden"
							aria-label={t("folder_more_actions")}
							title={t("folder_more_actions")}
						>
							<Icon name="DotsThree" className="size-4" />
						</Button>
					}
				/>
				<CurrentFolderDropdownMenuContent
					mode={currentFolderActions}
					uploadReady={uploadReady}
					onCreateFile={onCreateFile}
					onCreateFolder={onCreateFolder}
					onManageTagLibrary={onManageTagLibrary}
					onOfflineDownload={onOfflineDownload}
					onRefresh={onRefresh}
					onTriggerFileUpload={onTriggerFileUpload}
					onTriggerFolderUpload={onTriggerFolderUpload}
				/>
			</DropdownMenu>
			<Button
				type="button"
				size="sm"
				variant="outline"
				className="hidden md:inline-flex"
				onClick={onManageTagLibrary}
			>
				<Icon name="Tag" className="size-3.5" />
				<span>{t("tag_library_manage")}</span>
			</Button>
			<SortMenu
				sortBy={sortBy}
				sortOrder={sortOrder}
				onSortBy={onSetSortBy}
				onSortOrder={onSetSortOrder}
			/>
			<ViewToggle value={viewMode} onChange={onSetViewMode} />
		</>
	);

	return (
		<FileSelectionToolbarTransition
			defaultToolbar={<ToolbarBar left={defaultLeft} right={defaultRight} />}
			selectionToolbar={selectionToolbar}
		/>
	);
}
