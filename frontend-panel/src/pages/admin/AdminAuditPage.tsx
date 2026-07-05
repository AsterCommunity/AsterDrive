import type { SetStateAction } from "react";
import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { AdminFilterToolbar } from "@/components/common/AdminFilterToolbar";
import {
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminSortableTableHead,
	AdminTableCell as TableCell,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { AdminTableList } from "@/components/common/AdminTableList";
import { UserIdentity } from "@/components/common/UserIdentity";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { useManagedAdminList } from "@/hooks/useManagedAdminList";
import {
	type ManagedListQuerySchema,
	managedOffsetQueryField,
	managedPageSizeQueryField,
	managedSortByQueryField,
	managedSortOrderQueryField,
	managedStringQueryField,
	useManagedListQueryState,
} from "@/hooks/useManagedListQueryState";
import { usePageTitle } from "@/hooks/usePageTitle";
import {
	AUDIT_ENTITY_TYPE_FILTER_VALUES,
	formatAuditDetail,
	formatAuditEntityType,
	formatAuditSummary,
	formatAuditTarget,
	formatAuditTargetType,
	getAuditActionBadgeClass,
	isAuditEntityType,
} from "@/lib/audit";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import { parsePageSizeOption, type SortOrder } from "@/lib/pagination";
import { auditService } from "@/services/auditService";
import type { AdminAuditLogSortBy } from "@/types/adminSort";
import type { AuditEntityType, AuditLogEntry } from "@/types/api";

const AUDIT_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_AUDIT_PAGE_SIZE = 20 as const;
const AUDIT_SORT_BY_OPTIONS = [
	"id",
	"created_at",
	"user_id",
	"action",
	"entity_type",
	"entity_name",
	"ip_address",
] as const satisfies readonly AdminAuditLogSortBy[];
const DEFAULT_AUDIT_SORT_BY =
	"created_at" as const satisfies AdminAuditLogSortBy;
const DEFAULT_AUDIT_SORT_ORDER = "desc" as const satisfies SortOrder;

type AuditEntityTypeFilter = "__all__" | AuditEntityType;
type ManagedAuditQuery = {
	action: string;
	entityType: AuditEntityTypeFilter;
	offset: number;
	pageSize: (typeof AUDIT_PAGE_SIZE_OPTIONS)[number];
	sortBy: AdminAuditLogSortBy;
	sortOrder: SortOrder;
};

function parseEntityTypeSearchParam(
	value: string | null,
): AuditEntityTypeFilter {
	const normalized = value?.trim();
	return normalized && isAuditEntityType(normalized) ? normalized : "__all__";
}

const MANAGED_AUDIT_QUERY_DEFAULTS = {
	action: "",
	entityType: "__all__",
	offset: 0,
	pageSize: DEFAULT_AUDIT_PAGE_SIZE,
	sortBy: DEFAULT_AUDIT_SORT_BY,
	sortOrder: DEFAULT_AUDIT_SORT_ORDER,
} satisfies ManagedAuditQuery;

const MANAGED_AUDIT_QUERY_SCHEMA = {
	action: managedStringQueryField({ key: "action" }),
	entityType: {
		keys: ["entityType"],
		parse: (searchParams) =>
			parseEntityTypeSearchParam(searchParams.get("entityType")),
		serialize: (value) => (value !== "__all__" ? value : undefined),
	},
	offset: managedOffsetQueryField(),
	pageSize: managedPageSizeQueryField(
		AUDIT_PAGE_SIZE_OPTIONS,
		DEFAULT_AUDIT_PAGE_SIZE,
	),
	sortBy: managedSortByQueryField(AUDIT_SORT_BY_OPTIONS, DEFAULT_AUDIT_SORT_BY),
	sortOrder: managedSortOrderQueryField(DEFAULT_AUDIT_SORT_ORDER),
} satisfies ManagedListQuerySchema<ManagedAuditQuery>;

export default function AdminAuditPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("audit_log"));
	const [searchParams, setSearchParams] = useSearchParams();
	const { query, setQuery } = useManagedListQueryState({
		defaults: MANAGED_AUDIT_QUERY_DEFAULTS,
		schema: MANAGED_AUDIT_QUERY_SCHEMA,
		searchParams,
		setSearchParams,
	});
	const {
		action: actionFilter,
		entityType: entityTypeFilter,
		offset,
		pageSize,
		sortBy,
		sortOrder,
	} = query;
	const setOffset = useCallback(
		(value: SetStateAction<number>) => {
			setQuery((current) => ({
				offset: Math.max(
					0,
					typeof value === "function" ? value(current.offset) : value,
				),
			}));
		},
		[setQuery],
	);

	const {
		currentPage,
		items,
		loading,
		nextPageDisabled,
		prevPageDisabled,
		reload,
		total,
		totalPages,
	} = useManagedAdminList<AuditLogEntry, ManagedAuditQuery>({
		deps: [actionFilter, entityTypeFilter, offset, pageSize, sortBy, sortOrder],
		loadPage: (query) =>
			auditService.list({
				action: query.action.trim() || undefined,
				entity_type:
					query.entityType === "__all__" ? undefined : query.entityType,
				limit: query.pageSize,
				offset: query.offset,
				sort_by: query.sortBy,
				sort_order: query.sortOrder,
			}),
		query,
		setOffset,
	});

	const activeFilterCount =
		(actionFilter.trim().length > 0 ? 1 : 0) +
		(entityTypeFilter !== "__all__" ? 1 : 0);
	const hasServerFilters = activeFilterCount > 0;
	const entityTypeOptions = [
		{ label: t("audit_all_types"), value: "__all__" },
		...AUDIT_ENTITY_TYPE_FILTER_VALUES.map((value) => ({
			label: formatAuditEntityType(t, value),
			value,
		})),
	] satisfies ReadonlyArray<{ label: string; value: AuditEntityTypeFilter }>;
	const pageSizeOptions = AUDIT_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));

	const resetFilters = () => {
		setQuery({ action: "", entityType: "__all__", offset: 0 });
	};

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, AUDIT_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setQuery({ offset: 0, pageSize: next });
	};

	const handleActionFilterChange = (value: string) => {
		setQuery({ action: value, offset: 0 });
	};

	const handleEntityTypeFilterChange = (value: string | null) => {
		if (!value) return;
		setQuery({
			entityType: isAuditEntityType(value) ? value : "__all__",
			offset: 0,
		});
	};

	const handleSortChange = (
		nextSortBy: AdminAuditLogSortBy,
		nextOrder: SortOrder,
	) => {
		setQuery({ offset: 0, sortBy: nextSortBy, sortOrder: nextOrder });
	};
	const auditEmptyIcon = <Icon name="Scroll" className="size-10" />;
	const filteredEmptyAction = (
		<Button variant="outline" onClick={resetFilters}>
			{t("clear_filters")}
		</Button>
	);
	const auditTableHeader = (
		<TableHeader>
			<TableRow>
				<AdminSortableTableHead
					className="w-[180px]"
					sortKey="created_at"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("audit_time")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					className="w-[180px]"
					sortKey="user_id"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("audit_user")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					className="w-[180px]"
					sortKey="action"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("audit_action")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					className="w-32"
					sortKey="entity_type"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("audit_entity")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					sortKey="entity_name"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("core:name")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					className="w-[160px]"
					sortKey="ip_address"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("audit_ip")}
				</AdminSortableTableHead>
			</TableRow>
		</TableHeader>
	);
	const auditPagination = (
		<AdminOffsetPagination
			total={total}
			currentPage={currentPage}
			totalPages={totalPages}
			pageSize={String(pageSize)}
			pageSizeOptions={pageSizeOptions}
			onPageSizeChange={handlePageSizeChange}
			prevDisabled={prevPageDisabled}
			nextDisabled={nextPageDisabled}
			onPrevious={() => setOffset((current) => Math.max(0, current - pageSize))}
			onNext={() => setOffset((current) => current + pageSize)}
		/>
	);

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					title={t("audit_log")}
					description={t("audit_intro")}
					actions={
						<Button
							variant="outline"
							size="sm"
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							onClick={() => void reload()}
							disabled={loading}
						>
							<Icon
								name={loading ? "Spinner" : "ArrowsClockwise"}
								className={`mr-1 size-3.5 ${loading ? "animate-spin" : ""}`}
							/>
							{t("core:refresh")}
						</Button>
					}
					toolbar={
						<AdminFilterToolbar
							activeFilterCount={activeFilterCount}
							inline
							onResetFilters={resetFilters}
						>
							<div className="relative min-w-[240px] flex-1 md:max-w-sm">
								<Icon
									name="MagnifyingGlass"
									className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
								/>
								<Input
									placeholder={t("audit_filter_action")}
									value={actionFilter}
									onChange={(event) =>
										handleActionFilterChange(event.target.value)
									}
									className={`${ADMIN_CONTROL_HEIGHT_CLASS} pl-9`}
								/>
							</div>
							<Select
								items={entityTypeOptions}
								value={entityTypeFilter}
								onValueChange={handleEntityTypeFilterChange}
							>
								<SelectTrigger width="compact">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									{entityTypeOptions.map((option) => (
										<SelectItem key={option.value} value={option.value}>
											{option.label}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</AdminFilterToolbar>
					}
				/>

				<AdminTableList
					loading={loading}
					items={items}
					columns={6}
					rows={6}
					emptyIcon={auditEmptyIcon}
					emptyTitle={t("no_audit_logs")}
					filtered={hasServerFilters}
					filteredEmptyTitle={t("no_filtered_audit_logs")}
					filteredEmptyDescription={t("no_filtered_audit_logs_desc")}
					filteredEmptyAction={filteredEmptyAction}
					headerRow={auditTableHeader}
					pagination={auditPagination}
					renderRow={(item) => {
						const detail = formatAuditDetail(t, item);

						return (
							<TableRow key={item.id}>
								<TableCell>
									<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
										<span
											className="text-xs text-muted-foreground whitespace-nowrap"
											title={formatDateAbsoluteWithOffset(item.created_at)}
										>
											{formatDateAbsolute(item.created_at)}
										</span>
									</div>
								</TableCell>
								<TableCell>
									<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
										<UserIdentity user={item.user} />
									</div>
								</TableCell>
								<TableCell>
									<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
										<Badge
											variant="outline"
											className={getAuditActionBadgeClass(item.action)}
										>
											{formatAuditSummary(t, item)}
										</Badge>
									</div>
								</TableCell>
								<TableCell>
									<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
										<span className="text-sm text-muted-foreground">
											{formatAuditTargetType(t, item)}
										</span>
									</div>
								</TableCell>
								<TableCell className="max-w-0">
									<div className="flex min-w-0 flex-col gap-0.5 text-left">
										<span className="truncate text-sm text-muted-foreground">
											{formatAuditTarget(t, item)}
										</span>
										{detail ? (
											<span className="truncate text-xs text-muted-foreground/80">
												{detail}
											</span>
										) : null}
									</div>
								</TableCell>
								<TableCell>
									<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
										<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>
											{item.ip_address ?? "---"}
										</span>
									</div>
								</TableCell>
							</TableRow>
						);
					}}
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}
