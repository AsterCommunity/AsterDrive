import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import { UserIdentity } from "@/components/common/UserIdentity";
import { AnimatedTreeGroup } from "@/components/folders/folder-tree/AnimatedTreeGroup";
import { FolderTreeRootRow } from "@/components/folders/folder-tree/FolderTreeRootRow";
import { FolderTreeRow } from "@/components/folders/folder-tree/FolderTreeRow";
import { SidebarNavigation } from "@/components/layout/SidebarNavigation";
import { WorkspaceSidebarLayout } from "@/components/layout/WorkspaceSidebarLayout";
import { Button } from "@/components/ui/button";
import { handleApiError } from "@/hooks/useApiError";
import { SIDEBAR_SECTION_PADDING_CLASS } from "@/lib/constants";
import { formatBytes } from "@/lib/format";
import { getUserDisplayName } from "@/lib/user";
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
	depth = 1,
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
					<FolderTreeRow
						active={currentId === folder.id}
						folderId={folder.id}
						depth={depth}
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

					<AnimatedTreeGroup open={expanded.has(folder.id) && depth < 64}>
						{expanded.has(folder.id) && depth < 64 ? (
							<ScopedFolderChildren
								accessId={accessId}
								parentId={folder.id}
								currentId={currentId}
								onNavigate={onNavigate}
								depth={depth + 1}
							/>
						) : null}
					</AnimatedTreeGroup>
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
		<WorkspaceSidebarLayout
			header={
				<p className="px-1 py-1 text-sm font-semibold">
					{t("content_workspace_read_only_short")}
				</p>
			}
			tree={
				accessId && onNavigate ? (
					<div className={`${SIDEBAR_SECTION_PADDING_CLASS} py-2 space-y-0.5`}>
						<FolderTreeRootRow
							active={!inTrash && currentId == null}
							expanded={expanded}
							onClick={() => onNavigate()}
							onToggle={() => setExpanded((value) => !value)}
						/>
						<AnimatedTreeGroup open={expanded}>
							{expanded ? (
								<ScopedFolderChildren
									key={accessId}
									accessId={accessId}
									parentId={null}
									currentId={inTrash ? undefined : currentId}
									onNavigate={onNavigate}
								/>
							) : null}
						</AnimatedTreeGroup>
					</div>
				) : null
			}
			navigation={
				accessId && onTrash ? (
					<SidebarNavigation
						items={[
							{
								key: "trash",
								icon: "Trash",
								label: t("core:trash"),
								active: inTrash,
								onClick: onTrash,
							},
						]}
					/>
				) : null
			}
			footer={
				<div
					className="space-y-3 border-t p-4"
					data-testid="admin-workspace-identity"
				>
					{user ? (
						<UserIdentity
							user={user}
							size="md"
							className="gap-3"
							avatarClassName="size-10 rounded-2xl text-sm"
						/>
					) : (
						<div className="flex items-center gap-3">
							<UserAvatarImage avatar={null} name={name} size="md" />
							<div className="min-w-0">
								<p className="truncate text-sm font-semibold">{name}</p>
								<p className="truncate text-xs text-muted-foreground">
									{t("content_team_owner", { id: team?.id })}
								</p>
							</div>
						</div>
					)}
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
			}
		/>
	);
}
