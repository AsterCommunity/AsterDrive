import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useFileBrowserContext } from "@/components/files/FileBrowserContext";
import {
	FileContextDropdownMenu,
	FileContextMenu,
} from "@/components/files/FileContextMenu";
import { resolveFileBrowserItemMenuProps } from "@/components/files/fileBrowserItemActionResolver";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
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
): ReturnType<typeof resolveFileBrowserItemMenuProps> {
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
		onFolderPolicy,
		onFolderOpen,
		onGoToLocation,
		onInfo,
		onManageTags,
		onMove,
		onRename,
		onShare,
		onToggleLock,
		onVersions,
		readOnly,
		selectionEnabled,
	} = useFileBrowserContext();
	const selectedFileIds = useFileStore((s) => s.selectedFileIds);
	const selectedFolderIds = useFileStore((s) => s.selectedFolderIds);

	return resolveFileBrowserItemMenuProps({
		...props,
		batchSelectionActions,
		handlers: {
			onArchiveCompress,
			onArchiveDownload,
			onArchiveExtract,
			onCopy,
			onDelete,
			onDownload,
			onFileChooseOpenMethod,
			onFileClick,
			onFileOpen,
			onFolderPolicy,
			onFolderOpen,
			onGoToLocation,
			onInfo,
			onManageTags,
			onMove,
			onRename,
			onShare,
			onToggleLock,
			onVersions,
		},
		readOnly,
		selection: {
			selectedFileIds,
			selectedFolderIds,
		},
		selectionEnabled,
	});
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
					onPointerDown={(event) => {
						event.stopPropagation();
					}}
					onClick={(event) => {
						event.stopPropagation();
					}}
					onDoubleClick={(event) => {
						event.stopPropagation();
					}}
					onKeyDown={(event) => {
						event.stopPropagation();
					}}
				>
					<Icon name="DotsThree" className="size-4" />
				</Button>
			}
		/>
	);
}
