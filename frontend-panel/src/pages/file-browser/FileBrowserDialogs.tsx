import { Suspense } from "react";
import { FilePreview } from "@/components/files/FilePreview";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import {
	ArchiveTaskNameDialog,
	BatchTargetFolderDialog,
	CreateFileDialog,
	CreateFolderDialog,
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
import type { ArchiveFilenameEncoding } from "@/types/api";

interface FileBrowserDialogsProps {
	archiveTaskTarget: FileBrowserArchiveTaskTarget | null;
	breadcrumb: BreadcrumbItem[];
	copyTarget: FileBrowserCopyTarget | null;
	createFileOpen: boolean;
	createFolderOpen: boolean;
	currentFolderId: number | null;
	currentFolderName?: string | null;
	moveTarget: FileBrowserMoveTarget | null;
	offlineDownloadOpen: boolean;
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
	onCopyConfirm: (targetFolderId: number | null) => Promise<void>;
	onCreateFileOpenChange: (open: boolean) => void;
	onCreateFolderOpenChange: (open: boolean) => void;
	onMoveClose: () => void;
	onMoveConfirm: (targetFolderId: number | null) => Promise<void>;
	onOfflineDownloadOpenChange: (open: boolean) => void;
	onPreviewClose: () => void;
	onPreviewFileUpdated: () => void | Promise<void>;
	onRenameClose: () => void;
	onShareClose: () => void;
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
	moveTarget,
	offlineDownloadOpen,
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
	onMoveClose,
	onMoveConfirm,
	onOfflineDownloadOpenChange,
	onPreviewClose,
	onPreviewFileUpdated,
	onRenameClose,
	onShareClose,
	onVersionClose,
	onVersionRestored,
}: FileBrowserDialogsProps) {
	const { retainedValue: retainedPreviewState, handleOpenChangeComplete } =
		useRetainedDialogValue(previewState, previewState !== null);

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
				<ShareDialog
					open={shareTarget !== null}
					onOpenChange={(open) => {
						if (!open) onShareClose();
					}}
					fileId={shareTarget?.fileId}
					folderId={shareTarget?.folderId}
					name={shareTarget?.name ?? ""}
					initialMode={shareTarget?.initialMode}
				/>
			</Suspense>

			{retainedPreviewState ? (
				<FilePreview
					open={previewState !== null}
					file={retainedPreviewState.file}
					openMode={retainedPreviewState.openMode}
					onClose={onPreviewClose}
					onOpenChangeComplete={handleOpenChangeComplete}
					onFileUpdated={onPreviewFileUpdated}
					previewLinkFactory={() =>
						fileService.createPreviewLink(retainedPreviewState.file.id)
					}
					archivePreviewFactory={(options) =>
						fileService.getArchivePreview(retainedPreviewState.file.id, options)
					}
					wopiSessionFactory={(appKey) =>
						fileService.createWopiSession(retainedPreviewState.file.id, appKey)
					}
				/>
			) : null}

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
				/>
			</Suspense>
		</>
	);
}
