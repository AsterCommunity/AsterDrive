import type {
	FileBrowserBatchSelectionActions,
	FileBrowserContextValue,
	FileBrowserShareTarget,
} from "@/components/files/FileBrowserContext";
import type { FileContextMenuProps } from "@/components/files/FileContextMenu";
import { isExtractableArchiveFileName } from "@/lib/archiveFormats";
import { isDirectResourceLock, isResourceLocked } from "@/lib/resourceLock";
import type { FileListItem, FolderListItem } from "@/types/api";

type ResolvedFileContextMenuProps = Omit<
	FileContextMenuProps,
	"children" | "renderTrigger"
>;

export interface FileBrowserItemSelectionState {
	selectedFileIds: ReadonlySet<number>;
	selectedFolderIds: ReadonlySet<number>;
}

export interface FileBrowserItemActionHandlers {
	onArchiveCompress?: FileBrowserContextValue["onArchiveCompress"];
	onArchiveDownload?: FileBrowserContextValue["onArchiveDownload"];
	onArchiveExtract?: FileBrowserContextValue["onArchiveExtract"];
	onCopy?: FileBrowserContextValue["onCopy"];
	onDelete?: FileBrowserContextValue["onDelete"];
	onDownload: FileBrowserContextValue["onDownload"];
	onFileChooseOpenMethod?: FileBrowserContextValue["onFileChooseOpenMethod"];
	onFileClick: FileBrowserContextValue["onFileClick"];
	onFileOpen?: FileBrowserContextValue["onFileOpen"];
	onFolderPolicy?: FileBrowserContextValue["onFolderPolicy"];
	onFolderOpen: FileBrowserContextValue["onFolderOpen"];
	onGoToLocation?: FileBrowserContextValue["onGoToLocation"];
	onInfo?: FileBrowserContextValue["onInfo"];
	onManageTags?: FileBrowserContextValue["onManageTags"];
	onMove?: FileBrowserContextValue["onMove"];
	onRename?: FileBrowserContextValue["onRename"];
	onShare: (target: FileBrowserShareTarget) => void;
	onToggleLock: FileBrowserContextValue["onToggleLock"];
	onTrashPurge?: FileBrowserContextValue["onTrashPurge"];
	onTrashRestore?: FileBrowserContextValue["onTrashRestore"];
	onVersions?: FileBrowserContextValue["onVersions"];
}

type FileBrowserItemActionResolverInput =
	| {
			batchSelectionActions?: FileBrowserBatchSelectionActions | null;
			handlers: FileBrowserItemActionHandlers;
			isFolder: true;
			item: FolderListItem;
			readOnly?: boolean;
			selection: FileBrowserItemSelectionState;
			selectionEnabled?: boolean;
	  }
	| {
			batchSelectionActions?: FileBrowserBatchSelectionActions | null;
			handlers: FileBrowserItemActionHandlers;
			isFolder: false;
			item: FileListItem;
			readOnly?: boolean;
			selection: FileBrowserItemSelectionState;
			selectionEnabled?: boolean;
	  };

function shouldUseBatchSelectionMenu({
	batchSelectionActions,
	isFolder,
	item,
	selection,
	selectionEnabled,
}: FileBrowserItemActionResolverInput) {
	const selected = isFolder
		? selection.selectedFolderIds.has(item.id)
		: selection.selectedFileIds.has(item.id);
	const selectionCount =
		selection.selectedFileIds.size + selection.selectedFolderIds.size;

	return (
		selectionEnabled &&
		selected &&
		selectionCount > 1 &&
		batchSelectionActions != null
	);
}

function resolveBatchSelectionMenuProps({
	batchSelectionActions,
	isFolder,
}: FileBrowserItemActionResolverInput): ResolvedFileContextMenuProps {
	return {
		isFolder,
		isLocked: false,
		selectionCount: batchSelectionActions?.count,
		downloadAction: batchSelectionActions?.downloadAction,
		onArchiveCompress: batchSelectionActions?.onArchiveCompress,
		onCopy: batchSelectionActions?.onCopy,
		onMove: batchSelectionActions?.onMove,
		onManageTags: batchSelectionActions?.onManageTags,
		onDelete: batchSelectionActions?.onDelete,
		onTrashRestore: batchSelectionActions?.onRestore,
		onTrashPurge: batchSelectionActions?.onPurge,
	};
}

/** 回收站条目菜单：条目不可打开，主操作是恢复，危险操作是永久删除 */
function resolveTrashItemMenuProps(
	input: FileBrowserItemActionResolverInput,
): ResolvedFileContextMenuProps {
	const { handlers, isFolder, item } = input;
	const entityType = isFolder ? "folder" : "file";
	return {
		isFolder,
		isLocked: false,
		onTrashRestore: handlers.onTrashRestore
			? () => handlers.onTrashRestore?.(entityType, item.id)
			: undefined,
		onTrashPurge: handlers.onTrashPurge
			? () => handlers.onTrashPurge?.(entityType, item.id)
			: undefined,
	};
}

function resolveReadOnlyFolderMenuProps({
	handlers,
	item,
}: Extract<
	FileBrowserItemActionResolverInput,
	{ isFolder: true }
>): ResolvedFileContextMenuProps {
	return {
		isFolder: true,
		isLocked: false,
		onOpen: () => handlers.onFolderOpen(item.id, item.name),
		onArchiveDownload: handlers.onArchiveDownload
			? () => handlers.onArchiveDownload?.(item.id)
			: undefined,
	};
}

function resolveWritableFolderMenuProps({
	handlers,
	item,
}: Extract<
	FileBrowserItemActionResolverInput,
	{ isFolder: true }
>): ResolvedFileContextMenuProps {
	const isLocked = isResourceLocked(item.lock_state);
	const canToggleLock =
		item.lock_state.state === "unlocked" ||
		isDirectResourceLock(item.lock_state);
	return {
		isFolder: true,
		isLocked,
		onOpen: () => handlers.onFolderOpen(item.id, item.name),
		onPageShare: () =>
			handlers.onShare({
				folderId: item.id,
				name: item.name,
				initialMode: "page",
			}),
		onArchiveDownload: handlers.onArchiveDownload
			? () => handlers.onArchiveDownload?.(item.id)
			: undefined,
		onArchiveCompress: handlers.onArchiveCompress
			? () => handlers.onArchiveCompress?.("folder", item.id)
			: undefined,
		onCopy: handlers.onCopy
			? () => handlers.onCopy?.("folder", item.id)
			: undefined,
		onManageTags: handlers.onManageTags
			? () => handlers.onManageTags?.("folder", item.id)
			: undefined,
		onMove: handlers.onMove
			? () => handlers.onMove?.("folder", item.id)
			: undefined,
		onFolderPolicy: handlers.onFolderPolicy
			? () => handlers.onFolderPolicy?.(item)
			: undefined,
		onRename: handlers.onRename
			? () => handlers.onRename?.("folder", item.id, item.name)
			: undefined,
		onToggleLock: canToggleLock
			? () => handlers.onToggleLock("folder", item.id, isLocked)
			: undefined,
		onDelete: handlers.onDelete
			? () => handlers.onDelete?.("folder", item.id)
			: undefined,
		onInfo: () => handlers.onInfo?.("folder", item.id),
	};
}

function resolveReadOnlyFileMenuProps({
	handlers,
	item,
}: Extract<
	FileBrowserItemActionResolverInput,
	{ isFolder: false }
>): ResolvedFileContextMenuProps {
	return {
		isFolder: false,
		isLocked: false,
		onOpen: () => (handlers.onFileOpen ?? handlers.onFileClick)(item),
		onDownload: () => handlers.onDownload(item.id, item.name),
	};
}

function resolveWritableFileMenuProps({
	handlers,
	item,
}: Extract<
	FileBrowserItemActionResolverInput,
	{ isFolder: false }
>): ResolvedFileContextMenuProps {
	const isLocked = isResourceLocked(item.lock_state);
	const canToggleLock =
		item.lock_state.state === "unlocked" ||
		isDirectResourceLock(item.lock_state);
	return {
		isFolder: false,
		isLocked,
		onOpen: () => (handlers.onFileOpen ?? handlers.onFileClick)(item),
		onChooseOpenMethod: handlers.onFileChooseOpenMethod
			? () => handlers.onFileChooseOpenMethod?.(item)
			: undefined,
		onDownload: () => handlers.onDownload(item.id, item.name),
		onArchiveExtract:
			handlers.onArchiveExtract && isExtractableArchiveFileName(item.name)
				? () => handlers.onArchiveExtract?.(item.id)
				: undefined,
		onArchiveCompress: handlers.onArchiveCompress
			? () => handlers.onArchiveCompress?.("file", item.id)
			: undefined,
		onPageShare: () =>
			handlers.onShare({
				fileId: item.id,
				name: item.name,
				initialMode: "page",
			}),
		onDirectShare: () =>
			handlers.onShare({
				fileId: item.id,
				name: item.name,
				initialMode: "direct",
			}),
		onCopy: handlers.onCopy
			? () => handlers.onCopy?.("file", item.id)
			: undefined,
		onGoToLocation: handlers.onGoToLocation
			? () => handlers.onGoToLocation?.(item)
			: undefined,
		onManageTags: handlers.onManageTags
			? () => handlers.onManageTags?.("file", item.id)
			: undefined,
		onMove: handlers.onMove
			? () => handlers.onMove?.("file", item.id)
			: undefined,
		onRename: handlers.onRename
			? () => handlers.onRename?.("file", item.id, item.name)
			: undefined,
		onToggleLock: canToggleLock
			? () => handlers.onToggleLock("file", item.id, isLocked)
			: undefined,
		onDelete: handlers.onDelete
			? () => handlers.onDelete?.("file", item.id)
			: undefined,
		onVersions: handlers.onVersions
			? () => handlers.onVersions?.(item.id)
			: undefined,
		onInfo: () => handlers.onInfo?.("file", item.id),
	};
}

export function resolveFileBrowserItemMenuProps(
	input: FileBrowserItemActionResolverInput,
): ResolvedFileContextMenuProps {
	const selectionEnabled = input.selectionEnabled ?? !input.readOnly;
	const resolvedInput = { ...input, selectionEnabled };

	if (shouldUseBatchSelectionMenu(resolvedInput)) {
		return resolveBatchSelectionMenuProps(resolvedInput);
	}

	// trashMode 的浏览器以 readOnly + onTrashRestore 标识；
	// 回收站条目不可打开/下载，菜单只保留恢复与永久删除
	if (input.readOnly && input.handlers.onTrashRestore) {
		return resolveTrashItemMenuProps(input);
	}

	if (input.isFolder) {
		return input.readOnly
			? resolveReadOnlyFolderMenuProps(input)
			: resolveWritableFolderMenuProps(input);
	}

	return input.readOnly
		? resolveReadOnlyFileMenuProps(input)
		: resolveWritableFileMenuProps(input);
}
