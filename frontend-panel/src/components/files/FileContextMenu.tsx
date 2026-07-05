import {
	Fragment,
	isValidElement,
	type ReactElement,
	type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import {
	BUILTIN_FILE_ACTION_DESCRIPTORS,
	BUILTIN_FILE_SELECTION_ACTION_DESCRIPTORS,
	type FileActionId,
	type ResolvedFileAction,
	resolveFileActions,
} from "@/components/files/fileActionRegistry";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuGroup,
	ContextMenuItem,
	ContextMenuLabel,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuGroup,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Icon } from "@/components/ui/icon";
import type { FileBrowserSelectionDownloadAction } from "./FileBrowserContext";

export interface FileContextMenuProps {
	children: ReactNode;
	downloadAction?: FileBrowserSelectionDownloadAction;
	onOpen?: () => void;
	onChooseOpenMethod?: () => void;
	onDownload?: () => void;
	onArchiveExtract?: () => void;
	onArchiveCompress?: () => void;
	onArchiveDownload?: () => void;
	onPageShare?: () => void;
	onDirectShare?: () => void;
	onCopy?: () => void;
	onMove?: () => void;
	onFolderPolicy?: () => void;
	onGoToLocation?: () => void;
	onManageTags?: () => void;
	onToggleLock?: () => void;
	onDelete?: () => void;
	onRename?: () => void;
	onVersions?: () => void;
	onInfo?: () => void;
	isLocked: boolean;
	isFolder: boolean;
	renderTrigger?: boolean;
	selectionCount?: number;
}

type FileActionMenuProps = Omit<
	FileContextMenuProps,
	"children" | "renderTrigger"
>;

function menuActionHandlers({
	onOpen,
	onChooseOpenMethod,
	onDownload,
	onArchiveExtract,
	onArchiveCompress,
	onArchiveDownload,
	onPageShare,
	onDirectShare,
	onCopy,
	onMove,
	onFolderPolicy,
	onGoToLocation,
	onManageTags,
	onRename,
	onToggleLock,
	onDelete,
	onVersions,
	onInfo,
}: FileActionMenuProps): Partial<Record<FileActionId, () => void>> {
	return {
		archive_compress: onArchiveCompress,
		archive_download: onArchiveDownload,
		archive_extract: onArchiveExtract,
		choose_open_method: onChooseOpenMethod,
		copy: onCopy,
		delete: onDelete,
		download: onDownload,
		folder_policy: onFolderPolicy,
		go_to_location: onGoToLocation,
		info: onInfo,
		manage_tags: onManageTags,
		move: onMove,
		open: onOpen,
		rename: onRename,
		share_direct: onDirectShare,
		share_page: onPageShare,
		toggle_lock: onToggleLock,
		versions: onVersions,
	};
}

function FileContextMenuActionItem({
	action,
	item: Item,
}: {
	action: ResolvedFileAction;
	item: typeof ContextMenuItem | typeof DropdownMenuItem;
}) {
	const { t } = useTranslation(["files", "share", "tasks"]);
	const destructive = action.id === "delete";

	return (
		<Item
			onClick={action.onClick}
			disabled={action.disabled}
			variant={destructive ? "destructive" : undefined}
			className={destructive ? "text-destructive" : undefined}
		>
			<Icon name={action.icon} className="size-4 mr-2" />
			{t(action.labelKey)}
		</Item>
	);
}

function shouldSeparateSingleActionGroup(
	previous: ResolvedFileAction | undefined,
	current: ResolvedFileAction,
) {
	if (!previous) {
		return false;
	}
	if (
		previous.presentation.group === "open" &&
		current.presentation.group !== "open"
	) {
		return true;
	}
	return (
		(current.presentation.group === "metadata" ||
			current.presentation.group === "danger") &&
		previous.presentation.group !== "metadata" &&
		previous.presentation.group !== "danger"
	);
}

function FileContextMenuItems({
	downloadAction,
	onOpen,
	onChooseOpenMethod,
	onDownload,
	onArchiveExtract,
	onArchiveCompress,
	onArchiveDownload,
	onPageShare,
	onDirectShare,
	onCopy,
	onMove,
	onFolderPolicy,
	onGoToLocation,
	onManageTags,
	onRename,
	onToggleLock,
	onDelete,
	onVersions,
	onInfo,
	isLocked,
	isFolder,
	selectionCount,
	item: Item,
	separator: Separator,
	label: Label,
	group: Group,
}: FileActionMenuProps & {
	item: typeof ContextMenuItem | typeof DropdownMenuItem;
	separator: typeof ContextMenuSeparator | typeof DropdownMenuSeparator;
	label: typeof ContextMenuLabel | typeof DropdownMenuLabel;
	group: typeof ContextMenuGroup | typeof DropdownMenuGroup;
}) {
	const { t } = useTranslation(["files", "share", "tasks"]);
	const isSelectionMenu = selectionCount != null && selectionCount > 1;
	const actions = resolveFileActions(
		isSelectionMenu
			? BUILTIN_FILE_SELECTION_ACTION_DESCRIPTORS
			: BUILTIN_FILE_ACTION_DESCRIPTORS,
		{
			downloadAction,
			handlers: menuActionHandlers({
				downloadAction,
				onArchiveCompress,
				onArchiveDownload,
				onArchiveExtract,
				onChooseOpenMethod,
				onCopy,
				onDelete,
				onDirectShare,
				onDownload,
				onFolderPolicy,
				onGoToLocation,
				onInfo,
				onManageTags,
				onMove,
				onOpen,
				onPageShare,
				onRename,
				onToggleLock,
				onVersions,
				isLocked,
				isFolder,
				selectionCount,
			}),
			isFolder,
			isLocked,
			selectionCount,
		},
	);

	if (isSelectionMenu) {
		const primaryActions = actions.filter(
			(action) => action.presentation.group !== "danger",
		);
		const dangerActions = actions.filter(
			(action) => action.presentation.group === "danger",
		);

		return (
			<>
				<Group>
					<Label>{t("core:selected_count", { count: selectionCount })}</Label>
					{primaryActions.map((action) => (
						<FileContextMenuActionItem
							key={action.id}
							action={action}
							item={Item}
						/>
					))}
				</Group>
				{dangerActions.length > 0 && (
					<>
						<Separator />
						{dangerActions.map((action) => (
							<FileContextMenuActionItem
								key={action.id}
								action={action}
								item={Item}
							/>
						))}
					</>
				)}
			</>
		);
	}

	return (
		<>
			{actions.map((action, index) => (
				<Fragment key={action.id}>
					{shouldSeparateSingleActionGroup(actions[index - 1], action) ? (
						<Separator />
					) : null}
					<FileContextMenuActionItem action={action} item={Item} />
				</Fragment>
			))}
		</>
	);
}

export function FileContextDropdownMenu({
	trigger,
	...props
}: FileActionMenuProps & {
	trigger: ReactElement;
}) {
	return (
		<DropdownMenu>
			<DropdownMenuTrigger render={trigger} />
			<DropdownMenuContent align="end" className="w-auto min-w-44">
				<FileContextMenuItems
					{...props}
					item={DropdownMenuItem}
					separator={DropdownMenuSeparator}
					label={DropdownMenuLabel}
					group={DropdownMenuGroup}
				/>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

export function FileContextMenu({
	children,
	downloadAction,
	onOpen,
	onChooseOpenMethod,
	onDownload,
	onArchiveExtract,
	onArchiveCompress,
	onArchiveDownload,
	onPageShare,
	onDirectShare,
	onCopy,
	onMove,
	onFolderPolicy,
	onGoToLocation,
	onManageTags,
	onRename,
	onToggleLock,
	onDelete,
	onVersions,
	onInfo,
	isLocked,
	isFolder,
	renderTrigger = false,
	selectionCount,
}: FileContextMenuProps) {
	const trigger =
		renderTrigger && isValidElement(children) ? (
			<ContextMenuTrigger render={children} />
		) : (
			<ContextMenuTrigger className="w-full">{children}</ContextMenuTrigger>
		);

	return (
		<ContextMenu>
			{trigger}
			<ContextMenuContent>
				<FileContextMenuItems
					downloadAction={downloadAction}
					onOpen={onOpen}
					onChooseOpenMethod={onChooseOpenMethod}
					onDownload={onDownload}
					onArchiveExtract={onArchiveExtract}
					onArchiveCompress={onArchiveCompress}
					onArchiveDownload={onArchiveDownload}
					onPageShare={onPageShare}
					onDirectShare={onDirectShare}
					onCopy={onCopy}
					onMove={onMove}
					onFolderPolicy={onFolderPolicy}
					onGoToLocation={onGoToLocation}
					onManageTags={onManageTags}
					onRename={onRename}
					onToggleLock={onToggleLock}
					onDelete={onDelete}
					onVersions={onVersions}
					onInfo={onInfo}
					isLocked={isLocked}
					isFolder={isFolder}
					selectionCount={selectionCount}
					item={ContextMenuItem}
					separator={ContextMenuSeparator}
					label={ContextMenuLabel}
					group={ContextMenuGroup}
				/>
			</ContextMenuContent>
		</ContextMenu>
	);
}
