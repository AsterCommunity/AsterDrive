import { useTranslation } from "react-i18next";
import {
	ADMIN_INTERACTIVE_TABLE_ROW_CLASS,
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_STACKED_CELL_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminSortableTableHead,
	AdminTableShell,
	AdminTable as Table,
	AdminTableBody as TableBody,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { UserIdentity } from "@/components/common/UserIdentity";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import {
	ADMIN_ICON_BUTTON_CLASS,
	ADMIN_TABLE_ACTIONS_WIDTH_CLASS,
} from "@/lib/constants";
import { formatBytes, formatDateShort } from "@/lib/format";
import type { SortOrder } from "@/lib/pagination";
import type { AdminTeamSortBy } from "@/types/adminSort";
import type { AdminTeamInfo } from "@/types/api";

interface AdminTeamsTableProps {
	onOpenTeam: (team: AdminTeamInfo) => void;
	policyGroupNameById: (
		policyGroupId: number | null | undefined,
	) => string | null;
	sortBy: AdminTeamSortBy;
	sortOrder: SortOrder;
	teams: AdminTeamInfo[];
	onSortChange: (sortBy: AdminTeamSortBy, sortOrder: SortOrder) => void;
}

interface AdminTeamsTableHeaderProps {
	sortBy: AdminTeamSortBy;
	sortOrder: SortOrder;
	onSortChange: (sortBy: AdminTeamSortBy, sortOrder: SortOrder) => void;
}

interface AdminTeamsTableRowProps {
	onOpenTeam: (team: AdminTeamInfo) => void;
	policyGroupNameById: (
		policyGroupId: number | null | undefined,
	) => string | null;
	team: AdminTeamInfo;
}

export function AdminTeamsTableHeader({
	onSortChange,
	sortBy,
	sortOrder,
}: AdminTeamsTableHeaderProps) {
	const { t } = useTranslation(["admin", "core"]);

	return (
		<TableHeader>
			<TableRow>
				<AdminSortableTableHead
					sortKey="name"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={onSortChange}
				>
					{t("core:name")}
				</AdminSortableTableHead>
				<TableHead>{t("created_by")}</TableHead>
				<TableHead className="w-28">{t("member_count")}</TableHead>
				<AdminSortableTableHead
					className="w-[220px]"
					sortKey="storage_used"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={onSortChange}
				>
					{t("quota")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					className="w-36"
					sortKey="created_at"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={onSortChange}
				>
					{t("core:created_at")}
				</AdminSortableTableHead>
				<TableHead className={ADMIN_TABLE_ACTIONS_WIDTH_CLASS}>
					{t("core:actions")}
				</TableHead>
			</TableRow>
		</TableHeader>
	);
}

export function AdminTeamsTableRow({
	onOpenTeam,
	policyGroupNameById,
	team,
}: AdminTeamsTableRowProps) {
	const { t } = useTranslation(["admin", "core"]);

	return (
		<TableRow
			key={team.id}
			className={ADMIN_INTERACTIVE_TABLE_ROW_CLASS}
			onClick={() => onOpenTeam(team)}
			onKeyDown={(event) => {
				if (event.key === "Enter" || event.key === " ") {
					event.preventDefault();
					onOpenTeam(team);
				}
			}}
			tabIndex={0}
		>
			<TableCell>
				<div className="flex min-w-0 flex-col gap-1.5 text-left">
					<div className="flex flex-wrap items-center gap-2">
						<span className="font-medium text-foreground">{team.name}</span>
						<Badge variant="outline">#{team.id}</Badge>
						{team.archived_at ? (
							<Badge variant="outline">{t("archived_badge")}</Badge>
						) : null}
					</div>
					{team.description ? (
						<p className="max-w-md text-xs text-muted-foreground">
							{team.description}
						</p>
					) : null}
				</div>
			</TableCell>
			<TableCell>
				<UserIdentity user={team.created_by} />
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
					<span className="text-sm font-medium text-foreground">
						{team.member_count}
					</span>
				</div>
			</TableCell>
			<TableCell>
				<TeamStorageCell
					team={team}
					policyGroupName={policyGroupNameById(team.policy_group_id)}
				/>
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
					<span className="text-sm text-muted-foreground">
						{formatDateShort(team.archived_at ?? team.created_at)}
					</span>
				</div>
			</TableCell>
			<TableCell
				onClick={(event) => event.stopPropagation()}
				onKeyDown={(event) => event.stopPropagation()}
			>
				<div className="flex justify-end">
					<Button
						variant="ghost"
						size="icon"
						className={ADMIN_ICON_BUTTON_CLASS}
						onClick={() => onOpenTeam(team)}
						title={t("view_details")}
						aria-label={t("view_details")}
					>
						<Icon name="CaretRight" className="size-3.5" />
					</Button>
				</div>
			</TableCell>
		</TableRow>
	);
}

export function AdminTeamsTable({
	onOpenTeam,
	onSortChange,
	policyGroupNameById,
	sortBy,
	sortOrder,
	teams,
}: AdminTeamsTableProps) {
	return (
		<AdminTableShell>
			<Table>
				<AdminTeamsTableHeader
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={onSortChange}
				/>
				<TableBody>
					{teams.map((team) => (
						<AdminTeamsTableRow
							key={team.id}
							onOpenTeam={onOpenTeam}
							policyGroupNameById={policyGroupNameById}
							team={team}
						/>
					))}
				</TableBody>
			</Table>
		</AdminTableShell>
	);
}

function TeamStorageCell({
	policyGroupName,
	team,
}: {
	policyGroupName: string | null;
	team: AdminTeamInfo;
}) {
	const { t } = useTranslation(["admin", "core"]);

	return (
		<div className={ADMIN_TABLE_STACKED_CELL_CLASS}>
			<span className="text-sm font-medium text-foreground">
				{formatBytes(team.storage_used)}
				{team.storage_quota > 0
					? ` / ${formatBytes(team.storage_quota)}`
					: ` / ${t("core:unlimited")}`}
			</span>
			<span className="truncate text-xs text-muted-foreground">
				#{team.id}
				{team.policy_group_id != null
					? ` · ${policyGroupName ?? `PG ${team.policy_group_id}`}`
					: ""}
			</span>
		</div>
	);
}
