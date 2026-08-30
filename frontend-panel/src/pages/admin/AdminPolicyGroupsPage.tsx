import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import type { PolicyLookup } from "@/components/admin/PolicyGroupEditorForm";
import { PolicyGroupMigrationDialog } from "@/components/admin/PolicyGroupMigrationDialog";
import { PolicyGroupSimulationDialog } from "@/components/admin/PolicyGroupSimulationDialog";
import { PolicyGroupsTable } from "@/components/admin/PolicyGroupsTable";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { getApiErrorMessage, handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
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
	useManagedListQueryState,
} from "@/hooks/useManagedListQueryState";
import { usePageTitle } from "@/hooks/usePageTitle";
import { usePendingId } from "@/hooks/usePendingId";
import {
	invalidateAdminPolicyGroupLookup,
	loadAdminPolicyGroupLookup,
} from "@/lib/adminPolicyGroupLookup";
import {
	loadAdminPolicyLookup,
	readAdminPolicyLookup,
} from "@/lib/adminPolicyLookup";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { parsePageSizeOption, type SortOrder } from "@/lib/pagination";
import { adminPolicyGroupService } from "@/services/adminService";
import type { AdminPolicyGroupSortBy } from "@/types/adminSort";
import type {
	PolicyGroupAssignmentMigrationResult,
	StoragePlacementSimulationResult,
	StoragePolicyGroup,
} from "@/types/api";

const POLICY_GROUP_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_POLICY_GROUP_PAGE_SIZE = 20 as const;
const POLICY_GROUP_LOOKUP_PAGE_SIZE = 100;
const POLICY_LOOKUP_PAGE_SIZE = 100;
const BYTES_PER_MB = 1024 * 1024;
const POLICY_GROUP_SORT_BY_OPTIONS = [
	"id",
	"name",
	"is_enabled",
	"is_default",
	"created_at",
	"updated_at",
] as const satisfies readonly AdminPolicyGroupSortBy[];
const DEFAULT_POLICY_GROUP_SORT_BY =
	"created_at" as const satisfies AdminPolicyGroupSortBy;
const DEFAULT_POLICY_GROUP_SORT_ORDER = "desc" as const satisfies SortOrder;

type ManagedPolicyGroupQuery = {
	offset: number;
	pageSize: (typeof POLICY_GROUP_PAGE_SIZE_OPTIONS)[number];
	sortBy: AdminPolicyGroupSortBy;
	sortOrder: SortOrder;
};

const MANAGED_POLICY_GROUP_QUERY_DEFAULTS = {
	offset: 0,
	pageSize: DEFAULT_POLICY_GROUP_PAGE_SIZE,
	sortBy: DEFAULT_POLICY_GROUP_SORT_BY,
	sortOrder: DEFAULT_POLICY_GROUP_SORT_ORDER,
} satisfies ManagedPolicyGroupQuery;

const MANAGED_POLICY_GROUP_QUERY_SCHEMA = {
	offset: managedOffsetQueryField(),
	pageSize: managedPageSizeQueryField(
		POLICY_GROUP_PAGE_SIZE_OPTIONS,
		DEFAULT_POLICY_GROUP_PAGE_SIZE,
	),
	sortBy: managedSortByQueryField(
		POLICY_GROUP_SORT_BY_OPTIONS,
		DEFAULT_POLICY_GROUP_SORT_BY,
	),
	sortOrder: managedSortOrderQueryField(DEFAULT_POLICY_GROUP_SORT_ORDER),
} satisfies ManagedListQuerySchema<ManagedPolicyGroupQuery>;

function getMigrationSuccessMessage(
	t: ReturnType<typeof useTranslation>["t"],
	result: PolicyGroupAssignmentMigrationResult,
	sourceName: string,
	targetName: string,
) {
	return t("policy_group_migration_success", {
		users: result.affected_users,
		teams: result.affected_teams,
		total: result.migrated_assignments,
		source: sourceName,
		target: targetName,
	});
}

export default function AdminPolicyGroupsPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("policy_groups"));
	const navigate = useNavigate();
	const [searchParams, setSearchParams] = useSearchParams();
	const { query, setQuery } = useManagedListQueryState({
		defaults: MANAGED_POLICY_GROUP_QUERY_DEFAULTS,
		schema: MANAGED_POLICY_GROUP_QUERY_SCHEMA,
		searchParams,
		setSearchParams,
	});
	const { offset, pageSize, sortBy, sortOrder } = query;
	const setOffset = useManagedOffset(setQuery);
	const {
		currentPage,
		items: groups,
		total,
		totalPages,
		loading,
		reload,
		nextPageDisabled,
		prevPageDisabled,
	} = useManagedAdminList<StoragePolicyGroup, ManagedPolicyGroupQuery>({
		loadPage: (query) =>
			adminPolicyGroupService.list({
				limit: query.pageSize,
				offset: query.offset,
				sort_by: query.sortBy,
				sort_order: query.sortOrder,
			}),
		query,
		setOffset,
	});
	const initialPolicies = readAdminPolicyLookup();
	const [policies, setPolicies] = useState<PolicyLookup[]>(
		initialPolicies ?? [],
	);
	const [policiesLoading, setPoliciesLoading] = useState(
		initialPolicies == null,
	);
	const [migrationDialogOpen, setMigrationDialogOpen] = useState(false);
	const [migrationError, setMigrationError] = useState<string | null>(null);
	const [migrationSourceId, setMigrationSourceId] = useState<number | null>(
		null,
	);
	const [migrationSubmitting, setMigrationSubmitting] = useState(false);
	const [migrationTargetId, setMigrationTargetId] = useState("");
	const [migrationGroups, setMigrationGroups] = useState<
		StoragePolicyGroup[] | null
	>(null);
	const [migrationGroupsLoading, setMigrationGroupsLoading] = useState(false);
	const [simulationGroup, setSimulationGroup] =
		useState<StoragePolicyGroup | null>(null);
	const [simulationFilename, setSimulationFilename] = useState("example.pdf");
	const [simulationFileSizeMb, setSimulationFileSizeMb] = useState("1");
	const [simulationMimeType, setSimulationMimeType] = useState(
		"application/octet-stream",
	);
	const [simulationFolderPolicyId, setSimulationFolderPolicyId] = useState("");
	const [simulationResult, setSimulationResult] =
		useState<StoragePlacementSimulationResult | null>(null);
	const [simulationError, setSimulationError] = useState<string | null>(null);
	const [simulationSubmitting, setSimulationSubmitting] = useState(false);
	const { pendingId: deletingGroupId, runWithPending: runWithDeletingGroup } =
		usePendingId<number>();
	const refreshing = loading || policiesLoading;
	const pageSizeOptions = POLICY_GROUP_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));
	const availableMigrationGroups =
		migrationGroups ?? (total <= groups.length ? groups : []);
	const migrationSourceGroup =
		migrationSourceId === null
			? null
			: (availableMigrationGroups.find(
					(group) => group.id === migrationSourceId,
				) ??
				groups.find((group) => group.id === migrationSourceId) ??
				null);
	const migrationTargetOptions =
		migrationSourceGroup === null
			? []
			: availableMigrationGroups.filter(
					(group) => group.id !== migrationSourceGroup.id,
				);
	const migrationTargetSelectOptions = migrationTargetOptions.map((group) => ({
		label: group.name,
		value: String(group.id),
	}));
	const selectedMigrationTarget =
		migrationTargetOptions.find(
			(group) => String(group.id) === migrationTargetId,
		) ?? null;

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, POLICY_GROUP_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setQuery({ offset: 0, pageSize: next });
	};

	const handleSortChange = (
		nextSortBy: AdminPolicyGroupSortBy,
		nextOrder: SortOrder,
	) => {
		setQuery({ offset: 0, sortBy: nextSortBy, sortOrder: nextOrder });
	};

	const loadPolicies = useCallback(
		async ({ force = false }: { force?: boolean } = {}) => {
			try {
				setPoliciesLoading(true);
				const policyLookup = await loadAdminPolicyLookup({
					force,
					limit: POLICY_LOOKUP_PAGE_SIZE,
				});
				setPolicies(policyLookup);
			} catch (e) {
				handleApiError(e);
			} finally {
				setPoliciesLoading(false);
			}
		},
		[],
	);

	useEffect(() => {
		void loadPolicies();
	}, [loadPolicies]);

	useEffect(() => {
		if (
			!migrationDialogOpen ||
			migrationSourceId === null ||
			migrationGroupsLoading
		) {
			return;
		}

		const nextTargets = availableMigrationGroups.filter(
			(group) => group.id !== migrationSourceId,
		);
		if (nextTargets.length === 0) {
			if (migrationTargetId) {
				setMigrationTargetId("");
			}
			return;
		}

		if (!nextTargets.some((group) => String(group.id) === migrationTargetId)) {
			setMigrationTargetId(String(nextTargets[0].id));
		}
	}, [
		availableMigrationGroups,
		migrationDialogOpen,
		migrationGroupsLoading,
		migrationSourceId,
		migrationTargetId,
	]);

	const loadAllPolicyGroups = useCallback(async () => {
		try {
			setMigrationGroupsLoading(true);
			setMigrationGroups(
				await loadAdminPolicyGroupLookup({
					limit: POLICY_GROUP_LOOKUP_PAGE_SIZE,
				}),
			);
		} catch (e) {
			handleApiError(e);
		} finally {
			setMigrationGroupsLoading(false);
		}
	}, []);

	const handleRefresh = async () => {
		invalidateAdminPolicyGroupLookup();
		await Promise.all([reload(), loadPolicies({ force: true })]);
	};

	const resetMigrationState = () => {
		setMigrationError(null);
		setMigrationGroups(null);
		setMigrationGroupsLoading(false);
		setMigrationSourceId(null);
		setMigrationSubmitting(false);
		setMigrationTargetId("");
	};

	const resetSimulationState = () => {
		setSimulationFilename("example.pdf");
		setSimulationFileSizeMb("1");
		setSimulationMimeType("application/octet-stream");
		setSimulationFolderPolicyId("");
		setSimulationResult(null);
		setSimulationError(null);
		setSimulationSubmitting(false);
	};

	const openMigrationDialog = (group: StoragePolicyGroup) => {
		setMigrationSourceId(group.id);
		setMigrationTargetId("");
		setMigrationError(null);
		setMigrationGroups(total <= groups.length ? groups : null);
		setMigrationDialogOpen(true);
		if (total > groups.length) {
			void loadAllPolicyGroups();
		}
	};

	const openSimulationDialog = (group: StoragePolicyGroup) => {
		resetSimulationState();
		setSimulationGroup(group);
	};

	const handleMigrationDialogOpenChange = (open: boolean) => {
		setMigrationDialogOpen(open);
		if (!open) {
			resetMigrationState();
		}
	};

	const handleSimulationDialogOpenChange = (open: boolean) => {
		if (!open) {
			setSimulationGroup(null);
			resetSimulationState();
		}
	};

	const handleSimulationInputChange = (
		setter: (value: string) => void,
		value: string,
	) => {
		setter(value);
		setSimulationResult(null);
		setSimulationError(null);
	};

	const runSimulation = async () => {
		if (!simulationGroup) return;
		const filename = simulationFilename.trim();
		if (!filename) {
			setSimulationError(t("policy_group_simulator_filename_required"));
			return;
		}
		const fileSizeMb = Number(simulationFileSizeMb);
		const fileSize = Math.round(fileSizeMb * BYTES_PER_MB);
		if (
			!Number.isFinite(fileSizeMb) ||
			fileSizeMb < 0 ||
			!Number.isSafeInteger(fileSize)
		) {
			setSimulationError(t("policy_group_simulator_size_invalid"));
			return;
		}
		const folderPolicyId = simulationFolderPolicyId
			? Number(simulationFolderPolicyId)
			: null;
		if (folderPolicyId !== null && !Number.isSafeInteger(folderPolicyId)) {
			setSimulationError(t("policy_group_simulator_folder_policy_invalid"));
			return;
		}

		try {
			setSimulationSubmitting(true);
			setSimulationError(null);
			setSimulationResult(
				await adminPolicyGroupService.simulate(simulationGroup.id, {
					filename,
					file_size: fileSize,
					mime_type: simulationMimeType.trim(),
					folder_policy_id: folderPolicyId,
				}),
			);
		} catch (error) {
			setSimulationResult(null);
			setSimulationError(getApiErrorMessage(error));
		} finally {
			setSimulationSubmitting(false);
		}
	};

	const handleDelete = async (id: number) => {
		await runWithDeletingGroup(id, async () => {
			try {
				await adminPolicyGroupService.delete(id);
				invalidateAdminPolicyGroupLookup();
				if (groups.length === 1 && offset > 0) {
					setOffset(Math.max(0, offset - pageSize));
				} else {
					await reload();
				}
				toast.success(t("policy_group_deleted"));
			} catch (e) {
				handleApiError(e);
			}
		});
	};

	const handleMigrateUsers = async () => {
		if (!migrationSourceGroup) {
			return;
		}
		if (!migrationTargetId) {
			setMigrationError(t("policy_group_migration_target_required"));
			return;
		}

		const targetGroupId = Number(migrationTargetId);
		if (!Number.isInteger(targetGroupId)) {
			setMigrationError(t("policy_group_migration_target_required"));
			return;
		}
		if (targetGroupId === migrationSourceGroup.id) {
			setMigrationError(t("policy_group_migration_same_group_invalid"));
			return;
		}

		const targetGroupName =
			selectedMigrationTarget?.name ?? `#${targetGroupId}`;

		try {
			setMigrationSubmitting(true);
			setMigrationError(null);
			const result = await adminPolicyGroupService.migrateAssignments(
				migrationSourceGroup.id,
				{ target_group_id: targetGroupId },
			);
			invalidateAdminPolicyGroupLookup();
			await reload();
			toast.success(
				getMigrationSuccessMessage(
					t,
					result,
					migrationSourceGroup.name,
					targetGroupName,
				),
			);
			handleMigrationDialogOpenChange(false);
		} catch (e) {
			handleApiError(e);
		} finally {
			setMigrationSubmitting(false);
		}
	};

	const {
		confirmId: deleteId,
		requestConfirm,
		dialogProps,
	} = useConfirmDialog(handleDelete);

	const deleteGroupName =
		deleteId !== null
			? (groups.find((group) => group.id === deleteId)?.name ?? "")
			: "";

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					className="px-0 md:px-0"
					title={t("policy_groups")}
					description={t("policy_groups_intro")}
					actions={
						<>
							<Button
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() =>
									navigate("/admin/policy-groups/new", {
										viewTransition: false,
									})
								}
							>
								<Icon name="Plus" className="mr-1 size-4" />
								{t("new_policy_group")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void handleRefresh()}
								disabled={refreshing}
							>
								<Icon
									name={refreshing ? "Spinner" : "ArrowsClockwise"}
									className={`mr-1 size-3.5 ${refreshing ? "animate-spin" : ""}`}
								/>
								{t("core:refresh")}
							</Button>
						</>
					}
				/>

				<PolicyGroupsTable
					groups={groups}
					loading={loading}
					deletingGroupId={deletingGroupId}
					total={total}
					currentPage={currentPage}
					totalPages={totalPages}
					pageSize={pageSize}
					pageSizeOptions={pageSizeOptions}
					prevPageDisabled={prevPageDisabled}
					sortBy={sortBy}
					sortOrder={sortOrder}
					nextPageDisabled={nextPageDisabled}
					onPageSizeChange={handlePageSizeChange}
					onPreviousPage={() =>
						setOffset((current) => Math.max(0, current - pageSize))
					}
					onNextPage={() => setOffset((current) => current + pageSize)}
					onOpenEdit={(group) =>
						navigate(`/admin/policy-groups/${group.id}`, {
							viewTransition: false,
						})
					}
					onOpenMigration={openMigrationDialog}
					onOpenSimulation={openSimulationDialog}
					onRequestDelete={requestConfirm}
					onSortChange={handleSortChange}
				/>

				<PolicyGroupMigrationDialog
					open={migrationDialogOpen}
					onOpenChange={handleMigrationDialogOpenChange}
					sourceGroupName={migrationSourceGroup?.name ?? null}
					targetGroupId={migrationTargetId}
					targetOptions={migrationTargetSelectOptions}
					loading={migrationGroupsLoading}
					submitting={migrationSubmitting}
					error={migrationError}
					onTargetGroupChange={(value) => {
						setMigrationTargetId(value);
						setMigrationError(null);
					}}
					onConfirm={() => void handleMigrateUsers()}
				/>

				<PolicyGroupSimulationDialog
					open={simulationGroup !== null}
					group={simulationGroup}
					policies={policies}
					filename={simulationFilename}
					fileSizeMb={simulationFileSizeMb}
					mimeType={simulationMimeType}
					folderPolicyId={simulationFolderPolicyId}
					result={simulationResult}
					error={simulationError}
					submitting={simulationSubmitting}
					onOpenChange={handleSimulationDialogOpenChange}
					onFilenameChange={(value) =>
						handleSimulationInputChange(setSimulationFilename, value)
					}
					onFileSizeMbChange={(value) =>
						handleSimulationInputChange(setSimulationFileSizeMb, value)
					}
					onMimeTypeChange={(value) =>
						handleSimulationInputChange(setSimulationMimeType, value)
					}
					onFolderPolicyIdChange={(value) =>
						handleSimulationInputChange(setSimulationFolderPolicyId, value)
					}
					onSimulate={() => void runSimulation()}
				/>

				<ConfirmDialog
					{...dialogProps}
					title={`${t("delete_policy_group")} "${deleteGroupName}"?`}
					description={t("delete_policy_group_desc")}
					confirmLabel={t("core:delete")}
					variant="destructive"
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}
