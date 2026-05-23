import type { SetStateAction } from "react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import {
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminSortableTableHead,
	AdminTableShell,
	AdminTable as Table,
	AdminTableBody as TableBody,
	AdminTableCell as TableCell,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
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
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { useApiList } from "@/hooks/useApiList";
import { usePageTitle } from "@/hooks/usePageTitle";
import {
	AUDIT_ENTITY_TYPE_FILTER_VALUES,
	formatAuditAction,
	formatAuditEntityType,
	getAuditActionBadgeClass,
	isAuditEntityType,
} from "@/lib/audit";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import {
	buildOffsetPaginationSearchParams,
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
	parseSortOrderSearchParam,
	parseSortSearchParam,
	type SortOrder,
} from "@/lib/pagination";
import { auditService } from "@/services/auditService";
import type { AdminAuditLogSortBy } from "@/types/adminSort";
import type { AuditEntityType } from "@/types/api";

const AUDIT_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_AUDIT_PAGE_SIZE = 20 as const;
const AUDIT_MANAGED_QUERY_KEYS = [
	"action",
	"entityType",
	"offset",
	"pageSize",
	"sortBy",
	"sortOrder",
] as const;
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

function normalizeOffset(offset: number) {
	return Math.max(0, Math.floor(offset));
}

function parseEntityTypeSearchParam(
	value: string | null,
): AuditEntityTypeFilter {
	const normalized = value?.trim();
	return normalized && isAuditEntityType(normalized) ? normalized : "__all__";
}

function buildManagedAuditSearchParams({
	offset,
	pageSize,
	action,
	entityType,
	sortBy,
	sortOrder,
}: {
	offset: number;
	pageSize: (typeof AUDIT_PAGE_SIZE_OPTIONS)[number];
	action: string;
	entityType: AuditEntityTypeFilter;
	sortBy: AdminAuditLogSortBy;
	sortOrder: SortOrder;
}) {
	return buildOffsetPaginationSearchParams({
		offset,
		pageSize,
		defaultPageSize: DEFAULT_AUDIT_PAGE_SIZE,
		extraParams: {
			action: action.trim() || undefined,
			entityType: entityType !== "__all__" ? entityType : undefined,
			sortBy: sortBy !== DEFAULT_AUDIT_SORT_BY ? sortBy : undefined,
			sortOrder: sortOrder !== DEFAULT_AUDIT_SORT_ORDER ? sortOrder : undefined,
		},
	});
}

function getManagedAuditSearchString(searchParams: URLSearchParams) {
	return buildManagedAuditSearchParams({
		offset: normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
		pageSize: parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			AUDIT_PAGE_SIZE_OPTIONS,
			DEFAULT_AUDIT_PAGE_SIZE,
		),
		action: searchParams.get("action") ?? "",
		entityType: parseEntityTypeSearchParam(searchParams.get("entityType")),
		sortBy: parseSortSearchParam(
			searchParams.get("sortBy"),
			AUDIT_SORT_BY_OPTIONS,
			DEFAULT_AUDIT_SORT_BY,
		),
		sortOrder: parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_AUDIT_SORT_ORDER,
		),
	}).toString();
}

function mergeManagedAuditSearchParams(
	searchParams: URLSearchParams,
	managedSearchParams: URLSearchParams,
) {
	const merged = new URLSearchParams(searchParams);
	for (const key of AUDIT_MANAGED_QUERY_KEYS) {
		merged.delete(key);
	}
	for (const [key, value] of managedSearchParams.entries()) {
		merged.set(key, value);
	}
	return merged;
}

export default function AdminAuditPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("audit_log"));
	const [searchParams, setSearchParams] = useSearchParams();
	const initialAction = searchParams.get("action") ?? "";
	const [offset, setOffsetState] = useState(
		normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
	);
	const [pageSize, setPageSize] = useState<
		(typeof AUDIT_PAGE_SIZE_OPTIONS)[number]
	>(
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			AUDIT_PAGE_SIZE_OPTIONS,
			DEFAULT_AUDIT_PAGE_SIZE,
		),
	);
	const [actionFilter, setActionFilter] = useState(initialAction);
	const [entityTypeFilter, setEntityTypeFilter] =
		useState<AuditEntityTypeFilter>(
			parseEntityTypeSearchParam(searchParams.get("entityType")),
		);
	const [sortBy, setSortBy] = useState<AdminAuditLogSortBy>(
		parseSortSearchParam(
			searchParams.get("sortBy"),
			AUDIT_SORT_BY_OPTIONS,
			DEFAULT_AUDIT_SORT_BY,
		),
	);
	const [sortOrder, setSortOrder] = useState<SortOrder>(
		parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_AUDIT_SORT_ORDER,
		),
	);
	const lastWrittenSearchRef = useRef<string | null>(null);
	const setOffset = (value: SetStateAction<number>) => {
		setOffsetState((current) =>
			normalizeOffset(typeof value === "function" ? value(current) : value),
		);
	};

	useEffect(() => {
		const managedSearch = getManagedAuditSearchString(searchParams);
		if (managedSearch === lastWrittenSearchRef.current) {
			return;
		}

		const nextOffset = normalizeOffset(
			parseOffsetSearchParam(searchParams.get("offset")),
		);
		const nextPageSize = parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			AUDIT_PAGE_SIZE_OPTIONS,
			DEFAULT_AUDIT_PAGE_SIZE,
		);
		const nextAction = searchParams.get("action") ?? "";
		const nextEntityType = parseEntityTypeSearchParam(
			searchParams.get("entityType"),
		);
		const nextSortBy = parseSortSearchParam(
			searchParams.get("sortBy"),
			AUDIT_SORT_BY_OPTIONS,
			DEFAULT_AUDIT_SORT_BY,
		);
		const nextSortOrder = parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_AUDIT_SORT_ORDER,
		);

		setOffsetState((prev) => (prev === nextOffset ? prev : nextOffset));
		setPageSize((prev) => (prev === nextPageSize ? prev : nextPageSize));
		setActionFilter((prev) => (prev === nextAction ? prev : nextAction));
		setEntityTypeFilter((prev) =>
			prev === nextEntityType ? prev : nextEntityType,
		);
		setSortBy((prev) => (prev === nextSortBy ? prev : nextSortBy));
		setSortOrder((prev) => (prev === nextSortOrder ? prev : nextSortOrder));
	}, [searchParams]);

	useEffect(() => {
		const nextManagedSearchParams = buildManagedAuditSearchParams({
			offset,
			pageSize,
			action: actionFilter,
			entityType: entityTypeFilter,
			sortBy,
			sortOrder,
		});
		const nextSearch = nextManagedSearchParams.toString();
		const currentSearch = getManagedAuditSearchString(searchParams);
		if (
			currentSearch !== lastWrittenSearchRef.current &&
			currentSearch !== nextSearch
		) {
			return;
		}

		lastWrittenSearchRef.current = nextSearch;
		if (nextSearch === currentSearch) {
			return;
		}

		setSearchParams(
			mergeManagedAuditSearchParams(searchParams, nextManagedSearchParams),
			{ replace: true },
		);
	}, [
		actionFilter,
		entityTypeFilter,
		offset,
		pageSize,
		searchParams,
		setSearchParams,
		sortBy,
		sortOrder,
	]);

	const { items, loading, reload, total } = useApiList(
		() =>
			auditService.list({
				action: actionFilter.trim() || undefined,
				entity_type:
					entityTypeFilter === "__all__" ? undefined : entityTypeFilter,
				limit: pageSize,
				offset,
				sort_by: sortBy,
				sort_order: sortOrder,
			}),
		[actionFilter, entityTypeFilter, offset, pageSize, sortBy, sortOrder],
	);

	const activeFilterCount =
		(actionFilter.trim().length > 0 ? 1 : 0) +
		(entityTypeFilter !== "__all__" ? 1 : 0);
	const hasServerFilters = activeFilterCount > 0;
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;
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
		setActionFilter("");
		setEntityTypeFilter("__all__");
		setOffset(0);
	};

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, AUDIT_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
	};

	const handleActionFilterChange = (value: string) => {
		setActionFilter(value);
		setOffset(0);
	};

	const handleEntityTypeFilterChange = (value: string | null) => {
		if (!value) return;
		setEntityTypeFilter(isAuditEntityType(value) ? value : "__all__");
		setOffset(0);
	};

	const handleSortChange = (
		nextSortBy: AdminAuditLogSortBy,
		nextOrder: SortOrder,
	) => {
		setSortBy(nextSortBy);
		setSortOrder(nextOrder);
		setOffset(0);
	};

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
						<>
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
							<div className="ml-auto flex items-center gap-2 text-xs text-muted-foreground">
								{hasServerFilters ? <span>{t("filters_active")}</span> : null}
								{activeFilterCount > 0 ? (
									<Button
										variant="ghost"
										size="sm"
										className={ADMIN_CONTROL_HEIGHT_CLASS}
										onClick={resetFilters}
									>
										{t("clear_filters")}
									</Button>
								) : null}
							</div>
						</>
					}
				/>

				{loading ? (
					<SkeletonTable columns={6} rows={6} />
				) : items.length === 0 ? (
					hasServerFilters ? (
						<EmptyState
							icon={<Icon name="Scroll" className="size-10" />}
							title={t("no_filtered_audit_logs")}
							description={t("no_filtered_audit_logs_desc")}
							action={
								<Button variant="outline" onClick={resetFilters}>
									{t("clear_filters")}
								</Button>
							}
						/>
					) : (
						<EmptyState
							icon={<Icon name="Scroll" className="size-10" />}
							title={t("no_audit_logs")}
						/>
					)
				) : (
					<AdminTableShell>
						<Table>
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
							<TableBody>
								{items.map((item) => (
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
													{formatAuditAction(t, item.action)}
												</Badge>
											</div>
										</TableCell>
										<TableCell>
											<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
												<span className="text-sm text-muted-foreground">
													{formatAuditEntityType(t, item.entity_type)}
												</span>
											</div>
										</TableCell>
										<TableCell>
											<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
												<span className="truncate text-sm text-muted-foreground">
													{item.entity_name ?? "---"}
												</span>
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
								))}
							</TableBody>
						</Table>
					</AdminTableShell>
				)}

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
								onValueChange={handlePageSizeChange}
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
												onClick={() =>
													setOffset((current) =>
														Math.max(0, current - pageSize),
													)
												}
											/>
										}
									>
										<Icon name="CaretLeft" className="size-4" />
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
												onClick={() =>
													setOffset((current) => current + pageSize)
												}
											/>
										}
									>
										<Icon name="CaretRight" className="size-4" />
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
			</AdminPageShell>
		</AdminLayout>
	);
}
