import type { ReactElement, ReactNode } from "react";
import { isValidElement } from "react";
import { useTranslation } from "react-i18next";
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
	const selectionDownloadLabel =
		downloadAction?.kind === "file"
			? t("download")
			: t("tasks:archive_download_action");

	if (isSelectionMenu) {
		return (
			<>
				<Group>
					<Label>{t("core:selected_count", { count: selectionCount })}</Label>
					{downloadAction && (
						<Item onClick={downloadAction.onClick}>
							<Icon name="Download" className="size-4 mr-2" />
							{selectionDownloadLabel}
						</Item>
					)}
					{onArchiveCompress && (
						<Item onClick={onArchiveCompress}>
							<Icon name="FileZip" className="size-4 mr-2" />
							{t("tasks:archive_compress_action")}
						</Item>
					)}
					{onCopy && (
						<Item onClick={onCopy}>
							<Icon name="Copy" className="size-4 mr-2" />
							{t("copy_to")}
						</Item>
					)}
					{onMove && (
						<Item onClick={onMove}>
							<Icon name="ArrowsOutCardinal" className="size-4 mr-2" />
							{t("move_to")}
						</Item>
					)}
					{onManageTags && (
						<Item onClick={onManageTags}>
							<Icon name="Tag" className="size-4 mr-2" />
							{t("tag_manage")}
						</Item>
					)}
				</Group>
				{onDelete && (
					<>
						<Separator />
						<Item
							onClick={onDelete}
							variant="destructive"
							className="text-destructive"
						>
							<Icon name="Trash" className="size-4 mr-2" />
							{t("core:delete")}
						</Item>
					</>
				)}
			</>
		);
	}

	return (
		<>
			{onOpen && (
				<Item onClick={onOpen}>
					<Icon name="Eye" className="size-4 mr-2" />
					{t("open")}
				</Item>
			)}
			{!isFolder && onChooseOpenMethod && (
				<Item onClick={onChooseOpenMethod}>
					<Icon name="ListBullets" className="size-4 mr-2" />
					{t("open_with_action")}
				</Item>
			)}
			{onOpen || (!isFolder && onChooseOpenMethod) ? <Separator /> : null}
			{!isFolder && onDownload && (
				<Item onClick={onDownload}>
					<Icon name="Download" className="size-4 mr-2" />
					{t("download")}
				</Item>
			)}
			{!isFolder && onArchiveExtract && (
				<Item onClick={onArchiveExtract}>
					<Icon name="FolderOpen" className="size-4 mr-2" />
					{t("tasks:archive_extract_action")}
				</Item>
			)}
			{onArchiveCompress && (
				<Item onClick={onArchiveCompress}>
					<Icon name="FileZip" className="size-4 mr-2" />
					{t("tasks:archive_compress_action")}
				</Item>
			)}
			{isFolder && onArchiveDownload && (
				<Item onClick={onArchiveDownload}>
					<Icon name="Download" className="size-4 mr-2" />
					{t("tasks:archive_download_action")}
				</Item>
			)}
			{onPageShare && (
				<Item onClick={onPageShare}>
					<Icon name="Link" className="size-4 mr-2" />
					{t("share")}
				</Item>
			)}
			{!isFolder && onDirectShare && (
				<Item onClick={onDirectShare}>
					<Icon name="LinkSimple" className="size-4 mr-2" />
					{t("share:share_direct_link_action")}
				</Item>
			)}
			{onCopy && (
				<Item onClick={onCopy}>
					<Icon name="Copy" className="size-4 mr-2" />
					{t("copy_to")}
				</Item>
			)}
			{onMove && (
				<Item onClick={onMove}>
					<Icon name="ArrowsOutCardinal" className="size-4 mr-2" />
					{t("move_to")}
				</Item>
			)}
			{isFolder && onFolderPolicy && (
				<Item onClick={onFolderPolicy}>
					<Icon name="HardDrive" className="size-4 mr-2" />
					{t("folder_policy")}
				</Item>
			)}
			{!isFolder && onGoToLocation && (
				<Item onClick={onGoToLocation}>
					<Icon name="FolderOpen" className="size-4 mr-2" />
					{t("go_to_file_location")}
				</Item>
			)}
			{onRename && (
				<Item onClick={onRename}>
					<Icon name="PencilSimple" className="size-4 mr-2" />
					{t("rename")}
				</Item>
			)}
			{onManageTags && (
				<Item onClick={onManageTags}>
					<Icon name="Tag" className="size-4 mr-2" />
					{t("tag_manage")}
				</Item>
			)}
			{!isFolder && onVersions && (
				<Item onClick={onVersions}>
					<Icon name="Clock" className="size-4 mr-2" />
					{t("versions")}
				</Item>
			)}
			{(onInfo || onToggleLock || onDelete) && <Separator />}
			{onInfo && (
				<Item onClick={onInfo}>
					<Icon name="Info" className="size-4 mr-2" />
					{t("info")}
				</Item>
			)}
			{onToggleLock && (
				<Item onClick={onToggleLock}>
					{isLocked ? (
						<>
							<Icon name="LockOpen" className="size-4 mr-2" />
							{t("unlock")}
						</>
					) : (
						<>
							<Icon name="Lock" className="size-4 mr-2" />
							{t("lock")}
						</>
					)}
				</Item>
			)}
			{onDelete && (
				<Item
					onClick={onDelete}
					disabled={isLocked}
					variant="destructive"
					className="text-destructive"
				>
					<Icon name="Trash" className="size-4 mr-2" />
					{t("core:delete")}
				</Item>
			)}
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
