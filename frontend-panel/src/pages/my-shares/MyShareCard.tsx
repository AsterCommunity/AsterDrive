import {
	FileTypeIcon,
	getFileBadgeTint,
} from "@/components/files/FileTypeIcon";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { Icon } from "@/components/ui/icon";
import { ItemCheckbox } from "@/components/ui/item-checkbox";
import { formatDateAbsolute } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { MyShareInfo } from "@/types/api";
import { MyShareStatusBadge } from "./MyShareStatusBadge";

interface MyShareCardLabels {
	active: string;
	copy: string;
	created: (date: string) => string;
	delete: string;
	deleted: string;
	edit: string;
	exhausted: string;
	expire: (date: string) => string;
	expired: string;
	never: string;
	open: string;
}

interface MyShareCardProps {
	labels: MyShareCardLabels;
	onCopy: (share: MyShareInfo) => void;
	onDelete: (share: MyShareInfo) => void;
	onEdit: (share: MyShareInfo) => void;
	onOpen: (share: MyShareInfo) => void;
	onToggleSelect: (shareId: number) => void;
	selected: boolean;
	share: MyShareInfo;
}

export function MyShareCard({
	labels,
	onCopy,
	onDelete,
	onEdit,
	onOpen,
	onToggleSelect,
	selected,
	share,
}: MyShareCardProps) {
	const isFolder = share.resource_type === "folder";

	return (
		<ContextMenu>
			<ContextMenuTrigger className="w-full">
				{/* D9 行式列表（定稿概念图 concept-shares-rows）：分享管理是状态列表而非
				    内容浏览，单行色垫条目与 tasks 页同构；图标垫接 D4 类型色板 */}
				{/* biome-ignore lint/a11y/useSemanticElements: 条目内嵌选择框，不能用 button 套 button */}
				<div
					className={cn(
						"flex cursor-pointer items-center gap-3 rounded-xl bg-muted/45 px-4 py-3 transition-colors duration-150 hover:bg-muted/60 dark:bg-muted/20 dark:hover:bg-muted/30",
						selected && "bg-accent/60 dark:bg-accent/40",
					)}
					onClick={() => onOpen(share)}
					role="button"
					tabIndex={0}
					onKeyDown={(event) => {
						if (event.key === "Enter") {
							onOpen(share);
						}
					}}
				>
					<ItemCheckbox
						checked={selected}
						onChange={() => onToggleSelect(share.id)}
					/>
					<div
						className={cn(
							"flex size-10 shrink-0 items-center justify-center rounded-lg",
							isFolder
								? "bg-amber-500/10 dark:bg-amber-400/15"
								: getFileBadgeTint({
										mimeType: "",
										fileName: share.resource_name,
									}),
						)}
					>
						{isFolder ? (
							<Icon name="Folder" className="size-5 text-amber-500" />
						) : (
							<FileTypeIcon
								mimeType=""
								fileName={share.resource_name}
								className="size-5"
							/>
						)}
					</div>
					<div className="min-w-0 flex-1">
						<span className="block truncate text-sm font-semibold">
							{share.resource_name}
						</span>
						<span className="mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted-foreground">
							<span>
								{labels.created(formatDateAbsolute(share.created_at))}
							</span>
							{share.expires_at ? (
								<span>
									{labels.expire(formatDateAbsolute(share.expires_at))}
								</span>
							) : (
								<span>{labels.never}</span>
							)}
							{share.has_password ? (
								<Icon name="Lock" className="size-3" />
							) : null}
						</span>
					</div>
					<div className="shrink-0">
						<MyShareStatusBadge
							status={share.status}
							activeLabel={labels.active}
							expiredLabel={labels.expired}
							exhaustedLabel={labels.exhausted}
							deletedLabel={labels.deleted}
						/>
					</div>
				</div>
			</ContextMenuTrigger>
			<ContextMenuContent>
				<ContextMenuItem onClick={() => onEdit(share)}>
					<Icon name="PencilSimple" />
					{labels.edit}
				</ContextMenuItem>
				<ContextMenuItem onClick={() => onCopy(share)}>
					<Icon name="Copy" />
					{labels.copy}
				</ContextMenuItem>
				<ContextMenuItem onClick={() => onOpen(share)}>
					<Icon name="ArrowSquareOut" />
					{labels.open}
				</ContextMenuItem>
				<ContextMenuSeparator />
				<ContextMenuItem variant="destructive" onClick={() => onDelete(share)}>
					<Icon name="Trash" />
					{labels.delete}
				</ContextMenuItem>
			</ContextMenuContent>
		</ContextMenu>
	);
}
