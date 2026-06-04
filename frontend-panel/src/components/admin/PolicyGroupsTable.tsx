import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
	ADMIN_INTERACTIVE_TABLE_ROW_CLASS,
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_STACKED_CELL_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminSortableTableHead,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { AdminTableList } from "@/components/common/AdminTableList";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import {
	ADMIN_ICON_BUTTON_CLASS,
	ADMIN_TABLE_ACTIONS_WIDTH_CLASS,
} from "@/lib/constants";
import { formatBytes, formatDateAbsolute } from "@/lib/format";
import type { SortOrder } from "@/lib/pagination";
import type { AdminPolicyGroupSortBy } from "@/types/adminSort";
import type { StoragePolicyGroup, StoragePolicyGroupItem } from "@/types/api";

type AdminT = ReturnType<typeof useTranslation>["t"];
type PageSizeOption = {
	label: string;
	value: string;
};

function getRuleRangeLabel(
	t: AdminT,
	item: Pick<StoragePolicyGroupItem, "min_file_size" | "max_file_size">,
) {
	if (item.min_file_size <= 0 && item.max_file_size <= 0) {
		return t("policy_group_range_any");
	}
	if (item.min_file_size > 0 && item.max_file_size <= 0) {
		return t("policy_group_range_min", {
			size: formatBytes(item.min_file_size),
		});
	}
	if (item.min_file_size <= 0 && item.max_file_size > 0) {
		return t("policy_group_range_max", {
			size: formatBytes(item.max_file_size),
		});
	}
	return t("policy_group_range_between", {
		min: formatBytes(item.min_file_size),
		max: formatBytes(item.max_file_size),
	});
}

interface PolicyGroupsTableProps {
	currentPage: number;
	deletingGroupId: number | null;
	groups: StoragePolicyGroup[];
	loading: boolean;
	nextPageDisabled: boolean;
	pageSize: number;
	pageSizeOptions: PageSizeOption[];
	prevPageDisabled: boolean;
	sortBy: AdminPolicyGroupSortBy;
	sortOrder: SortOrder;
	total: number;
	totalPages: number;
	onNextPage: () => void;
	onOpenEdit: (group: StoragePolicyGroup) => void;
	onOpenMigration: (group: StoragePolicyGroup) => void;
	onPageSizeChange: (value: string | null) => void;
	onPreviousPage: () => void;
	onRequestDelete: (groupId: number) => void;
	onSortChange: (sortBy: AdminPolicyGroupSortBy, sortOrder: SortOrder) => void;
}

interface PolicyGroupsTableHeaderProps {
	onSortChange: (sortBy: AdminPolicyGroupSortBy, sortOrder: SortOrder) => void;
	sortBy: AdminPolicyGroupSortBy;
	sortOrder: SortOrder;
	t: AdminT;
}

function PolicyGroupsTableHeader({
	onSortChange,
	sortBy,
	sortOrder,
	t,
}: PolicyGroupsTableHeaderProps) {
	return (
		<TableHeader>
			<TableRow>
				<AdminSortableTableHead
					className="w-16"
					sortKey="id"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={onSortChange}
				>
					{t("id")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					sortKey="name"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={onSortChange}
				>
					{t("core:name")}
				</AdminSortableTableHead>
				<TableHead>{t("policy_group_rules")}</TableHead>
				<AdminSortableTableHead
					sortKey="is_enabled"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={onSortChange}
				>
					{t("policy_group_status")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					sortKey="updated_at"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={onSortChange}
				>
					{t("core:updated_at")}
				</AdminSortableTableHead>
				<TableHead className={ADMIN_TABLE_ACTIONS_WIDTH_CLASS}>
					{t("core:actions")}
				</TableHead>
			</TableRow>
		</TableHeader>
	);
}

interface PolicyGroupRulesCellProps {
	group: StoragePolicyGroup;
	t: AdminT;
}

function PolicyGroupRulesCell({ group, t }: PolicyGroupRulesCellProps) {
	return (
		<div className="flex min-w-0 flex-col gap-1.5 text-left">
			{group.items.slice(0, 2).map((item) => (
				<div
					key={item.id}
					className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground"
				>
					<Badge variant="outline">{item.policy.name}</Badge>
					<span>
						{t("policy_group_priority_short", {
							priority: item.priority,
						})}
					</span>
					<span>{getRuleRangeLabel(t, item)}</span>
				</div>
			))}
			{group.items.length > 2 ? (
				<span className="text-xs text-muted-foreground">
					{t("policy_group_more_rules", {
						count: group.items.length - 2,
					})}
				</span>
			) : null}
		</div>
	);
}

interface PolicyGroupStatusCellProps {
	group: StoragePolicyGroup;
	t: AdminT;
}

function PolicyGroupStatusCell({ group, t }: PolicyGroupStatusCellProps) {
	return (
		<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
			{group.is_default ? (
				<Badge className="border-blue-300 bg-blue-100 text-blue-700 dark:border-blue-700 dark:bg-blue-900 dark:text-blue-300">
					{t("is_default")}
				</Badge>
			) : null}
			<Badge
				variant="outline"
				className={
					group.is_enabled
						? "border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300"
						: "border-muted-foreground/30 bg-muted text-muted-foreground"
				}
			>
				{group.is_enabled ? t("core:active") : t("core:disabled_status")}
			</Badge>
		</div>
	);
}

interface PolicyGroupActionsProps {
	deleteLabel: string;
	group: StoragePolicyGroup;
	isDeleting: boolean;
	onOpenEdit: (group: StoragePolicyGroup) => void;
	onOpenMigration: (group: StoragePolicyGroup) => void;
	onRequestDelete: (groupId: number) => void;
	t: AdminT;
	total: number;
}

function PolicyGroupActions({
	deleteLabel,
	group,
	isDeleting,
	onOpenEdit,
	onOpenMigration,
	onRequestDelete,
	t,
	total,
}: PolicyGroupActionsProps) {
	return (
		<TooltipProvider>
			<div className="flex justify-end gap-1">
				<Tooltip>
					<TooltipTrigger
						render={<span className="inline-flex size-8 shrink-0" />}
					>
						<Button
							variant="ghost"
							size="icon"
							className={ADMIN_ICON_BUTTON_CLASS}
							onClick={() => onOpenMigration(group)}
							aria-label={t("migrate_policy_group_assignments")}
							title={t("migrate_policy_group_assignments")}
							disabled={total <= 1 || isDeleting}
						>
							<Icon name="ArrowsClockwise" className="size-3.5" />
						</Button>
					</TooltipTrigger>
					{total <= 1 ? (
						<TooltipContent>
							{t("policy_group_migration_unavailable")}
						</TooltipContent>
					) : null}
				</Tooltip>
				<Button
					variant="ghost"
					size="icon"
					className={ADMIN_ICON_BUTTON_CLASS}
					onClick={() => onOpenEdit(group)}
					aria-label={t("edit_policy_group")}
					title={t("edit_policy_group")}
					disabled={isDeleting}
				>
					<Icon name="PencilSimple" className="size-3.5" />
				</Button>
				<Tooltip>
					<TooltipTrigger
						render={<span className="inline-flex size-8 shrink-0" />}
					>
						<Button
							variant="ghost"
							size="icon"
							className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
							onClick={() => onRequestDelete(group.id)}
							aria-label={deleteLabel}
							title={deleteLabel}
							disabled={group.is_default || isDeleting}
						>
							<Icon
								name={isDeleting ? "Spinner" : "Trash"}
								className={`size-3.5 ${isDeleting ? "animate-spin" : ""}`}
							/>
						</Button>
					</TooltipTrigger>
					{group.is_default ? (
						<TooltipContent>
							{t("policy_group_delete_default_blocked")}
						</TooltipContent>
					) : null}
				</Tooltip>
			</div>
		</TooltipProvider>
	);
}

interface PolicyGroupRowProps {
	deletingGroupId: number | null;
	group: StoragePolicyGroup;
	onOpenEdit: (group: StoragePolicyGroup) => void;
	onOpenMigration: (group: StoragePolicyGroup) => void;
	onRequestDelete: (groupId: number) => void;
	t: AdminT;
	total: number;
}

function PolicyGroupRow({
	deletingGroupId,
	group,
	onOpenEdit,
	onOpenMigration,
	onRequestDelete,
	t,
	total,
}: PolicyGroupRowProps) {
	const isDeleting = deletingGroupId === group.id;
	const deleteLabel = isDeleting
		? t("policy_group_deleting")
		: t("delete_policy_group");

	return (
		<TableRow
			key={group.id}
			className={ADMIN_INTERACTIVE_TABLE_ROW_CLASS}
			onClick={() => {
				if (!isDeleting) onOpenEdit(group);
			}}
			onKeyDown={(event) => {
				if (event.key === "Enter" || event.key === " ") {
					event.preventDefault();
					if (!isDeleting) onOpenEdit(group);
				}
			}}
			tabIndex={0}
		>
			<TableCell>
				<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
					<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>{group.id}</span>
				</div>
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_STACKED_CELL_CLASS}>
					<div className="truncate font-medium text-foreground">
						{group.name}
					</div>
					{group.description ? (
						<p className="line-clamp-2 text-xs text-muted-foreground">
							{group.description}
						</p>
					) : (
						<span className="text-xs text-muted-foreground">
							{t("policy_group_description_empty")}
						</span>
					)}
				</div>
			</TableCell>
			<TableCell>
				<PolicyGroupRulesCell group={group} t={t} />
			</TableCell>
			<TableCell>
				<PolicyGroupStatusCell group={group} t={t} />
			</TableCell>
			<TableCell>
				<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
					<span className="text-xs text-muted-foreground">
						{formatDateAbsolute(group.updated_at)}
					</span>
				</div>
			</TableCell>
			<TableCell
				onClick={(event) => event.stopPropagation()}
				onKeyDown={(event) => event.stopPropagation()}
			>
				<PolicyGroupActions
					deleteLabel={deleteLabel}
					group={group}
					isDeleting={isDeleting}
					onOpenEdit={onOpenEdit}
					onOpenMigration={onOpenMigration}
					onRequestDelete={onRequestDelete}
					t={t}
					total={total}
				/>
			</TableCell>
		</TableRow>
	);
}

interface PolicyGroupsPaginationProps {
	currentPage: number;
	nextPageDisabled: boolean;
	onNextPage: () => void;
	onPageSizeChange: (value: string | null) => void;
	onPreviousPage: () => void;
	pageSize: number;
	pageSizeOptions: PageSizeOption[];
	prevPageDisabled: boolean;
	t: AdminT;
	total: number;
	totalPages: number;
}

function PolicyGroupsPagination({
	currentPage,
	nextPageDisabled,
	onNextPage,
	onPageSizeChange,
	onPreviousPage,
	pageSize,
	pageSizeOptions,
	prevPageDisabled,
	t,
	total,
	totalPages,
}: PolicyGroupsPaginationProps) {
	if (total <= 0) {
		return null;
	}

	return (
		<div className="flex items-center justify-between gap-3 px-4 pb-4 text-sm text-muted-foreground md:px-6">
			<div className="flex items-center gap-3">
				<span>
					{t("entries_page", {
						total,
						current: currentPage,
						pages: totalPages,
					})}
				</span>
				<Select
					items={pageSizeOptions}
					value={String(pageSize)}
					onValueChange={onPageSizeChange}
				>
					<SelectTrigger width="page-size">
						<SelectValue />
					</SelectTrigger>
					<SelectContent>
						{pageSizeOptions.map((option) => (
							<SelectItem key={option.value} value={option.value}>
								{option.label}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			</div>
			<TooltipProvider>
				<div className="flex items-center gap-2">
					<Tooltip>
						<TooltipTrigger
							render={
								<Button
									variant="outline"
									size="sm"
									disabled={prevPageDisabled}
									onClick={onPreviousPage}
								/>
							}
						>
							<Icon name="CaretLeft" className="size-4" />
						</TooltipTrigger>
						{prevPageDisabled ? (
							<TooltipContent>{t("pagination_prev_disabled")}</TooltipContent>
						) : null}
					</Tooltip>
					<Tooltip>
						<TooltipTrigger
							render={
								<Button
									variant="outline"
									size="sm"
									disabled={nextPageDisabled}
									onClick={onNextPage}
								/>
							}
						>
							<Icon name="CaretRight" className="size-4" />
						</TooltipTrigger>
						{nextPageDisabled ? (
							<TooltipContent>{t("pagination_next_disabled")}</TooltipContent>
						) : null}
					</Tooltip>
				</div>
			</TooltipProvider>
		</div>
	);
}

export function PolicyGroupsTable({
	currentPage,
	deletingGroupId,
	groups,
	loading,
	nextPageDisabled,
	pageSize,
	pageSizeOptions,
	prevPageDisabled,
	sortBy,
	sortOrder,
	total,
	totalPages,
	onNextPage,
	onOpenEdit,
	onOpenMigration,
	onPageSizeChange,
	onPreviousPage,
	onRequestDelete,
	onSortChange,
}: PolicyGroupsTableProps) {
	const { t } = useTranslation("admin");
	const emptyIcon = useMemo(
		() => <Icon name="ListBullets" className="size-6" />,
		[],
	);
	const headerRow = useMemo(
		() => (
			<PolicyGroupsTableHeader
				onSortChange={onSortChange}
				sortBy={sortBy}
				sortOrder={sortOrder}
				t={t}
			/>
		),
		[onSortChange, sortBy, sortOrder, t],
	);

	return (
		<>
			<AdminTableList
				loading={loading}
				items={groups}
				columns={6}
				rows={5}
				emptyIcon={emptyIcon}
				emptyTitle={t("no_policy_groups")}
				emptyDescription={t("no_policy_groups_desc")}
				headerRow={headerRow}
				renderRow={(group) => (
					<PolicyGroupRow
						key={group.id}
						deletingGroupId={deletingGroupId}
						group={group}
						onOpenEdit={onOpenEdit}
						onOpenMigration={onOpenMigration}
						onRequestDelete={onRequestDelete}
						t={t}
						total={total}
					/>
				)}
			/>

			<PolicyGroupsPagination
				currentPage={currentPage}
				nextPageDisabled={nextPageDisabled}
				onNextPage={onNextPage}
				onPageSizeChange={onPageSizeChange}
				onPreviousPage={onPreviousPage}
				pageSize={pageSize}
				pageSizeOptions={pageSizeOptions}
				prevPageDisabled={prevPageDisabled}
				t={t}
				total={total}
				totalPages={totalPages}
			/>
		</>
	);
}
