import { Suspense } from "react";
import type { BatchTargetFolderSelection } from "@/components/files/BatchTargetFolderDialog";
import { FilePreview } from "@/components/files/FilePreview";
import type { ImagePreviewNavigation } from "@/components/files/preview/navigation/imagePreviewNavigation";
import type { FilePreviewResources } from "@/components/files/preview/resources/filePreviewResources";
import {
	ArchiveTaskNameDialog,
	BatchTargetFolderDialog,
	CreateFileDialog,
	CreateFolderDialog,
	FolderIconDialog,
	FolderPolicyDialog,
	OfflineDownloadDialog,
	RenameDialog,
	ShareDialog,
	VersionHistoryDialog,
} from "@/pages/file-browser/fileBrowserLazy";
import type {
	FileBrowserArchiveTaskTarget,
	FileBrowserCopyTarget,
	FileBrowserMoveTarget,
	FileBrowserPreviewState,
	FileBrowserRenameTarget,
	FileBrowserShareTarget,
	FileBrowserVersionTarget,
} from "@/pages/file-browser/types";
import { fileService } from "@/services/fileService";
import type { BreadcrumbItem } from "@/stores/fileStore";
import type { ArchiveFilenameEncoding, FolderListItem } from "@/types/api";

interface FileBrowserDialogsProps {
	archiveTaskTarget: FileBrowserArchiveTaskTarget | null;
	breadcrumb: BreadcrumbItem[];
	copyTarget: FileBrowserCopyTarget | null;
	createFileOpen: boolean;
	createFolderOpen: boolean;
	currentFolderId: number | null;
	currentFolderName?: string | null;
	folderPolicyTarget: FolderListItem | null;
	folderIconTarget: FolderListItem | null;
	moveTarget: FileBrowserMoveTarget | null;
	offlineDownloadOpen: boolean;
	previewImageNavigation?: ImagePreviewNavigation<
		FileBrowserPreviewState["file"]
	>;
	previewState: FileBrowserPreviewState | null;
	renameTarget: FileBrowserRenameTarget | null;
	shareTarget: FileBrowserShareTarget | null;
	versionTarget: FileBrowserVersionTarget | null;
	onArchiveTaskClose: () => void;
	onArchiveTaskSubmit: (
		name: string | undefined,
		filenameEncoding?: ArchiveFilenameEncoding,
	) => Promise<void>;
	onCopyClose: () => void;
	onCopyConfirm: (selection: BatchTargetFolderSelection) => Promise<void>;
	onCreateFileOpenChange: (open: boolean) => void;
	onCreateFolderOpenChange: (open: boolean) => void;
	onFolderPolicyClose: () => void;
	onFolderPolicyUpdated?: () => void | Promise<void>;
	onFolderIconClose: () => void;
	onFolderIconUpdated?: () => void | Promise<void>;
	onMoveClose: () => void;
	onMoveConfirm: (selection: BatchTargetFolderSelection) => Promise<void>;
	onOfflineDownloadOpenChange: (open: boolean) => void;
	onPreviewClose: () => void;
	onPreviewFileUpdated: () => void | Promise<void>;
	onPreviewNavigate?: (file: FileBrowserPreviewState["file"]) => void;
	onRenameClose: () => void;
	onRenamed?: () => void | Promise<void>;
	onShareClose: () => void;
	onShareCreated?: () => void | Promise<void>;
	onVersionClose: () => void;
	onVersionRestored: () => void | Promise<void>;
}

export function FileBrowserDialogs({
	archiveTaskTarget,
	breadcrumb,
	copyTarget,
	createFileOpen,
	createFolderOpen,
	currentFolderId,
	currentFolderName,
	folderPolicyTarget,
	folderIconTarget,
	moveTarget,
	offlineDownloadOpen,
	previewImageNavigation,
	previewState,
	renameTarget,
	shareTarget,
	versionTarget,
	onArchiveTaskClose,
	onArchiveTaskSubmit,
	onCopyClose,
	onCopyConfirm,
	onCreateFileOpenChange,
	onCreateFolderOpenChange,
	onFolderPolicyClose,
	onFolderPolicyUpdated,
	onFolderIconClose,
	onFolderIconUpdated,
	onMoveClose,
	onMoveConfirm,
	onOfflineDownloadOpenChange,
	onPreviewClose,
	onPreviewFileUpdated,
	onPreviewNavigate,
	onRenameClose,
	onRenamed,
	onShareClose,
	onShareCreated,
	onVersionClose,
	onVersionRestored,
}: FileBrowserDialogsProps) {
	return (
		<>
			<Suspense fallback={null}>
				<CreateFolderDialog
					open={createFolderOpen}
					onOpenChange={onCreateFolderOpenChange}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<CreateFileDialog
					open={createFileOpen}
					onOpenChange={onCreateFileOpenChange}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<ArchiveTaskNameDialog
					open={archiveTaskTarget !== null}
					onOpenChange={(open) => {
						if (!open) onArchiveTaskClose();
					}}
					mode={archiveTaskTarget?.mode ?? "compress"}
					initialName={archiveTaskTarget?.initialName ?? ""}
					onSubmit={onArchiveTaskSubmit}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<OfflineDownloadDialog
					open={offlineDownloadOpen}
					onOpenChange={onOfflineDownloadOpenChange}
					targetFolderId={currentFolderId}
					targetFolderName={currentFolderName}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<FolderIconDialog
					open={folderIconTarget !== null}
					onOpenChange={(open) => {
						if (!open) onFolderIconClose();
					}}
					folder={folderIconTarget}
					onUpdated={onFolderIconUpdated}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<FolderPolicyDialog
					open={folderPolicyTarget !== null}
					onOpenChange={(open) => {
						if (!open) onFolderPolicyClose();
					}}
					folder={folderPolicyTarget}
					onUpdated={onFolderPolicyUpdated}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<ShareDialog
					open={shareTarget !== null}
					onOpenChange={(open) => {
						if (!open) onShareClose();
					}}
					onShareCreated={onShareCreated}
					fileId={shareTarget?.fileId}
					folderId={shareTarget?.folderId}
					name={shareTarget?.name ?? ""}
					initialMode={shareTarget?.initialMode}
				/>
			</Suspense>

			{
				<FilePreview
					open={previewState !== null}
					file={previewState?.file ?? null}
					openMode={previewState?.openMode}
					onClose={onPreviewClose}
					onFileUpdated={onPreviewFileUpdated}
					imageNavigation={
						previewImageNavigation && onPreviewNavigate
							? {
									previousFile: previewImageNavigation.previousFile,
									nextFile: previewImageNavigation.nextFile,
									onNavigate: onPreviewNavigate,
								}
							: undefined
					}
					resources={
						previewState
							? ({
									scope: "personal",
									paths: {
										download: fileService.downloadPath(previewState.file.id),
										imagePreview: fileService.imagePreviewPath(
											previewState.file.id,
										),
										thumbnail: fileService.thumbnailPath(previewState.file.id),
									},
									resolve: fileService.resolveResourceHandle,
									actions: {
										createExternalPreviewLink: () =>
											fileService.createPreviewLink(previewState.file.id),
										loadArchiveManifest: (options) =>
											fileService.getArchivePreview(
												previewState.file.id,
												options,
											),
										launchWopiSession: (appKey) =>
											fileService.createWopiSession(
												previewState.file.id,
												appKey,
											),
									},
								} satisfies FilePreviewResources)
							: null
					}
				/>
			}

			<Suspense fallback={null}>
				<BatchTargetFolderDialog
					open={copyTarget !== null}
					onOpenChange={(open) => {
						if (!open) onCopyClose();
					}}
					mode="copy"
					onConfirm={onCopyConfirm}
					currentFolderId={currentFolderId}
					initialBreadcrumb={breadcrumb}
					selectedFolderIds={
						copyTarget?.type === "folder" ? [copyTarget.id] : []
					}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<BatchTargetFolderDialog
					open={moveTarget !== null}
					onOpenChange={(open) => {
						if (!open) onMoveClose();
					}}
					mode="move"
					onConfirm={onMoveConfirm}
					currentFolderId={currentFolderId}
					initialBreadcrumb={breadcrumb}
					selectedFolderIds={moveTarget?.folderIds ?? []}
					sourceWorkspace={moveTarget?.sourceWorkspace}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<VersionHistoryDialog
					open={versionTarget !== null}
					onOpenChange={(open) => {
						if (!open) onVersionClose();
					}}
					fileId={versionTarget?.fileId ?? 0}
					fileName={versionTarget?.fileName ?? ""}
					mimeType={versionTarget?.mimeType}
					onRestored={onVersionRestored}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<RenameDialog
					open={renameTarget !== null}
					onOpenChange={(open) => {
						if (!open) onRenameClose();
					}}
					type={renameTarget?.type ?? "file"}
					id={renameTarget?.id ?? 0}
					currentName={renameTarget?.name ?? ""}
					onRenamed={onRenamed}
				/>
			</Suspense>
		</>
	);
}
