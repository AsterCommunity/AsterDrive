import type { ReactNode } from "react";
import { useFileBrowserContext } from "@/components/files/FileBrowserContext";
import { FileContextMenu } from "@/components/files/FileContextMenu";
import { isExtractableArchiveFileName } from "@/lib/archiveFormats";
import { useFileStore } from "@/stores/fileStore";
import type { FileListItem, FolderListItem } from "@/types/api";

type FileBrowserItemContextMenuProps =
	| {
			children: ReactNode;
			item: FolderListItem;
			isFolder: true;
			renderTrigger?: boolean;
	  }
	| {
			children: ReactNode;
			item: FileListItem;
			isFolder: false;
			renderTrigger?: boolean;
	  };

export function FileBrowserItemContextMenu({
	children,
	item,
	isFolder,
	renderTrigger = false,
}: FileBrowserItemContextMenuProps) {
	const {
		batchSelectionActions,
		onArchiveCompress,
		onArchiveDownload,
		onArchiveExtract,
		onCopy,
		onDelete,
		onDownload,
		onFileChooseOpenMethod,
		onFileClick,
		onFileOpen,
		onFolderOpen,
		onInfo,
		onMove,
		onRename,
		onShare,
		onToggleLock,
		onVersions,
	} = useFileBrowserContext();
	const selectedFileIds = useFileStore((s) => s.selectedFileIds);
	const selectedFolderIds = useFileStore((s) => s.selectedFolderIds);
	const selected = isFolder
		? selectedFolderIds.has(item.id)
		: selectedFileIds.has(item.id);
	const selectionCount = selectedFileIds.size + selectedFolderIds.size;
	const useBatchMenu =
		selected && selectionCount > 1 && batchSelectionActions != null;

	if (useBatchMenu) {
		return (
			<FileContextMenu
				renderTrigger={renderTrigger}
				isFolder={isFolder}
				isLocked={false}
				selectionCount={batchSelectionActions.count}
				downloadAction={batchSelectionActions.downloadAction}
				onArchiveCompress={batchSelectionActions.onArchiveCompress}
				onCopy={batchSelectionActions.onCopy}
				onMove={batchSelectionActions.onMove}
				onDelete={batchSelectionActions.onDelete}
			>
				{children}
			</FileContextMenu>
		);
	}

	if (isFolder) {
		return (
			<FileContextMenu
				renderTrigger={renderTrigger}
				isFolder
				isLocked={item.is_locked ?? false}
				onOpen={() => onFolderOpen(item.id, item.name)}
				onPageShare={() =>
					onShare({
						folderId: item.id,
						name: item.name,
						initialMode: "page",
					})
				}
				onArchiveDownload={
					onArchiveDownload ? () => onArchiveDownload(item.id) : undefined
				}
				onArchiveCompress={
					onArchiveCompress
						? () => onArchiveCompress("folder", item.id)
						: undefined
				}
				onCopy={() => onCopy("folder", item.id)}
				onMove={onMove ? () => onMove("folder", item.id) : undefined}
				onRename={
					onRename ? () => onRename("folder", item.id, item.name) : undefined
				}
				onToggleLock={() =>
					onToggleLock("folder", item.id, item.is_locked ?? false)
				}
				onDelete={() => onDelete("folder", item.id)}
				onInfo={() => onInfo?.("folder", item.id)}
			>
				{children}
			</FileContextMenu>
		);
	}

	return (
		<FileContextMenu
			renderTrigger={renderTrigger}
			isFolder={false}
			isLocked={item.is_locked ?? false}
			onOpen={() => (onFileOpen ?? onFileClick)(item)}
			onChooseOpenMethod={
				onFileChooseOpenMethod ? () => onFileChooseOpenMethod(item) : undefined
			}
			onDownload={() => onDownload(item.id, item.name)}
			onArchiveExtract={
				onArchiveExtract && isExtractableArchiveFileName(item.name)
					? () => onArchiveExtract(item.id)
					: undefined
			}
			onArchiveCompress={
				onArchiveCompress ? () => onArchiveCompress("file", item.id) : undefined
			}
			onPageShare={() =>
				onShare({
					fileId: item.id,
					name: item.name,
					initialMode: "page",
				})
			}
			onDirectShare={() =>
				onShare({
					fileId: item.id,
					name: item.name,
					initialMode: "direct",
				})
			}
			onCopy={() => onCopy("file", item.id)}
			onMove={onMove ? () => onMove("file", item.id) : undefined}
			onRename={
				onRename ? () => onRename("file", item.id, item.name) : undefined
			}
			onToggleLock={() =>
				onToggleLock("file", item.id, item.is_locked ?? false)
			}
			onDelete={() => onDelete("file", item.id)}
			onVersions={onVersions ? () => onVersions(item.id) : undefined}
			onInfo={() => onInfo?.("file", item.id)}
		>
			{children}
		</FileContextMenu>
	);
}
