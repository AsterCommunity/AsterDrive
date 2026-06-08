import type { ReactNode } from "react";
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
import { Icon } from "@/components/ui/icon";
import type { FileBrowserSelectionDownloadAction } from "./FileBrowserContext";

interface FileContextMenuProps {
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
	const { t } = useTranslation(["files", "share", "tasks"]);
	const isSelectionMenu = selectionCount != null && selectionCount > 1;
	const selectionDownloadLabel =
		downloadAction?.kind === "file"
			? t("download")
			: t("tasks:archive_download_action");

	const trigger =
		renderTrigger && isValidElement(children) ? (
			<ContextMenuTrigger render={children} />
		) : (
			<ContextMenuTrigger className="w-full">{children}</ContextMenuTrigger>
		);

	if (isSelectionMenu) {
		return (
			<ContextMenu>
				{trigger}
				<ContextMenuContent>
					<ContextMenuGroup>
						<ContextMenuLabel>
							{t("core:selected_count", { count: selectionCount })}
						</ContextMenuLabel>
						{downloadAction && (
							<ContextMenuItem onClick={downloadAction.onClick}>
								<Icon name="Download" className="size-4 mr-2" />
								{selectionDownloadLabel}
							</ContextMenuItem>
						)}
						{onArchiveCompress && (
							<ContextMenuItem onClick={onArchiveCompress}>
								<Icon name="FileZip" className="size-4 mr-2" />
								{t("tasks:archive_compress_action")}
							</ContextMenuItem>
						)}
						{onCopy && (
							<ContextMenuItem onClick={onCopy}>
								<Icon name="Copy" className="size-4 mr-2" />
								{t("copy_to")}
							</ContextMenuItem>
						)}
						{onMove && (
							<ContextMenuItem onClick={onMove}>
								<Icon name="ArrowsOutCardinal" className="size-4 mr-2" />
								{t("move_to")}
							</ContextMenuItem>
						)}
						{onManageTags && (
							<ContextMenuItem onClick={onManageTags}>
								<Icon name="Tag" className="size-4 mr-2" />
								{t("tag_manage")}
							</ContextMenuItem>
						)}
					</ContextMenuGroup>
					{onDelete && (
						<>
							<ContextMenuSeparator />
							<ContextMenuItem
								onClick={onDelete}
								variant="destructive"
								className="text-destructive"
							>
								<Icon name="Trash" className="size-4 mr-2" />
								{t("core:delete")}
							</ContextMenuItem>
						</>
					)}
				</ContextMenuContent>
			</ContextMenu>
		);
	}

	return (
		<ContextMenu>
			{trigger}
			<ContextMenuContent>
				{onOpen && (
					<ContextMenuItem onClick={onOpen}>
						<Icon name="Eye" className="size-4 mr-2" />
						{t("open")}
					</ContextMenuItem>
				)}
				{!isFolder && onChooseOpenMethod && (
					<ContextMenuItem onClick={onChooseOpenMethod}>
						<Icon name="ListBullets" className="size-4 mr-2" />
						{t("open_with_action")}
					</ContextMenuItem>
				)}
				{onOpen || (!isFolder && onChooseOpenMethod) ? (
					<ContextMenuSeparator />
				) : null}
				{!isFolder && onDownload && (
					<ContextMenuItem onClick={onDownload}>
						<Icon name="Download" className="size-4 mr-2" />
						{t("download")}
					</ContextMenuItem>
				)}
				{!isFolder && onArchiveExtract && (
					<ContextMenuItem onClick={onArchiveExtract}>
						<Icon name="FolderOpen" className="size-4 mr-2" />
						{t("tasks:archive_extract_action")}
					</ContextMenuItem>
				)}
				{onArchiveCompress && (
					<ContextMenuItem onClick={onArchiveCompress}>
						<Icon name="FileZip" className="size-4 mr-2" />
						{t("tasks:archive_compress_action")}
					</ContextMenuItem>
				)}
				{isFolder && onArchiveDownload && (
					<ContextMenuItem onClick={onArchiveDownload}>
						<Icon name="Download" className="size-4 mr-2" />
						{t("tasks:archive_download_action")}
					</ContextMenuItem>
				)}
				{onPageShare && (
					<ContextMenuItem onClick={onPageShare}>
						<Icon name="Link" className="size-4 mr-2" />
						{t("share")}
					</ContextMenuItem>
				)}
				{!isFolder && onDirectShare && (
					<ContextMenuItem onClick={onDirectShare}>
						<Icon name="LinkSimple" className="size-4 mr-2" />
						{t("share:share_direct_link_action")}
					</ContextMenuItem>
				)}
				{onCopy && (
					<ContextMenuItem onClick={onCopy}>
						<Icon name="Copy" className="size-4 mr-2" />
						{t("copy_to")}
					</ContextMenuItem>
				)}
				{onMove && (
					<ContextMenuItem onClick={onMove}>
						<Icon name="ArrowsOutCardinal" className="size-4 mr-2" />
						{t("move_to")}
					</ContextMenuItem>
				)}
				{onRename && (
					<ContextMenuItem onClick={onRename}>
						<Icon name="PencilSimple" className="size-4 mr-2" />
						{t("rename")}
					</ContextMenuItem>
				)}
				{onManageTags && (
					<ContextMenuItem onClick={onManageTags}>
						<Icon name="Tag" className="size-4 mr-2" />
						{t("tag_manage")}
					</ContextMenuItem>
				)}
				{!isFolder && onVersions && (
					<ContextMenuItem onClick={onVersions}>
						<Icon name="Clock" className="size-4 mr-2" />
						{t("versions")}
					</ContextMenuItem>
				)}
				{(onInfo || onToggleLock || onDelete) && <ContextMenuSeparator />}
				{onInfo && (
					<ContextMenuItem onClick={onInfo}>
						<Icon name="Info" className="size-4 mr-2" />
						{t("info")}
					</ContextMenuItem>
				)}
				{onToggleLock && (
					<ContextMenuItem onClick={onToggleLock}>
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
					</ContextMenuItem>
				)}
				{onDelete && (
					<ContextMenuItem
						onClick={onDelete}
						disabled={isLocked}
						className="text-destructive"
					>
						<Icon name="Trash" className="size-4 mr-2" />
						{t("core:delete")}
					</ContextMenuItem>
				)}
			</ContextMenuContent>
		</ContextMenu>
	);
}
