import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import {
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminTableShell,
	AdminTable as Table,
	AdminTableBody as TableBody,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
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
} from "@/lib/audit";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import {
	buildOffsetPaginationSearchParams,
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
} from "@/lib/pagination";
import { auditService } from "@/services/auditService";

const AUDIT_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_AUDIT_PAGE_SIZE = 20 as const;
const AUDIT_MANAGED_QUERY_KEYS = [
	"action",
	"entityType",
	"offset",
	"pageSize",
] as const;

type AuditEntityTypeFilter = "__all__" | string;

function normalizeOffset(offset: number) {
	return Math.max(0, Math.floor(offset));
}

function parseEntityTypeSearchParam(
	value: string | null,
): AuditEntityTypeFilter {
	const normalized = value?.trim();
	return normalized ? normalized : "__all__";
}

function buildManagedAuditSearchParams({
	offset,
	pageSize,
	action,
	entityType,
}: {
	offset: number;
	pageSize: (typeof AUDIT_PAGE_SIZE_OPTIONS)[number];
	action: string;
	entityType: AuditEntityTypeFilter;
}) {
	return buildOffsetPaginationSearchParams({
		offset,
		pageSize,
		defaultPageSize: DEFAULT_AUDIT_PAGE_SIZE,
		extraParams: {
			action: action.trim() || undefined,
			entityType: entityType !== "__all__" ? entityType : undefined,
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
	const lastWrittenSearchRef = useRef<string | null>(null);
	const setOffset = (value: number) => {
		setOffsetState(normalizeOffset(value));
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

		setOffsetState((prev) => (prev === nextOffset ? prev : nextOffset));
		setPageSize((prev) => (prev === nextPageSize ? prev : nextPageSize));
		setActionFilter((prev) => (prev === nextAction ? prev : nextAction));
		setEntityTypeFilter((prev) =>
			prev === nextEntityType ? prev : nextEntityType,
		);
	}, [searchParams]);

	useEffect(() => {
		const nextManagedSearchParams = buildManagedAuditSearchParams({
			offset,
			pageSize,
			action: actionFilter,
			entityType: entityTypeFilter,
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
	]);

	const { items, loading, reload, total } = useApiList(
		() =>
			auditService.list({
				action: actionFilter.trim() || undefined,
				entity_type:
					entityTypeFilter === "__all__" ? undefined : entityTypeFilter,
				limit: pageSize,
				offset,
			}),
		[actionFilter, entityTypeFilter, offset, pageSize],
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
		setEntityTypeFilter(value as AuditEntityTypeFilter);
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
								className={`mr-1 h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`}
							/>
							{t("core:refresh")}
						</Button>
					}
					toolbar={
						<>
							<div className="relative min-w-[240px] flex-1 md:max-w-sm">
								<Icon
									name="MagnifyingGlass"
									className="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground"
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
							icon={<Icon name="Scroll" className="h-10 w-10" />}
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
							icon={<Icon name="Scroll" className="h-10 w-10" />}
							title={t("no_audit_logs")}
						/>
					)
				) : (
					<AdminTableShell>
						<Table>
							<TableHeader>
								<TableRow>
									<TableHead className="w-[180px]">{t("audit_time")}</TableHead>
									<TableHead className="w-24">{t("audit_user")}</TableHead>
									<TableHead className="w-[180px]">
										{t("audit_action")}
									</TableHead>
									<TableHead className="w-32">{t("audit_entity")}</TableHead>
									<TableHead>{t("core:name")}</TableHead>
									<TableHead className="w-[160px]">{t("audit_ip")}</TableHead>
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
												<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>
													{item.user_id ?? "---"}
												</span>
											</div>
										</TableCell>
										<TableCell>
											<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
												<span className="inline-flex items-center rounded-full bg-blue-50 px-2 py-0.5 text-xs font-medium text-blue-700 dark:bg-blue-950 dark:text-blue-300">
													{formatAuditAction(t, item.action)}
												</span>
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
													setOffset(Math.max(0, offset - pageSize))
												}
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
												onClick={() => setOffset(offset + pageSize)}
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
			</AdminPageShell>
		</AdminLayout>
	);
}
