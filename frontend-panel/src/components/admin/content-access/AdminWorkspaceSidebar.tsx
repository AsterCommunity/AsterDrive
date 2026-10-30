import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import { FolderTreeItemContent } from "@/components/folders/folder-tree/FolderTreeItemContent";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ScrollArea } from "@/components/ui/scroll-area";
import { handleApiError } from "@/hooks/useApiError";
import { formatBytes } from "@/lib/format";
import { getUserDisplayName } from "@/lib/user";
import { folderTreeRowClass } from "@/lib/utils";
import { adminContentAccessService } from "@/services/adminContentAccessService";
import type { AdminTeamInfo, FolderListItem, UserInfo } from "@/types/api";

export interface AdminWorkspaceIdentity {
	user?: UserInfo;
	team?: AdminTeamInfo;
}
function ScopedFolderChildren({
	accessId,
	parentId,
	currentId,
	onNavigate,
	depth = 0,
}: {
	accessId: string;
	parentId: number | null;
	currentId?: number;
	onNavigate: (id?: number) => void;
	depth?: number;
}) {
	const { t } = useTranslation(["files", "core"]);
	const [items, setItems] = useState<FolderListItem[]>([]);
	const [offset, setOffset] = useState(0);
	const [total, setTotal] = useState(0);
	const [busy, setBusy] = useState(true);
	const [failed, setFailed] = useState(false);
	const [expanded, setExpanded] = useState<Set<number>>(new Set());
	useEffect(() => {
		let active = true;
		setBusy(true);
		setFailed(false);
		adminContentAccessService
			.list({
				access_id: accessId,
				folder_id: parentId ?? undefined,
				folder_limit: 100,
				folder_offset: offset,
				file_limit: 0,
			})
			.then((page) => {
				if (active) {
					setItems((previous) =>
						offset === 0
							? page.contents.folders
							: [...previous, ...page.contents.folders],
					);
					setTotal(page.contents.folders_total);
				}
			})
			.catch((error) => {
				if (active) {
					setFailed(true);
					handleApiError(error);
				}
			})
			.finally(() => {
				if (active) setBusy(false);
			});
		return () => {
			active = false;
		};
	}, [accessId, parentId, offset]);
	return (
		<>
			{items.map((folder) => (
				<div key={folder.id}>
					<div
						className={folderTreeRowClass(currentId === folder.id)}
						style={{ paddingLeft: `${depth * 12}px` }}
					>
						<FolderTreeItemContent
							label={folder.name}
							icon={folder.icon}
							expanded={expanded.has(folder.id)}
							showToggle={depth < 64}
							toggleLabel={t(
								expanded.has(folder.id) ? "collapse_tree" : "expand_tree",
							)}
							onNavigate={() => onNavigate(folder.id)}
							onToggle={() =>
								setExpanded((previous) => {
									const next = new Set(previous);
									if (next.has(folder.id)) next.delete(folder.id);
									else next.add(folder.id);
									return next;
								})
							}
						/>
					</div>
					{expanded.has(folder.id) && depth < 64 ? (
						<ScopedFolderChildren
							accessId={accessId}
							parentId={folder.id}
							currentId={currentId}
							onNavigate={onNavigate}
							depth={depth + 1}
						/>
					) : null}
				</div>
			))}
			{busy ? (
				<p className="px-3 py-2 text-xs text-muted-foreground">
					{t("core:loading")}
				</p>
			) : failed ? (
				<p role="alert" className="px-3 py-2 text-xs">
					{t("admin:content_workspace_unavailable")}
				</p>
			) : items.length < total ? (
				<Button
					variant="ghost"
					size="sm"
					onClick={() => setOffset(items.length)}
				>
					{t("admin:content_next_folders")}
				</Button>
			) : null}
		</>
	);
}

export function AdminWorkspaceSidebar({
	accessId,
	user,
	team,
	currentId,
	onNavigate,
	onEnd,
	inTrash = false,
	onTrash,
}: AdminWorkspaceIdentity & {
	accessId?: string;
	currentId?: number;
	onNavigate?: (id?: number) => void;
	onEnd?: () => void;
	inTrash?: boolean;
	onTrash?: () => void;
}) {
	const { t } = useTranslation(["admin", "files"]);
	const [expanded, setExpanded] = useState(true);
	const name = user
		? getUserDisplayName(user)
		: (team?.name ?? t("content_workspace"));
	return (
		<div className="flex h-full min-h-0 flex-col">
			<div className="px-4 py-3 text-sm font-semibold">
				{t("content_workspace_read_only_short")}
			</div>
			<ScrollArea className="min-h-0 flex-1">
				{accessId && onNavigate ? (
					<div className="px-3 py-2">
						<div className={folderTreeRowClass(!inTrash && currentId == null)}>
							<FolderTreeItemContent
								expanded={expanded}
								label={t("files:root")}
								showToggle
								toggleLabel={t(
									expanded ? "files:collapse_tree" : "files:expand_tree",
								)}
								onNavigate={() => onNavigate()}
								onToggle={() => setExpanded((value) => !value)}
							/>
						</div>
						{expanded ? (
							<ScopedFolderChildren
								key={accessId}
								accessId={accessId}
								parentId={null}
								currentId={currentId}
								onNavigate={onNavigate}
							/>
						) : null}
					</div>
				) : null}
				{accessId && onTrash ? (
					<div className="px-3 py-2">
						<Button
							variant={inTrash ? "secondary" : "ghost"}
							className="w-full justify-start"
							onClick={onTrash}
						>
							<Icon name="Trash" className="mr-2 size-4" />
							{t("core:trash")}
						</Button>
					</div>
				) : null}
			</ScrollArea>
			<div
				className="space-y-3 border-t p-4"
				data-testid="admin-workspace-identity"
			>
				<div className="flex items-center gap-3">
					{user ? (
						<UserAvatarImage
							avatar={user.profile.avatar}
							name={name}
							size="md"
						/>
					) : (
						<Icon name="User" className="size-9" />
					)}
					<div className="min-w-0">
						<p className="truncate text-sm font-semibold">{name}</p>
						<p className="truncate text-xs text-muted-foreground">
							{user
								? `@${user.username}`
								: t("content_team_owner", { id: team?.id })}
						</p>
					</div>
				</div>
				{user || team ? (
					<p className="text-xs text-muted-foreground">
						{formatBytes((user ?? team)?.storage_used ?? 0)} ·{" "}
						{t(user ? "content_personal_owner" : "content_team_owner", {
							id: (user ?? team)?.id,
						})}
					</p>
				) : null}
				{onEnd ? (
					<Button className="w-full" variant="outline" onClick={onEnd}>
						{t("content_access_end")}
					</Button>
				) : null}
			</div>
		</div>
	);
}
