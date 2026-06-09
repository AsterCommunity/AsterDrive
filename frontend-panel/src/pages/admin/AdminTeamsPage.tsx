import {
	type FormEvent,
	type SetStateAction,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import {
	AdminTeamsTableHeader,
	AdminTeamsTableRow,
} from "@/components/admin/admin-teams-page/AdminTeamsTable";
import { AdminTeamsToolbar } from "@/components/admin/admin-teams-page/AdminTeamsToolbar";
import {
	CreateTeamDialog,
	type CreateTeamFormState,
	type TeamPolicyGroupOption,
} from "@/components/admin/admin-teams-page/CreateTeamDialog";
import { AdminTableList } from "@/components/common/AdminTableList";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useApiList } from "@/hooks/useApiList";
import { usePageTitle } from "@/hooks/usePageTitle";
import {
	loadAdminPolicyGroupLookup,
	readAdminPolicyGroupLookup,
} from "@/lib/adminPolicyGroupLookup";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import {
	buildOffsetPaginationSearchParams,
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
	parseSortOrderSearchParam,
	parseSortSearchParam,
	type SortOrder,
} from "@/lib/pagination";
import { parseStorageQuotaMbToBytes } from "@/lib/storageQuota";
import { adminTeamService } from "@/services/adminService";
import type { AdminTeamSortBy } from "@/types/adminSort";
import type { AdminTeamInfo, StoragePolicyGroup } from "@/types/api";

const TEAM_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_TEAM_PAGE_SIZE = 20 as const;
const TEAM_MANAGED_QUERY_KEYS = [
	"archived",
	"keyword",
	"offset",
	"pageSize",
	"sortBy",
	"sortOrder",
] as const;
const TEAM_SORT_BY_OPTIONS = [
	"id",
	"name",
	"storage_used",
	"storage_quota",
	"created_at",
	"updated_at",
	"archived_at",
] as const satisfies readonly AdminTeamSortBy[];
const DEFAULT_TEAM_SORT_BY = "created_at" as const satisfies AdminTeamSortBy;
const DEFAULT_TEAM_SORT_ORDER = "desc" as const satisfies SortOrder;

const EMPTY_CREATE_FORM: CreateTeamFormState = {
	name: "",
	description: "",
	adminIdentifier: "",
	quotaValue: "",
	policyGroupId: "",
};

function normalizeOffset(offset: number) {
	return Math.max(0, Math.floor(offset));
}

function parseArchivedSearchParam(value: string | null) {
	return value === "1" || value === "true";
}

function createTeamQuotaBytes(value: string) {
	const normalized = value.trim();
	if (!normalized) return undefined;

	return parseStorageQuotaMbToBytes(normalized);
}

function buildManagedTeamSearchParams({
	offset,
	pageSize,
	keyword,
	archived,
	sortBy,
	sortOrder,
}: {
	offset: number;
	pageSize: (typeof TEAM_PAGE_SIZE_OPTIONS)[number];
	keyword: string;
	archived: boolean;
	sortBy: AdminTeamSortBy;
	sortOrder: SortOrder;
}) {
	return buildOffsetPaginationSearchParams({
		offset,
		pageSize,
		defaultPageSize: DEFAULT_TEAM_PAGE_SIZE,
		extraParams: {
			archived: archived ? true : undefined,
			keyword: keyword.trim() || undefined,
			sortBy: sortBy !== DEFAULT_TEAM_SORT_BY ? sortBy : undefined,
			sortOrder: sortOrder !== DEFAULT_TEAM_SORT_ORDER ? sortOrder : undefined,
		},
	});
}

function getManagedTeamSearchString(searchParams: URLSearchParams) {
	return buildManagedTeamSearchParams({
		offset: normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
		pageSize: parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			TEAM_PAGE_SIZE_OPTIONS,
			DEFAULT_TEAM_PAGE_SIZE,
		),
		keyword: searchParams.get("keyword") ?? "",
		archived: parseArchivedSearchParam(searchParams.get("archived")),
		sortBy: parseSortSearchParam(
			searchParams.get("sortBy"),
			TEAM_SORT_BY_OPTIONS,
			DEFAULT_TEAM_SORT_BY,
		),
		sortOrder: parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_TEAM_SORT_ORDER,
		),
	}).toString();
}

function mergeManagedTeamSearchParams(
	searchParams: URLSearchParams,
	managedSearchParams: URLSearchParams,
) {
	const merged = new URLSearchParams(searchParams);
	for (const key of TEAM_MANAGED_QUERY_KEYS) {
		merged.delete(key);
	}
	for (const [key, value] of managedSearchParams.entries()) {
		merged.set(key, value);
	}
	return merged;
}

function getDefaultPolicyGroupId(policyGroups: StoragePolicyGroup[]) {
	return (
		policyGroups.find(
			(group) => group.is_default && group.is_enabled && group.items.length > 0,
		)?.id ??
		policyGroups.find((group) => group.is_enabled && group.items.length > 0)
			?.id ??
		null
	);
}

function buildPolicyGroupOptions(
	policyGroups: StoragePolicyGroup[],
	selectedPolicyGroupId: number | null,
): TeamPolicyGroupOption[] {
	const options: TeamPolicyGroupOption[] = policyGroups
		.filter((group) => group.is_enabled && group.items.length > 0)
		.map((group) => ({
			label: group.name,
			value: String(group.id),
		}));

	if (
		selectedPolicyGroupId != null &&
		!options.some((option) => option.value === String(selectedPolicyGroupId))
	) {
		const selectedGroup = policyGroups.find(
			(group) => group.id === selectedPolicyGroupId,
		);
		options.unshift({
			label: selectedGroup?.name ?? `#${selectedPolicyGroupId}`,
			value: String(selectedPolicyGroupId),
			disabled: true,
		});
	}

	return options;
}

export default function AdminTeamsPage() {
	const { t } = useTranslation(["admin", "core"]);
	usePageTitle(t("teams"));
	const initialPolicyGroups = readAdminPolicyGroupLookup();
	const navigate = useNavigate();
	const [searchParams, setSearchParams] = useSearchParams();
	const initialKeyword = searchParams.get("keyword") ?? "";
	const [offset, setOffsetState] = useState(
		normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
	);
	const [pageSize, setPageSize] = useState<
		(typeof TEAM_PAGE_SIZE_OPTIONS)[number]
	>(
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			TEAM_PAGE_SIZE_OPTIONS,
			DEFAULT_TEAM_PAGE_SIZE,
		),
	);
	const [keyword, setKeyword] = useState(initialKeyword);
	const [showArchived, setShowArchived] = useState(
		parseArchivedSearchParam(searchParams.get("archived")),
	);
	const [sortBy, setSortBy] = useState<AdminTeamSortBy>(
		parseSortSearchParam(
			searchParams.get("sortBy"),
			TEAM_SORT_BY_OPTIONS,
			DEFAULT_TEAM_SORT_BY,
		),
	);
	const [sortOrder, setSortOrder] = useState<SortOrder>(
		parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_TEAM_SORT_ORDER,
		),
	);
	const [createDialogOpen, setCreateDialogOpen] = useState(false);
	const [createForm, setCreateForm] =
		useState<CreateTeamFormState>(EMPTY_CREATE_FORM);
	const [submitting, setSubmitting] = useState(false);
	const [policyGroups, setPolicyGroups] = useState<StoragePolicyGroup[]>(
		initialPolicyGroups ?? [],
	);
	const [policyGroupsLoading, setPolicyGroupsLoading] = useState(
		initialPolicyGroups == null,
	);
	const lastWrittenSearchRef = useRef<string | null>(null);
	const setOffset = (value: SetStateAction<number>) => {
		setOffsetState((current) =>
			normalizeOffset(typeof value === "function" ? value(current) : value),
		);
	};

	useEffect(() => {
		const managedSearch = getManagedTeamSearchString(searchParams);
		if (managedSearch === lastWrittenSearchRef.current) {
			return;
		}

		const nextOffset = normalizeOffset(
			parseOffsetSearchParam(searchParams.get("offset")),
		);
		const nextPageSize = parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			TEAM_PAGE_SIZE_OPTIONS,
			DEFAULT_TEAM_PAGE_SIZE,
		);
		const nextKeyword = searchParams.get("keyword") ?? "";
		const nextArchived = parseArchivedSearchParam(searchParams.get("archived"));
		const nextSortBy = parseSortSearchParam(
			searchParams.get("sortBy"),
			TEAM_SORT_BY_OPTIONS,
			DEFAULT_TEAM_SORT_BY,
		);
		const nextSortOrder = parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_TEAM_SORT_ORDER,
		);

		setOffsetState((prev) => (prev === nextOffset ? prev : nextOffset));
		setPageSize((prev) => (prev === nextPageSize ? prev : nextPageSize));
		setKeyword((prev) => (prev === nextKeyword ? prev : nextKeyword));
		setShowArchived((prev) => (prev === nextArchived ? prev : nextArchived));
		setSortBy((prev) => (prev === nextSortBy ? prev : nextSortBy));
		setSortOrder((prev) => (prev === nextSortOrder ? prev : nextSortOrder));
	}, [searchParams]);

	useEffect(() => {
		const nextManagedSearchParams = buildManagedTeamSearchParams({
			offset,
			pageSize,
			keyword,
			archived: showArchived,
			sortBy,
			sortOrder,
		});
		const nextSearch = nextManagedSearchParams.toString();
		const currentSearch = getManagedTeamSearchString(searchParams);
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
			mergeManagedTeamSearchParams(searchParams, nextManagedSearchParams),
			{ replace: true },
		);
	}, [
		keyword,
		offset,
		pageSize,
		searchParams,
		setSearchParams,
		showArchived,
		sortBy,
		sortOrder,
	]);

	const {
		items: teams,
		loading,
		reload,
		total,
	} = useApiList(
		() =>
			adminTeamService.list({
				archived: showArchived,
				keyword: keyword.trim() || undefined,
				limit: pageSize,
				offset,
				sort_by: sortBy,
				sort_order: sortOrder,
			}),
		[keyword, offset, pageSize, showArchived, sortBy, sortOrder],
	);

	const loadPolicyGroups = useCallback(
		async (options?: { force?: boolean }) => {
			try {
				const cachedPolicyGroups = readAdminPolicyGroupLookup();
				if (!options?.force && cachedPolicyGroups != null) {
					setPolicyGroups(cachedPolicyGroups);
					setPolicyGroupsLoading(false);
				} else {
					setPolicyGroupsLoading(true);
				}
				setPolicyGroups(await loadAdminPolicyGroupLookup(options));
			} catch (error) {
				handleApiError(error);
			} finally {
				setPolicyGroupsLoading(false);
			}
		},
		[],
	);

	useEffect(() => {
		void loadPolicyGroups();
	}, [loadPolicyGroups]);

	const defaultPolicyGroupId = getDefaultPolicyGroupId(policyGroups);
	const createPolicyGroupOptions = buildPolicyGroupOptions(
		policyGroups,
		createForm.policyGroupId
			? Number(createForm.policyGroupId)
			: defaultPolicyGroupId,
	);
	const createPolicyGroupUnavailable =
		!policyGroupsLoading && createPolicyGroupOptions.length === 0;
	const activeFilterCount =
		(keyword.trim().length > 0 ? 1 : 0) + (showArchived ? 1 : 0);
	const hasServerFilters = activeFilterCount > 0;
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;
	const pageSizeOptions = TEAM_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));

	useEffect(() => {
		if (
			createDialogOpen &&
			!createForm.policyGroupId &&
			defaultPolicyGroupId != null
		) {
			setCreateForm((prev) =>
				prev.policyGroupId
					? prev
					: { ...prev, policyGroupId: String(defaultPolicyGroupId) },
			);
		}
	}, [createDialogOpen, createForm.policyGroupId, defaultPolicyGroupId]);

	const handleOpenCreateDialog = () => {
		setCreateForm({
			...EMPTY_CREATE_FORM,
			policyGroupId:
				defaultPolicyGroupId != null ? String(defaultPolicyGroupId) : "",
		});
		setCreateDialogOpen(true);
	};

	const handleCreate = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		const name = createForm.name.trim();
		const adminIdentifier = createForm.adminIdentifier.trim();
		const policyGroupId = Number(createForm.policyGroupId);
		if (
			!name ||
			!adminIdentifier ||
			!Number.isSafeInteger(policyGroupId) ||
			policyGroupId <= 0
		) {
			return;
		}
		const storageQuota = createTeamQuotaBytes(createForm.quotaValue);
		if (storageQuota === null) {
			toast.error(t("team_quota_invalid"));
			return;
		}

		try {
			setSubmitting(true);
			await adminTeamService.create({
				name,
				description: createForm.description.trim() || undefined,
				admin_identifier: adminIdentifier,
				policy_group_id: policyGroupId,
				...(storageQuota === undefined ? {} : { storage_quota: storageQuota }),
			});
			setCreateDialogOpen(false);
			setCreateForm(EMPTY_CREATE_FORM);
			toast.success(t("team_created"));
			await reload();
		} catch (error) {
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	const handleRefresh = async () => {
		await Promise.all([reload(), loadPolicyGroups({ force: true })]);
	};

	const policyGroupNameById = (policyGroupId: number | null | undefined) =>
		policyGroupId != null
			? (policyGroups.find((group) => group.id === policyGroupId)?.name ?? null)
			: null;

	const resetFilters = () => {
		setKeyword("");
		setShowArchived(false);
		setOffset(0);
	};

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, TEAM_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
	};

	const handleKeywordChange = (value: string) => {
		setKeyword(value);
		setOffset(0);
	};

	const handleSortChange = (
		nextSortBy: AdminTeamSortBy,
		nextOrder: SortOrder,
	) => {
		setSortBy(nextSortBy);
		setSortOrder(nextOrder);
		setOffset(0);
	};

	const handleArchivedToggle = () => {
		setShowArchived((prev) => !prev);
		setOffset(0);
	};
	const openTeam = (team: AdminTeamInfo) => {
		navigate(`/admin/teams/${team.id}/overview`, {
			viewTransition: false,
		});
	};
	const teamsEmptyIcon = <Icon name="Cloud" className="size-10" />;
	const teamsFilteredEmptyAction = (
		<Button variant="outline" onClick={resetFilters}>
			{t("clear_filters")}
		</Button>
	);
	const teamsTableHeader = (
		<AdminTeamsTableHeader
			sortBy={sortBy}
			sortOrder={sortOrder}
			onSortChange={handleSortChange}
		/>
	);
	const teamsPagination = (
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
					title={t("teams")}
					description={t("teams_intro")}
					actions={
						<>
							<Button
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={handleOpenCreateDialog}
							>
								<Icon name="Plus" className="mr-1 size-4" />
								{t("new_team")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void handleRefresh()}
								disabled={loading || policyGroupsLoading}
							>
								<Icon
									name={
										loading || policyGroupsLoading
											? "Spinner"
											: "ArrowsClockwise"
									}
									className={`mr-1 size-3.5 ${loading || policyGroupsLoading ? "animate-spin" : ""}`}
								/>
								{t("core:refresh")}
							</Button>
						</>
					}
					toolbar={
						<AdminTeamsToolbar
							activeFilterCount={activeFilterCount}
							keyword={keyword}
							onArchivedToggle={handleArchivedToggle}
							onKeywordChange={handleKeywordChange}
							onResetFilters={resetFilters}
							showArchived={showArchived}
						/>
					}
				/>

				<AdminTableList
					loading={loading}
					items={teams}
					columns={6}
					rows={6}
					emptyIcon={teamsEmptyIcon}
					emptyTitle={t("no_teams")}
					emptyDescription={t("no_teams_desc")}
					filtered={hasServerFilters}
					filteredEmptyTitle={t("no_filtered_teams")}
					filteredEmptyDescription={t("no_filtered_teams_desc")}
					filteredEmptyAction={teamsFilteredEmptyAction}
					headerRow={teamsTableHeader}
					pagination={teamsPagination}
					renderRow={(team) => (
						<AdminTeamsTableRow
							key={team.id}
							onOpenTeam={openTeam}
							policyGroupNameById={policyGroupNameById}
							team={team}
						/>
					)}
				/>
			</AdminPageShell>
			<CreateTeamDialog
				open={createDialogOpen}
				form={createForm}
				submitting={submitting}
				policyGroupsLoading={policyGroupsLoading}
				policyGroupOptions={createPolicyGroupOptions}
				policyGroupUnavailable={createPolicyGroupUnavailable}
				onOpenChange={setCreateDialogOpen}
				onSubmit={(event) => void handleCreate(event)}
				onFieldChange={(key, value) =>
					setCreateForm((prev) => ({ ...prev, [key]: value }))
				}
			/>
		</AdminLayout>
	);
}
