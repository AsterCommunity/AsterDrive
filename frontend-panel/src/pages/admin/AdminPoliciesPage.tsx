import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { PoliciesTable } from "@/components/admin/admin-policies-page/PoliciesTable";
import { PolicyDialogs } from "@/components/admin/admin-policies-page/PolicyDialogs";
import { PROTECTED_POLICY_ID } from "@/components/admin/admin-policies-page/policyPresentation";
import {
	buildCreatePolicyPayload,
	buildPolicyTestPayload,
	buildUpdatePolicyPayload,
	emptyForm,
	getEndpointValidationMessage,
	getPolicyConnectionTestKey,
	getPolicyForm,
	hasConnectionFieldChanges,
	normalizePolicyForm,
	type PolicyFormData,
} from "@/components/admin/storagePolicyDialogShared";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useApiList } from "@/hooks/useApiList";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
import {
	loadAdminRemoteNodeLookup,
	readAdminRemoteNodeLookup,
} from "@/lib/adminRemoteNodeLookup";
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
import { adminPolicyService } from "@/services/adminService";
import { ApiError } from "@/services/http";
import type { AdminPolicySortBy } from "@/types/adminSort";
import type {
	DeletePolicyQuery,
	DriverType,
	RemoteNodeInfo,
	StoragePolicy,
} from "@/types/api";

const POLICY_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_POLICY_PAGE_SIZE = 20 as const;
const POLICY_SORT_BY_OPTIONS = [
	"id",
	"name",
	"driver_type",
	"endpoint",
	"bucket",
	"is_default",
	"created_at",
	"updated_at",
] as const satisfies readonly AdminPolicySortBy[];
const DEFAULT_POLICY_SORT_BY =
	"created_at" as const satisfies AdminPolicySortBy;
const DEFAULT_POLICY_SORT_ORDER = "desc" as const satisfies SortOrder;
const CREATE_LAST_STEP = 2;
const POLICY_UPLOAD_SESSION_BLOCKER_SUBCODE = "policy.upload_sessions_exist";

export default function AdminPoliciesPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("policies"));
	const [searchParams, setSearchParams] = useSearchParams();
	const [offset, setOffset] = useState(
		parseOffsetSearchParam(searchParams.get("offset")),
	);
	const [pageSize, setPageSize] = useState<
		(typeof POLICY_PAGE_SIZE_OPTIONS)[number]
	>(
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			POLICY_PAGE_SIZE_OPTIONS,
			DEFAULT_POLICY_PAGE_SIZE,
		),
	);
	const [sortBy, setSortBy] = useState<AdminPolicySortBy>(
		parseSortSearchParam(
			searchParams.get("sortBy"),
			POLICY_SORT_BY_OPTIONS,
			DEFAULT_POLICY_SORT_BY,
		),
	);
	const [sortOrder, setSortOrder] = useState<SortOrder>(
		parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_POLICY_SORT_ORDER,
		),
	);
	const {
		items: policies,
		setItems: setPolicies,
		total,
		setTotal,
		loading,
		reload,
	} = useApiList(
		() =>
			adminPolicyService.list({
				limit: pageSize,
				offset,
				sort_by: sortBy,
				sort_order: sortOrder,
			}),
		[offset, pageSize, sortBy, sortOrder],
	);
	const [dialogOpen, setDialogOpen] = useState(false);
	const [editingId, setEditingId] = useState<number | null>(null);
	const [editingPolicy, setEditingPolicy] = useState<StoragePolicy | null>(
		null,
	);
	const [remoteNodes, setRemoteNodes] = useState<RemoteNodeInfo[]>(
		() => readAdminRemoteNodeLookup() ?? [],
	);
	const [form, setForm] = useState<PolicyFormData>(emptyForm);
	const [submitting, setSubmitting] = useState(false);
	const [validatedConnectionKey, setValidatedConnectionKey] = useState<
		string | null
	>(null);
	const [createStep, setCreateStep] = useState(0);
	const [createStepTouched, setCreateStepTouched] = useState(false);
	const endpointValidationMessage = getEndpointValidationMessage(form, t);
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;
	const pageSizeOptions = POLICY_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));
	const remoteNodeNameById = new Map(
		remoteNodes.map((node) => [node.id, node.name] as const),
	);

	useEffect(() => {
		setSearchParams(
			buildOffsetPaginationSearchParams({
				offset,
				pageSize,
				defaultPageSize: DEFAULT_POLICY_PAGE_SIZE,
				extraParams: {
					sortBy: sortBy !== DEFAULT_POLICY_SORT_BY ? sortBy : undefined,
					sortOrder:
						sortOrder !== DEFAULT_POLICY_SORT_ORDER ? sortOrder : undefined,
				},
			}),
			{ replace: true },
		);
	}, [offset, pageSize, setSearchParams, sortBy, sortOrder]);

	useEffect(() => {
		let active = true;

		void loadAdminRemoteNodeLookup()
			.then((nodes) => {
				if (active) {
					setRemoteNodes(nodes);
				}
			})
			.catch((error) => {
				if (active) {
					handleApiError(error);
				}
			});

		return () => {
			active = false;
		};
	}, []);

	const refreshRemoteNodeLookup = async (options?: { force?: boolean }) => {
		try {
			setRemoteNodes(await loadAdminRemoteNodeLookup(options));
		} catch (error) {
			handleApiError(error);
		}
	};

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, POLICY_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
	};

	const handleSortChange = (
		nextSortBy: AdminPolicySortBy,
		nextOrder: SortOrder,
	) => {
		setSortBy(nextSortBy);
		setSortOrder(nextOrder);
		setOffset(0);
	};

	const finalizePolicyDelete = async () => {
		invalidateAdminPolicyLookup();
		if (policies.length === 1 && offset > 0) {
			setOffset(Math.max(0, offset - pageSize));
		} else {
			await reload();
		}
	};

	const handleDelete = async (id: number, options?: DeletePolicyQuery) => {
		if (id === PROTECTED_POLICY_ID) return;
		try {
			if (options) {
				await adminPolicyService.delete(id, options);
			} else {
				await adminPolicyService.delete(id);
			}
			await finalizePolicyDelete();
			toast.success(
				options?.force ? t("policy_force_deleted") : t("policy_deleted"),
			);
		} catch (error) {
			if (
				!options?.force &&
				error instanceof ApiError &&
				error.subcode === POLICY_UPLOAD_SESSION_BLOCKER_SUBCODE
			) {
				requestForceDeleteConfirm(id);
				return;
			}
			handleApiError(error);
		}
	};

	const {
		confirmId: deleteId,
		requestConfirm,
		dialogProps,
	} = useConfirmDialog(handleDelete);
	const {
		confirmId: forceDeleteId,
		requestConfirm: requestForceDeleteConfirm,
		dialogProps: forceDeleteDialogProps,
	} = useConfirmDialog<number>(async (id) => {
		await handleDelete(id, { force: true });
	});
	const {
		requestConfirm: requestSaveAnywayConfirm,
		dialogProps: saveConfirmDialogProps,
	} = useConfirmDialog<true>(async () => {
		await submitPolicy(true);
	});
	const requestDeleteConfirm = (id: number) => {
		if (id === PROTECTED_POLICY_ID) return;
		requestConfirm(id);
	};

	const resetDialogState = () => {
		saveConfirmDialogProps.onOpenChange(false);
		setValidatedConnectionKey(null);
		setCreateStep(0);
		setCreateStepTouched(false);
	};

	const openCreate = () => {
		setEditingId(null);
		setEditingPolicy(null);
		resetDialogState();
		setForm(emptyForm);
		void refreshRemoteNodeLookup();
		setDialogOpen(true);
	};

	const openEdit = (policy: StoragePolicy) => {
		setEditingId(policy.id);
		setEditingPolicy(policy);
		resetDialogState();
		setForm(getPolicyForm(policy));
		void refreshRemoteNodeLookup();
		setDialogOpen(true);
	};

	const handleDialogOpenChange = (open: boolean) => {
		setDialogOpen(open);
		if (!open) {
			resetDialogState();
		}
	};

	const setField = <K extends keyof PolicyFormData>(
		key: K,
		value: PolicyFormData[K],
	) => setForm((prev) => ({ ...prev, [key]: value }));

	const setDriverType = (driverType: DriverType) => {
		setValidatedConnectionKey(null);
		setCreateStepTouched(false);
		setForm((prev) => {
			if (driverType === "s3") {
				return {
					...prev,
					driver_type: driverType,
					remote_node_id: "",
				};
			}

			if (driverType === "remote") {
				return {
					...prev,
					driver_type: driverType,
					endpoint: "",
					bucket: "",
					access_key: "",
					secret_key: "",
					content_dedup: false,
					remote_download_strategy: "relay_stream",
					remote_upload_strategy: "relay_stream",
				};
			}

			return {
				...prev,
				driver_type: driverType,
				endpoint: "",
				bucket: "",
				access_key: "",
				secret_key: "",
				remote_node_id: "",
				remote_download_strategy: "relay_stream",
				remote_upload_strategy: "relay_stream",
				s3_upload_strategy: "relay_stream",
				s3_download_strategy: "relay_stream",
			};
		});
	};

	const syncNormalizedS3Form = () => {
		const normalizedForm = normalizePolicyForm(form);
		if (normalizedForm !== form) {
			setForm(normalizedForm);
		}
		return normalizedForm;
	};

	const runConnectionTest = async ({
		showSuccessToast = true,
		showFailureError = true,
	}: {
		showSuccessToast?: boolean;
		showFailureError?: boolean;
	} = {}) => {
		const currentForm = syncNormalizedS3Form();
		const shouldUseParamTest =
			editingId === null ||
			hasConnectionFieldChanges(currentForm, editingPolicy);

		try {
			if (shouldUseParamTest) {
				await adminPolicyService.testParams(
					buildPolicyTestPayload(currentForm),
				);
			} else {
				await adminPolicyService.testConnection(editingId);
			}

			if (
				currentForm.driver_type === "s3" ||
				currentForm.driver_type === "remote"
			) {
				setValidatedConnectionKey(getPolicyConnectionTestKey(currentForm));
			}
			if (showSuccessToast) {
				toast.success(t("connection_success"));
			}
			return true;
		} catch (e) {
			setValidatedConnectionKey(null);
			if (showFailureError) {
				handleApiError(e);
			}
			return false;
		}
	};

	const persistPolicy = async () => {
		try {
			const currentForm = syncNormalizedS3Form();
			if (editingId) {
				const updated = await adminPolicyService.update(
					editingId,
					buildUpdatePolicyPayload(currentForm),
				);
				invalidateAdminPolicyLookup();
				setPolicies((prev) =>
					prev.map((policy) => (policy.id === editingId ? updated : policy)),
				);
				toast.success(t("policy_updated"));
			} else {
				await adminPolicyService.create(buildCreatePolicyPayload(currentForm));
				invalidateAdminPolicyLookup();
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
				toast.success(t("policy_created"));
			}
			handleDialogOpenChange(false);
		} catch (e) {
			handleApiError(e);
		}
	};

	const shouldRunConnectionSaveTest = () => {
		if (form.driver_type !== "s3" && form.driver_type !== "remote") {
			return false;
		}

		if (editingId !== null && !hasConnectionFieldChanges(form, editingPolicy)) {
			return false;
		}

		return validatedConnectionKey !== getPolicyConnectionTestKey(form);
	};

	const submitPolicy = async (forceSave = false) => {
		if (submitting) {
			return;
		}

		setSubmitting(true);
		try {
			if (!forceSave && shouldRunConnectionSaveTest()) {
				const testPassed = await runConnectionTest({
					showSuccessToast: false,
					showFailureError: false,
				});
				if (!testPassed) {
					requestSaveAnywayConfirm(true);
					return;
				}
			}

			await persistPolicy();
		} finally {
			setSubmitting(false);
		}
	};

	const handleCreateBack = () => {
		setCreateStepTouched(false);
		setCreateStep((prev) => Math.max(0, prev - 1));
	};

	const handleCreateStepChange = (step: number) => {
		setCreateStepTouched(false);
		setCreateStep(Math.max(0, Math.min(CREATE_LAST_STEP, step)));
	};

	const handleCreateNext = () => {
		if (createStep >= CREATE_LAST_STEP) {
			return;
		}

		if (createStep === 0) {
			setCreateStep(1);
			return;
		}

		setCreateStepTouched(true);

		if (!form.name.trim()) {
			return;
		}

		if (form.driver_type === "s3" && !form.bucket.trim()) {
			return;
		}

		if (form.driver_type === "remote" && !form.remote_node_id) {
			return;
		}

		if (endpointValidationMessage) {
			return;
		}

		syncNormalizedS3Form();
		setCreateStepTouched(false);
		setCreateStep(CREATE_LAST_STEP);
	};

	const handleSubmit = () => {
		if (editingId === null && createStep < CREATE_LAST_STEP) {
			handleCreateNext();
			return;
		}
		void submitPolicy();
	};

	const deletePolicyName =
		deleteId !== null
			? (policies.find((policy) => policy.id === deleteId)?.name ?? "")
			: "";
	const forceDeletePolicyName =
		forceDeleteId !== null
			? (policies.find((policy) => policy.id === forceDeleteId)?.name ?? "")
			: "";
	const handleRefresh = async () => {
		try {
			const [policyPage, remoteNodeLookup] = await Promise.all([
				adminPolicyService.list({
					limit: pageSize,
					offset,
					sort_by: sortBy,
					sort_order: sortOrder,
				}),
				loadAdminRemoteNodeLookup({ force: true }),
			]);
			setPolicies(policyPage.items);
			setTotal(policyPage.total);
			setRemoteNodes(remoteNodeLookup);
			invalidateAdminPolicyLookup();
		} catch (error) {
			handleApiError(error);
		}
	};

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					title={t("policies")}
					description={t("policies_intro")}
					actions={
						<>
							<Button
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={openCreate}
							>
								<Icon name="Plus" className="mr-1 h-4 w-4" />
								{t("new_policy")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void handleRefresh()}
								disabled={loading}
							>
								<Icon
									name={loading ? "Spinner" : "ArrowsClockwise"}
									className={`mr-1 h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`}
								/>
								{t("core:refresh")}
							</Button>
						</>
					}
				/>

				<PoliciesTable
					loading={loading}
					onDeletePolicy={requestDeleteConfirm}
					onEditPolicy={openEdit}
					policies={policies}
					remoteNodeNameById={remoteNodeNameById}
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				/>

				<AdminOffsetPagination
					total={total}
					currentPage={currentPage}
					totalPages={totalPages}
					pageSize={String(pageSize)}
					pageSizeOptions={pageSizeOptions}
					onPageSizeChange={handlePageSizeChange}
					prevDisabled={prevPageDisabled}
					nextDisabled={nextPageDisabled}
					onPrevious={() => setOffset(Math.max(0, offset - pageSize))}
					onNext={() => setOffset(offset + pageSize)}
				/>

				<PolicyDialogs
					deleteDialogProps={dialogProps}
					deletePolicyName={deletePolicyName}
					forceDeleteDialogProps={forceDeleteDialogProps}
					forceDeletePolicyName={forceDeletePolicyName}
					dialogOpen={dialogOpen}
					editMode={editingId !== null}
					form={form}
					remoteNodes={remoteNodes}
					submitting={submitting}
					createStep={createStep}
					createStepTouched={createStepTouched}
					endpointValidationMessage={endpointValidationMessage}
					saveConfirmDialogProps={saveConfirmDialogProps}
					onDialogOpenChange={handleDialogOpenChange}
					onSubmit={handleSubmit}
					onRunConnectionTest={() => runConnectionTest()}
					onFieldChange={setField}
					onDriverTypeChange={setDriverType}
					onCreateBack={handleCreateBack}
					onCreateStepChange={handleCreateStepChange}
					onCreateNext={handleCreateNext}
					onSyncNormalizedS3Form={syncNormalizedS3Form}
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}
