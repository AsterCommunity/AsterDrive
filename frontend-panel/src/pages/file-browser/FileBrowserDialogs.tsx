import { Suspense } from "react";
import { FilePreview } from "@/components/files/FilePreview";
import type { ImagePreviewNavigation } from "@/components/files/preview/imagePreviewNavigation";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import {
	ArchiveTaskNameDialog,
	BatchTargetFolderDialog,
	CreateFileDialog,
	CreateFolderDialog,
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
	onCopyConfirm: (targetFolderId: number | null) => Promise<void>;
	onCreateFileOpenChange: (open: boolean) => void;
	onCreateFolderOpenChange: (open: boolean) => void;
	onFolderPolicyClose: () => void;
	onFolderPolicyUpdated?: () => void | Promise<void>;
	onMoveClose: () => void;
	onMoveConfirm: (targetFolderId: number | null) => Promise<void>;
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
	const { retainedValue: retainedPreviewState, handleOpenChangeComplete } =
		useRetainedDialogValue(previewState, previewState !== null);
	const {
		retainedValue: retainedArchiveTaskTarget,
		handleOpenChangeComplete: handleArchiveTaskOpenChangeComplete,
	} = useRetainedDialogValue(archiveTaskTarget, archiveTaskTarget !== null);
	const {
		retainedValue: retainedShareTarget,
		handleOpenChangeComplete: handleShareOpenChangeComplete,
	} = useRetainedDialogValue(shareTarget, shareTarget !== null);
	const {
		retainedValue: retainedCopyTarget,
		handleOpenChangeComplete: handleCopyOpenChangeComplete,
	} = useRetainedDialogValue(copyTarget, copyTarget !== null);
	const {
		retainedValue: retainedMoveTarget,
		handleOpenChangeComplete: handleMoveOpenChangeComplete,
	} = useRetainedDialogValue(moveTarget, moveTarget !== null);
	const {
		retainedValue: retainedVersionTarget,
		handleOpenChangeComplete: handleVersionOpenChangeComplete,
	} = useRetainedDialogValue(versionTarget, versionTarget !== null);
	const {
		retainedValue: retainedRenameTarget,
		handleOpenChangeComplete: handleRenameOpenChangeComplete,
	} = useRetainedDialogValue(renameTarget, renameTarget !== null);
	const {
		retainedValue: retainedFolderPolicyTarget,
		handleOpenChangeComplete: handleFolderPolicyOpenChangeComplete,
	} = useRetainedDialogValue(folderPolicyTarget, folderPolicyTarget !== null);

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
					onOpenChangeComplete={handleArchiveTaskOpenChangeComplete}
					mode={retainedArchiveTaskTarget?.mode ?? "compress"}
					initialName={retainedArchiveTaskTarget?.initialName ?? ""}
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
				<FolderPolicyDialog
					open={folderPolicyTarget !== null}
					onOpenChange={(open) => {
						if (!open) onFolderPolicyClose();
					}}
					onOpenChangeComplete={handleFolderPolicyOpenChangeComplete}
					folder={retainedFolderPolicyTarget}
					onUpdated={onFolderPolicyUpdated}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<ShareDialog
					open={shareTarget !== null}
					onOpenChange={(open) => {
						if (!open) onShareClose();
					}}
					onOpenChangeComplete={handleShareOpenChangeComplete}
					onShareCreated={onShareCreated}
					fileId={retainedShareTarget?.fileId}
					folderId={retainedShareTarget?.folderId}
					name={retainedShareTarget?.name ?? ""}
					initialMode={retainedShareTarget?.initialMode}
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
					imageNavigation={
						previewImageNavigation && onPreviewNavigate
							? {
									previousFile: previewImageNavigation.previousFile,
									nextFile: previewImageNavigation.nextFile,
									onNavigate: onPreviewNavigate,
								}
							: undefined
					}
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
					onOpenChangeComplete={handleCopyOpenChangeComplete}
					mode="copy"
					onConfirm={onCopyConfirm}
					currentFolderId={currentFolderId}
					initialBreadcrumb={breadcrumb}
					selectedFolderIds={
						retainedCopyTarget?.type === "folder" ? [retainedCopyTarget.id] : []
					}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<BatchTargetFolderDialog
					open={moveTarget !== null}
					onOpenChange={(open) => {
						if (!open) onMoveClose();
					}}
					onOpenChangeComplete={handleMoveOpenChangeComplete}
					mode="move"
					onConfirm={onMoveConfirm}
					currentFolderId={currentFolderId}
					initialBreadcrumb={breadcrumb}
					selectedFolderIds={retainedMoveTarget?.folderIds ?? []}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<VersionHistoryDialog
					open={versionTarget !== null}
					onOpenChange={(open) => {
						if (!open) onVersionClose();
					}}
					onOpenChangeComplete={handleVersionOpenChangeComplete}
					fileId={retainedVersionTarget?.fileId ?? 0}
					fileName={retainedVersionTarget?.fileName ?? ""}
					mimeType={retainedVersionTarget?.mimeType}
					onRestored={onVersionRestored}
				/>
			</Suspense>

			<Suspense fallback={null}>
				<RenameDialog
					open={renameTarget !== null}
					onOpenChange={(open) => {
						if (!open) onRenameClose();
					}}
					onOpenChangeComplete={handleRenameOpenChangeComplete}
					type={retainedRenameTarget?.type ?? "file"}
					id={retainedRenameTarget?.id ?? 0}
					currentName={retainedRenameTarget?.name ?? ""}
					onRenamed={onRenamed}
				/>
			</Suspense>
		</>
	);
}
