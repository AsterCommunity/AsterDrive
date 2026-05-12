import { useTranslation } from "react-i18next";
import {
	ADMIN_INTERACTIVE_TABLE_ROW_CLASS,
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_STACKED_CELL_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
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
import type { StoragePolicyGroup, StoragePolicyGroupItem } from "@/types/api";

function getRuleRangeLabel(
	t: ReturnType<typeof useTranslation>["t"],
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
	groups: StoragePolicyGroup[];
	loading: boolean;
	nextPageDisabled: boolean;
	pageSize: number;
	pageSizeOptions: Array<{
		label: string;
		value: string;
	}>;
	prevPageDisabled: boolean;
	total: number;
	totalPages: number;
	onNextPage: () => void;
	onOpenEdit: (group: StoragePolicyGroup) => void;
	onOpenMigration: (group: StoragePolicyGroup) => void;
	onPageSizeChange: (value: string | null) => void;
	onPreviousPage: () => void;
	onRequestDelete: (groupId: number) => void;
}

export function PolicyGroupsTable({
	currentPage,
	groups,
	loading,
	nextPageDisabled,
	pageSize,
	pageSizeOptions,
	prevPageDisabled,
	total,
	totalPages,
	onNextPage,
	onOpenEdit,
	onOpenMigration,
	onPageSizeChange,
	onPreviousPage,
	onRequestDelete,
}: PolicyGroupsTableProps) {
	const { t } = useTranslation("admin");

	return (
		<>
			<AdminTableList
				loading={loading}
				items={groups}
				columns={6}
				rows={5}
				emptyIcon={<Icon name="ListBullets" className="h-6 w-6" />}
				emptyTitle={t("no_policy_groups")}
				emptyDescription={t("no_policy_groups_desc")}
				headerRow={
					<TableHeader>
						<TableRow>
							<TableHead className="w-16">{t("id")}</TableHead>
							<TableHead>{t("core:name")}</TableHead>
							<TableHead>{t("policy_group_rules")}</TableHead>
							<TableHead>{t("policy_group_status")}</TableHead>
							<TableHead>{t("core:updated_at")}</TableHead>
							<TableHead className={ADMIN_TABLE_ACTIONS_WIDTH_CLASS}>
								{t("core:actions")}
							</TableHead>
						</TableRow>
					</TableHeader>
				}
				renderRow={(group) => (
					<TableRow
						key={group.id}
						className={ADMIN_INTERACTIVE_TABLE_ROW_CLASS}
						onClick={() => onOpenEdit(group)}
						onKeyDown={(event) => {
							if (event.key === "Enter" || event.key === " ") {
								event.preventDefault();
								onOpenEdit(group);
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
						</TableCell>
						<TableCell>
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
									{group.is_enabled
										? t("core:active")
										: t("core:disabled_status")}
								</Badge>
							</div>
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
							<TooltipProvider>
								<div className="flex justify-end gap-1">
									<Tooltip>
										<TooltipTrigger>
											<div>
												<Button
													variant="ghost"
													size="icon"
													className={ADMIN_ICON_BUTTON_CLASS}
													onClick={() => onOpenMigration(group)}
													aria-label={t("migrate_policy_group_users")}
													title={t("migrate_policy_group_users")}
													disabled={total <= 1}
												>
													<Icon
														name="ArrowsClockwise"
														className="h-3.5 w-3.5"
													/>
												</Button>
											</div>
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
									>
										<Icon name="PencilSimple" className="h-3.5 w-3.5" />
									</Button>
									<Tooltip>
										<TooltipTrigger>
											<div>
												<Button
													variant="ghost"
													size="icon"
													className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
													onClick={() => onRequestDelete(group.id)}
													aria-label={t("delete_policy_group")}
													title={t("delete_policy_group")}
													disabled={group.is_default}
												>
													<Icon name="Trash" className="h-3.5 w-3.5" />
												</Button>
											</div>
										</TooltipTrigger>
										{group.is_default ? (
											<TooltipContent>
												{t("policy_group_delete_default_blocked")}
											</TooltipContent>
										) : null}
									</Tooltip>
								</div>
							</TooltipProvider>
						</TableCell>
					</TableRow>
				)}
			/>

			{total > 0 ? (
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
									<Icon name="CaretLeft" className="h-4 w-4" />
								</TooltipTrigger>
								{prevPageDisabled ? (
									<TooltipContent>
										{t("pagination_prev_disabled")}
									</TooltipContent>
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
									<Icon name="CaretRight" className="h-4 w-4" />
								</TooltipTrigger>
								{nextPageDisabled ? (
									<TooltipContent>
										{t("pagination_next_disabled")}
									</TooltipContent>
								) : null}
							</Tooltip>
						</div>
					</TooltipProvider>
				</div>
			) : null}
		</>
	);
}
