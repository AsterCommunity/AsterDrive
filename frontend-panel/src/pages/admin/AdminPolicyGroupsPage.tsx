import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import {
	PolicyGroupDialog,
	type PolicyLookup,
} from "@/components/admin/PolicyGroupDialog";
import { PolicyGroupMigrationDialog } from "@/components/admin/PolicyGroupMigrationDialog";
import { PolicyGroupsTable } from "@/components/admin/PolicyGroupsTable";
import {
	buildPolicyGroupPayload,
	buildPolicyGroupRuleForm,
	getDefaultPolicyGroupForm,
	getPolicyGroupForm,
	type PolicyGroupFormData,
	type PolicyGroupRuleForm,
	validatePolicyGroupForm,
} from "@/components/admin/policyGroupDialogShared";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useApiList } from "@/hooks/useApiList";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import {
	invalidateAdminPolicyGroupLookup,
	loadAdminPolicyGroupLookup,
} from "@/lib/adminPolicyGroupLookup";
import {
	loadAdminPolicyLookup,
	readAdminPolicyLookup,
} from "@/lib/adminPolicyLookup";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import {
	buildOffsetPaginationSearchParams,
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
} from "@/lib/pagination";
import { adminPolicyGroupService } from "@/services/adminService";
import type {
	PolicyGroupUserMigrationResult,
	StoragePolicyGroup,
} from "@/types/api";

const POLICY_GROUP_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_POLICY_GROUP_PAGE_SIZE = 20 as const;
const POLICY_GROUP_LOOKUP_PAGE_SIZE = 100;
const POLICY_LOOKUP_PAGE_SIZE = 100;

function getMigrationSuccessMessage(
	t: ReturnType<typeof useTranslation>["t"],
	result: PolicyGroupUserMigrationResult,
	sourceName: string,
	targetName: string,
) {
	return t("policy_group_migration_success", {
		affected: result.affected_users,
		source: sourceName,
		target: targetName,
	});
}

function mergePolicies(
	current: PolicyLookup[],
	incoming: PolicyLookup[],
): PolicyLookup[] {
	if (incoming.length === 0) return current;
	const merged = [...current];
	const seen = new Set(current.map((policy) => policy.id));
	for (const policy of incoming) {
		if (seen.has(policy.id)) continue;
		seen.add(policy.id);
		merged.push(policy);
	}
	return merged;
}

export default function AdminPolicyGroupsPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("policy_groups"));
	const [searchParams, setSearchParams] = useSearchParams();
	const [offset, setOffset] = useState(
		parseOffsetSearchParam(searchParams.get("offset")),
	);
	const [pageSize, setPageSize] = useState<
		(typeof POLICY_GROUP_PAGE_SIZE_OPTIONS)[number]
	>(
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			POLICY_GROUP_PAGE_SIZE_OPTIONS,
			DEFAULT_POLICY_GROUP_PAGE_SIZE,
		),
	);
	const {
		items: groups,
		total,
		loading,
		reload,
	} = useApiList(
		() => adminPolicyGroupService.list({ limit: pageSize, offset }),
		[offset, pageSize],
	);
	const initialPolicies = readAdminPolicyLookup();
	const [policies, setPolicies] = useState<PolicyLookup[]>(
		initialPolicies ?? [],
	);
	const [loadedPoliciesCount, setLoadedPoliciesCount] = useState(
		initialPolicies?.length ?? 0,
	);
	const [policiesTotal, setPoliciesTotal] = useState(
		initialPolicies?.length ?? 0,
	);
	const [policiesLoading, setPoliciesLoading] = useState(
		initialPolicies == null,
	);
	const [policiesLoadingMore, setPoliciesLoadingMore] = useState(false);
	const [dialogOpen, setDialogOpen] = useState(false);
	const [editingGroup, setEditingGroup] = useState<StoragePolicyGroup | null>(
		null,
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
	const [form, setForm] = useState<PolicyGroupFormData>(() =>
		getDefaultPolicyGroupForm([]),
	);
	const [formError, setFormError] = useState<string | null>(null);
	const [submitting, setSubmitting] = useState(false);
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;
	const hasMorePolicies = loadedPoliciesCount < policiesTotal;
	const refreshing = loading || policiesLoading || policiesLoadingMore;
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

	useEffect(() => {
		setSearchParams(
			buildOffsetPaginationSearchParams({
				offset,
				pageSize,
				defaultPageSize: DEFAULT_POLICY_GROUP_PAGE_SIZE,
			}),
			{ replace: true },
		);
	}, [offset, pageSize, setSearchParams]);

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, POLICY_GROUP_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
	};

	const loadPolicies = useCallback(
		async ({ force = false }: { force?: boolean } = {}) => {
			try {
				setPoliciesLoading(true);
				setPoliciesLoadingMore(false);
				const policyLookup = await loadAdminPolicyLookup({
					force,
					limit: POLICY_LOOKUP_PAGE_SIZE,
				});
				setPoliciesTotal(policyLookup.length);
				setLoadedPoliciesCount(policyLookup.length);
				setPolicies(policyLookup);
			} catch (e) {
				handleApiError(e);
			} finally {
				setPoliciesLoading(false);
				setPoliciesLoadingMore(false);
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

	const reloadPolicies = useCallback(
		async (options?: { force?: boolean }) => {
			await loadPolicies(options);
		},
		[loadPolicies],
	);

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

	const loadMorePolicies = useCallback(async () => {
		if (policiesLoading || policiesLoadingMore || !hasMorePolicies) {
			return;
		}
		await loadPolicies();
	}, [hasMorePolicies, loadPolicies, policiesLoading, policiesLoadingMore]);

	const handleRefresh = async () => {
		invalidateAdminPolicyGroupLookup();
		await Promise.all([reload(), reloadPolicies({ force: true })]);
	};

	const setField = <K extends keyof PolicyGroupFormData>(
		key: K,
		value: PolicyGroupFormData[K],
	) => {
		setForm((prev) => ({ ...prev, [key]: value }));
		setFormError(null);
	};

	const setRuleField = <K extends Exclude<keyof PolicyGroupRuleForm, "key">>(
		ruleKey: string,
		key: K,
		value: PolicyGroupRuleForm[K],
	) => {
		setForm((prev) => ({
			...prev,
			items: prev.items.map((item) =>
				item.key === ruleKey ? { ...item, [key]: value } : item,
			),
		}));
		setFormError(null);
	};

	const getNextPolicyId = () => {
		const selected = new Set(
			form.items.map((item) => item.policyId).filter(Boolean),
		);
		return (
			policies.find((policy) => !selected.has(String(policy.id)))?.id ??
			policies[0]?.id ??
			null
		);
	};

	const addRule = () => {
		setForm((prev) => ({
			...prev,
			items: [
				...prev.items,
				buildPolicyGroupRuleForm(getNextPolicyId(), prev.items.length + 1),
			],
		}));
		setFormError(null);
	};

	const removeRule = (ruleKey: string) => {
		setForm((prev) => ({
			...prev,
			items: prev.items.filter((item) => item.key !== ruleKey),
		}));
		setFormError(null);
	};

	const resetDialogState = () => {
		setFormError(null);
		setSubmitting(false);
	};

	const resetMigrationState = () => {
		setMigrationError(null);
		setMigrationGroups(null);
		setMigrationGroupsLoading(false);
		setMigrationSourceId(null);
		setMigrationSubmitting(false);
		setMigrationTargetId("");
	};

	const openCreate = () => {
		setEditingGroup(null);
		setForm(getDefaultPolicyGroupForm(policies));
		resetDialogState();
		setDialogOpen(true);
	};

	const openEdit = (group: StoragePolicyGroup) => {
		setPolicies((prev) =>
			mergePolicies(
				prev,
				group.items.map((item) => item.policy),
			),
		);
		setEditingGroup(group);
		setForm(getPolicyGroupForm(group));
		resetDialogState();
		setDialogOpen(true);
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

	const handleDialogOpenChange = (open: boolean) => {
		setDialogOpen(open);
		if (!open) {
			resetDialogState();
		}
	};

	const handleMigrationDialogOpenChange = (open: boolean) => {
		setMigrationDialogOpen(open);
		if (!open) {
			resetMigrationState();
		}
	};

	const submitForm = async () => {
		const validationError = validatePolicyGroupForm(form, policies.length, t);
		if (validationError) {
			setFormError(validationError);
			return;
		}

		const payload = buildPolicyGroupPayload(form);

		try {
			setSubmitting(true);
			if (editingGroup) {
				await adminPolicyGroupService.update(editingGroup.id, payload);
				invalidateAdminPolicyGroupLookup();
				await reload();
				toast.success(t("policy_group_updated"));
			} else {
				await adminPolicyGroupService.create(payload);
				invalidateAdminPolicyGroupLookup();
				const nextTotal = total + 1;
				const nextLastOffset = Math.max(
					0,
					Math.floor((nextTotal - 1) / pageSize) * pageSize,
				);
				if (nextLastOffset !== offset) {
					setOffset(nextLastOffset);
				} else {
					await reload();
				}
				toast.success(t("policy_group_created"));
			}
			handleDialogOpenChange(false);
		} catch (e) {
			handleApiError(e);
		} finally {
			setSubmitting(false);
		}
	};

	const handleDelete = async (id: number) => {
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
			const result = await adminPolicyGroupService.migrateUsers(
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
					title={t("policy_groups")}
					description={t("policy_groups_intro")}
					actions={
						<>
							<Button
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={openCreate}
								disabled={policiesLoading || policies.length === 0}
							>
								<Icon name="Plus" className="mr-1 h-4 w-4" />
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
									className={`mr-1 h-3.5 w-3.5 ${refreshing ? "animate-spin" : ""}`}
								/>
								{t("core:refresh")}
							</Button>
						</>
					}
				/>

				<PolicyGroupsTable
					groups={groups}
					loading={loading}
					total={total}
					currentPage={currentPage}
					totalPages={totalPages}
					pageSize={pageSize}
					pageSizeOptions={pageSizeOptions}
					prevPageDisabled={prevPageDisabled}
					nextPageDisabled={nextPageDisabled}
					onPageSizeChange={handlePageSizeChange}
					onPreviousPage={() => setOffset(Math.max(0, offset - pageSize))}
					onNextPage={() => setOffset(offset + pageSize)}
					onOpenEdit={openEdit}
					onOpenMigration={openMigrationDialog}
					onRequestDelete={requestConfirm}
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

				<ConfirmDialog
					{...dialogProps}
					title={`${t("delete_policy_group")} "${deleteGroupName}"?`}
					description={t("delete_policy_group_desc")}
					confirmLabel={t("core:delete")}
					variant="destructive"
				/>

				<PolicyGroupDialog
					open={dialogOpen}
					mode={editingGroup ? "edit" : "create"}
					form={form}
					formError={formError}
					submitting={submitting}
					policies={policies}
					policiesTotal={policiesTotal}
					policiesLoading={policiesLoading}
					policiesLoadingMore={policiesLoadingMore}
					hasMorePolicies={hasMorePolicies}
					onOpenChange={handleDialogOpenChange}
					onSubmit={() => void submitForm()}
					onRefreshPolicies={reloadPolicies}
					onLoadMorePolicies={loadMorePolicies}
					onFieldChange={setField}
					onRuleFieldChange={setRuleField}
					onAddRule={addRule}
					onRemoveRule={removeRule}
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}
