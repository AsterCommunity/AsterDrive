import { type FormEvent, useCallback, useEffect, useState } from "react";
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
import {
	useManagedAdminList,
	useManagedOffset,
} from "@/hooks/useManagedAdminList";
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
	loadAdminPolicyGroupLookup,
	readAdminPolicyGroupLookup,
} from "@/lib/adminPolicyGroupLookup";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { parsePageSizeOption, type SortOrder } from "@/lib/pagination";
import { parseStorageQuotaMbToBytes } from "@/lib/storageQuota";
import { adminTeamService } from "@/services/adminService";
import type { AdminTeamSortBy } from "@/types/adminSort";
import type { AdminTeamInfo, StoragePolicyGroup } from "@/types/api";

const TEAM_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_TEAM_PAGE_SIZE = 20 as const;
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

function parseArchivedSearchParam(value: string | null) {
	return value === "1" || value === "true";
}

type ManagedTeamQuery = {
	archived: boolean;
	keyword: string;
	offset: number;
	pageSize: (typeof TEAM_PAGE_SIZE_OPTIONS)[number];
	sortBy: AdminTeamSortBy;
	sortOrder: SortOrder;
};

function createTeamQuotaBytes(value: string) {
	const normalized = value.trim();
	if (!normalized) return undefined;

	return parseStorageQuotaMbToBytes(normalized);
}

const MANAGED_TEAM_QUERY_DEFAULTS = {
	archived: false,
	keyword: "",
	offset: 0,
	pageSize: DEFAULT_TEAM_PAGE_SIZE,
	sortBy: DEFAULT_TEAM_SORT_BY,
	sortOrder: DEFAULT_TEAM_SORT_ORDER,
} satisfies ManagedTeamQuery;

const MANAGED_TEAM_QUERY_SCHEMA = {
	archived: {
		keys: ["archived"],
		parse: (searchParams) =>
			parseArchivedSearchParam(searchParams.get("archived")),
		serialize: (value) => (value ? true : undefined),
	},
	keyword: managedStringQueryField({ key: "keyword" }),
	offset: managedOffsetQueryField(),
	pageSize: managedPageSizeQueryField(
		TEAM_PAGE_SIZE_OPTIONS,
		DEFAULT_TEAM_PAGE_SIZE,
	),
	sortBy: managedSortByQueryField(TEAM_SORT_BY_OPTIONS, DEFAULT_TEAM_SORT_BY),
	sortOrder: managedSortOrderQueryField(DEFAULT_TEAM_SORT_ORDER),
} satisfies ManagedListQuerySchema<ManagedTeamQuery>;

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
	const { query, setQuery } = useManagedListQueryState({
		defaults: MANAGED_TEAM_QUERY_DEFAULTS,
		schema: MANAGED_TEAM_QUERY_SCHEMA,
		searchParams,
		setSearchParams,
	});
	const {
		archived: showArchived,
		keyword: debouncedKeyword,
		pageSize,
		sortBy,
		sortOrder,
	} = query;
	const [keyword, setKeyword] = useState(debouncedKeyword);
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
	const setOffset = useManagedOffset(setQuery);

	const {
		currentPage,
		items: teams,
		loading,
		nextPageDisabled,
		prevPageDisabled,
		reload,
		total,
		totalPages,
	} = useManagedAdminList<AdminTeamInfo, ManagedTeamQuery>({
		loadPage: (query) =>
			adminTeamService.list({
				archived: query.archived,
				keyword: query.keyword.trim() || undefined,
				limit: query.pageSize,
				offset: query.offset,
				sort_by: query.sortBy,
				sort_order: query.sortOrder,
			}),
		query,
		setOffset,
	});

	useEffect(() => {
		if (keyword === debouncedKeyword) {
			return;
		}
		const timer = window.setTimeout(() => {
			setQuery({ keyword, offset: 0 });
		}, 300);
		return () => window.clearTimeout(timer);
	}, [debouncedKeyword, keyword, setQuery]);

	useEffect(() => {
		setKeyword((current) =>
			current === debouncedKeyword ? current : debouncedKeyword,
		);
	}, [debouncedKeyword]);

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
		(debouncedKeyword.trim().length > 0 ? 1 : 0) + (showArchived ? 1 : 0);
	const hasServerFilters = activeFilterCount > 0;
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
		setQuery({ archived: false, keyword: "", offset: 0 });
	};

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, TEAM_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setQuery({ offset: 0, pageSize: next });
	};

	const handleKeywordChange = (value: string) => {
		setKeyword(value);
	};

	const handleSortChange = (
		nextSortBy: AdminTeamSortBy,
		nextOrder: SortOrder,
	) => {
		setQuery({ offset: 0, sortBy: nextSortBy, sortOrder: nextOrder });
	};

	const handleArchivedToggle = () => {
		setQuery((current) => ({ archived: !current.archived, offset: 0 }));
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
