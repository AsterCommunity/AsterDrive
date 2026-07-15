import type {
	FileBrowserBatchSelectionActions,
	FileBrowserContextValue,
	FileBrowserShareTarget,
} from "@/components/files/FileBrowserContext";
import type { FileContextMenuProps } from "@/components/files/FileContextMenu";
import { isExtractableArchiveFileName } from "@/lib/archiveFormats";
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
	return {
		isFolder: true,
		isLocked: item.is_locked ?? false,
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
		onToggleLock: () =>
			handlers.onToggleLock("folder", item.id, item.is_locked ?? false),
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
	return {
		isFolder: false,
		isLocked: item.is_locked ?? false,
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
		onToggleLock: () =>
			handlers.onToggleLock("file", item.id, item.is_locked ?? false),
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

	if (input.isFolder) {
		return input.readOnly
			? resolveReadOnlyFolderMenuProps(input)
			: resolveWritableFolderMenuProps(input);
	}

	return input.readOnly
		? resolveReadOnlyFileMenuProps(input)
		: resolveWritableFileMenuProps(input);
}
