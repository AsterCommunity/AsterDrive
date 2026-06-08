import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useFileBrowserContext } from "@/components/files/FileBrowserContext";
import {
	FileContextDropdownMenu,
	FileContextMenu,
	type FileContextMenuProps,
} from "@/components/files/FileContextMenu";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
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

type FileBrowserItemActionMenuProps =
	| {
			item: FolderListItem;
			isFolder: true;
	  }
	| {
			item: FileListItem;
			isFolder: false;
	  };

function useFileBrowserItemMenuProps(
	props: FileBrowserItemActionMenuProps,
): Omit<FileContextMenuProps, "children" | "renderTrigger"> {
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
		onManageTags,
		onMove,
		onRename,
		onShare,
		onToggleLock,
		onVersions,
	} = useFileBrowserContext();
	const selectedFileIds = useFileStore((s) => s.selectedFileIds);
	const selectedFolderIds = useFileStore((s) => s.selectedFolderIds);
	const selected = props.isFolder
		? selectedFolderIds.has(props.item.id)
		: selectedFileIds.has(props.item.id);
	const selectionCount = selectedFileIds.size + selectedFolderIds.size;
	const useBatchMenu =
		selected && selectionCount > 1 && batchSelectionActions != null;

	if (useBatchMenu) {
		return {
			isFolder: props.isFolder,
			isLocked: false,
			selectionCount: batchSelectionActions.count,
			downloadAction: batchSelectionActions.downloadAction,
			onArchiveCompress: batchSelectionActions.onArchiveCompress,
			onCopy: batchSelectionActions.onCopy,
			onMove: batchSelectionActions.onMove,
			onManageTags: batchSelectionActions.onManageTags,
			onDelete: batchSelectionActions.onDelete,
		};
	}

	if (props.isFolder) {
		const { item } = props;
		return {
			isFolder: true,
			isLocked: item.is_locked ?? false,
			onOpen: () => onFolderOpen(item.id, item.name),
			onPageShare: () =>
				onShare({
					folderId: item.id,
					name: item.name,
					initialMode: "page",
				}),
			onArchiveDownload: onArchiveDownload
				? () => onArchiveDownload(item.id)
				: undefined,
			onArchiveCompress: onArchiveCompress
				? () => onArchiveCompress("folder", item.id)
				: undefined,
			onCopy: () => onCopy("folder", item.id),
			onManageTags: onManageTags
				? () => onManageTags("folder", item.id)
				: undefined,
			onMove: onMove ? () => onMove("folder", item.id) : undefined,
			onRename: onRename
				? () => onRename("folder", item.id, item.name)
				: undefined,
			onToggleLock: () =>
				onToggleLock("folder", item.id, item.is_locked ?? false),
			onDelete: () => onDelete("folder", item.id),
			onInfo: () => onInfo?.("folder", item.id),
		};
	}

	const { item } = props;
	return {
		isFolder: false,
		isLocked: item.is_locked ?? false,
		onOpen: () => (onFileOpen ?? onFileClick)(item),
		onChooseOpenMethod: onFileChooseOpenMethod
			? () => onFileChooseOpenMethod(item)
			: undefined,
		onDownload: () => onDownload(item.id, item.name),
		onArchiveExtract:
			onArchiveExtract && isExtractableArchiveFileName(item.name)
				? () => onArchiveExtract(item.id)
				: undefined,
		onArchiveCompress: onArchiveCompress
			? () => onArchiveCompress("file", item.id)
			: undefined,
		onPageShare: () =>
			onShare({
				fileId: item.id,
				name: item.name,
				initialMode: "page",
			}),
		onDirectShare: () =>
			onShare({
				fileId: item.id,
				name: item.name,
				initialMode: "direct",
			}),
		onCopy: () => onCopy("file", item.id),
		onManageTags: onManageTags
			? () => onManageTags("file", item.id)
			: undefined,
		onMove: onMove ? () => onMove("file", item.id) : undefined,
		onRename: onRename ? () => onRename("file", item.id, item.name) : undefined,
		onToggleLock: () => onToggleLock("file", item.id, item.is_locked ?? false),
		onDelete: () => onDelete("file", item.id),
		onVersions: onVersions ? () => onVersions(item.id) : undefined,
		onInfo: () => onInfo?.("file", item.id),
	};
}

export function FileBrowserItemContextMenu({
	children,
	renderTrigger = false,
	...props
}: FileBrowserItemContextMenuProps) {
	const menuProps = useFileBrowserItemMenuProps(props);

	return (
		<FileContextMenu renderTrigger={renderTrigger} {...menuProps}>
			{children}
		</FileContextMenu>
	);
}

export function FileBrowserItemActionMenu({
	...props
}: FileBrowserItemActionMenuProps) {
	const { t } = useTranslation("files");
	const menuProps = useFileBrowserItemMenuProps(props);

	return (
		<FileContextDropdownMenu
			{...menuProps}
			trigger={
				<Button
					type="button"
					variant="ghost"
					size="icon-sm"
					className="rounded-lg opacity-100 sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100"
					aria-label={t("more_actions")}
					onClick={(event) => {
						event.stopPropagation();
					}}
					onDoubleClick={(event) => {
						event.stopPropagation();
					}}
				>
					<Icon name="DotsThree" className="size-4" />
				</Button>
			}
		/>
	);
}
