import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { PoliciesTable } from "@/components/admin/admin-policies-page/PoliciesTable";
import { PolicyDialogs } from "@/components/admin/admin-policies-page/PolicyDialogs";
import { StoragePolicyMigrationDialog } from "@/components/admin/admin-policies-page/StoragePolicyMigrationDialog";
import {
	getEndpointValidationMessage,
	normalizePolicyForm,
} from "@/components/admin/storage-policy-dialog/connectionNormalization";
import { supportsStorageCredentialLifecycle } from "@/components/admin/storage-policy-dialog/descriptorPredicates";
import {
	emptyForm,
	getPolicyForm,
	type PolicyFormData,
} from "@/components/admin/storage-policy-dialog/formTypes";
import {
	applyPolicyDriverTransition,
	applyPolicyFormFieldChange,
} from "@/components/admin/storage-policy-dialog/policyFormTransition";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { config } from "@/config/app";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
import { getStorageDriverDescriptor } from "@/lib/adminStorageDriverDescriptors";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { adminPolicyService } from "@/services/adminService";
import type {
	DriverType,
	StoragePolicy,
	StoragePolicyCapacityInfo,
	StoragePolicyCredentialInfo,
} from "@/types/api";
import { useStoragePolicyActionController } from "./admin-policies-page/useStoragePolicyActionController";
import { useStoragePolicyDescriptorController } from "./admin-policies-page/useStoragePolicyDescriptorController";
import { useStoragePolicyEditorController } from "./admin-policies-page/useStoragePolicyEditorController";
import { useStoragePolicyListController } from "./admin-policies-page/useStoragePolicyListController";
import { useStoragePolicyMigrationController } from "./admin-policies-page/useStoragePolicyMigrationController";

function getStorageAuthorizationCallbackUrl() {
	const apiBaseUrl = new URL(config.apiBaseUrl, window.location.origin);
	return new URL(
		"admin/policies/storage-authorization/callback",
		apiBaseUrl.href.endsWith("/") ? apiBaseUrl.href : `${apiBaseUrl.href}/`,
	).toString();
}

function consumeStorageAuthorizationSearchParams(
	searchParams: URLSearchParams,
) {
	const status = searchParams.get("storage_authorization");
	if (!status) {
		return null;
	}

	const nextSearchParams = new URLSearchParams(searchParams);
	const policyId = nextSearchParams.get("policy_id");
	const reason = nextSearchParams.get("reason");
	nextSearchParams.delete("storage_authorization");
	nextSearchParams.delete("policy_id");
	nextSearchParams.delete("reason");
	return {
		policyId,
		reason,
		status,
		nextSearchParams,
	};
}

function storageAuthorizationFailureI18nKey(reason: string | null) {
	switch (reason) {
		case "invalid_state":
			return "onedrive_authorization_failed_invalid_state";
		case "provider_error":
			return "onedrive_authorization_failed_provider";
		case "token_exchange_failed":
			return "onedrive_authorization_failed_token_exchange";
		case "drive_resolution_failed":
			return "onedrive_authorization_failed_drive_resolution";
		case "unsupported_provider":
			return "onedrive_authorization_failed_unsupported_provider";
		case "invalid_request":
			return "onedrive_authorization_failed_invalid_request";
		case "server_error":
			return "onedrive_authorization_failed_server";
		default:
			return "onedrive_authorization_failed";
	}
}

function useAdminPoliciesPageContent() {
	const { t } = useTranslation("admin");
	usePageTitle(t("policies"));
	const [searchParams, setSearchParams] = useSearchParams();
	const policyList = useStoragePolicyListController();
	const migrationController = useStoragePolicyMigrationController();
	const [dialogOpen, setDialogOpen] = useState(false);
	const [editingId, setEditingId] = useState<number | null>(null);
	const currentEditingIdRef = useRef<number | null>(null);
	const [editingPolicy, setEditingPolicy] = useState<StoragePolicy | null>(
		null,
	);
	const [policyCapacity, setPolicyCapacity] =
		useState<StoragePolicyCapacityInfo | null>(null);
	const [policyCapacityLoading, setPolicyCapacityLoading] = useState(false);
	const policyCapacityRequestSerial = useRef(0);
	const [storageCredentials, setStorageCredentials] = useState<
		StoragePolicyCredentialInfo[]
	>([]);
	const [storageCredentialsLoading, setStorageCredentialsLoading] =
		useState(false);
	const storageCredentialsRequestSerial = useRef(0);
	const storageCredentialValidationRequestSerial = useRef(0);
	const consumedStorageAuthorizationSearchRef = useRef<string | null>(null);
	const [form, setForm] = useState<PolicyFormData>(emptyForm);
	const descriptorController = useStoragePolicyDescriptorController({
		dialogOpen,
		form,
		setForm,
	});
	const [submitting, setSubmitting] = useState(false);

	currentEditingIdRef.current = editingId;
	const [saveAnywayConfirmOpen, setSaveAnywayConfirmOpen] = useState(false);
	const [createStep, setCreateStep] = useState(0);
	const [createStepTouched, setCreateStepTouched] = useState(false);
	const currentStorageDriverDescriptor =
		descriptorController.currentStorageDriverDescriptor;
	const endpointValidationMessage = getEndpointValidationMessage(
		form,
		t,
		currentStorageDriverDescriptor,
	);
	const storageAuthorizationRedirectUri = getStorageAuthorizationCallbackUrl();
	const remoteNodeNameById = new Map(
		descriptorController.remoteNodes.map(
			(node) => [node.id, node.name] as const,
		),
	);
	const loadPolicyCapacity = useCallback((policyId: number) => {
		const capacityRequestSerial = ++policyCapacityRequestSerial.current;
		setPolicyCapacityLoading(true);
		void adminPolicyService
			.getCapacity(policyId)
			.then((capacity) => {
				if (capacityRequestSerial === policyCapacityRequestSerial.current) {
					setPolicyCapacity(capacity);
				}
			})
			.catch((error) => {
				if (capacityRequestSerial === policyCapacityRequestSerial.current) {
					handleApiError(error);
					setPolicyCapacity(null);
				}
			})
			.finally(() => {
				if (capacityRequestSerial === policyCapacityRequestSerial.current) {
					setPolicyCapacityLoading(false);
				}
			});
	}, []);
	const actionController = useStoragePolicyActionController({
		currentEditingIdRef,
		currentStorageDriverDescriptor,
		editingId,
		editingPolicy,
		form,
		loadPolicyCapacity,
		onDriverSuggestionApply: setDriverType,
		setEditingId,
		setEditingPolicy,
		setForm,
		setPolicies: policyList.setPolicies,
		setPolicyCapacity,
		setStorageCredentials,
		storageCredentialValidationRequestSerial,
		storageDriverDescriptors: descriptorController.storageDriverDescriptors,
		syncNormalizedPolicyForm,
	});
	const editorController = useStoragePolicyEditorController({
		currentStorageDriverDescriptor,
		createStep,
		editingId,
		editingPolicy,
		endpointValidationMessage,
		form,
		list: {
			offset: policyList.offset,
			pageSize: policyList.pageSize,
			reload: policyList.reload,
			setOffset: policyList.setOffset,
			setPolicies: policyList.setPolicies,
			setTotal: policyList.setTotal,
			total: policyList.total,
		},
		loadPolicyCapacity,
		onCloseDialog: () => handleDialogOpenChange(false),
		setCreateStep,
		setCreateStepTouched,
		setEditingId,
		setEditingPolicy,
		setForm,
		setSaveAnywayConfirmOpen,
		setSubmitting,
		storageDriverDescriptors: descriptorController.storageDriverDescriptors,
		submitting,
		syncNormalizedPolicyForm,
	});

	const resetDialogState = useCallback(() => {
		policyCapacityRequestSerial.current += 1;
		storageCredentialsRequestSerial.current += 1;
		storageCredentialValidationRequestSerial.current += 1;
		setSaveAnywayConfirmOpen(false);
		setPolicyCapacity(null);
		setPolicyCapacityLoading(false);
		setStorageCredentials([]);
		setStorageCredentialsLoading(false);
		actionController.resetActionState();
		descriptorController.resetRemoteStorageTargets();
		setCreateStep(0);
		setCreateStepTouched(false);
	}, [actionController, descriptorController]);

	const openCreate = () => {
		setEditingId(null);
		setEditingPolicy(null);
		resetDialogState();
		setForm(emptyForm);
		void descriptorController.refreshRemoteNodeLookup();
		setDialogOpen(true);
	};

	const loadStorageCredentials = useCallback(
		(policyId: number, driverType: DriverType) => {
			const descriptor = getStorageDriverDescriptor(
				descriptorController.storageDriverDescriptors,
				driverType,
			);
			if (!supportsStorageCredentialLifecycle(descriptor)) {
				setStorageCredentials([]);
				setStorageCredentialsLoading(false);
				return;
			}

			const credentialsRequestSerial =
				++storageCredentialsRequestSerial.current;
			setStorageCredentialsLoading(true);
			void adminPolicyService
				.listStorageCredentials(policyId)
				.then((credentials) => {
					if (
						credentialsRequestSerial === storageCredentialsRequestSerial.current
					) {
						setStorageCredentials(credentials);
					}
				})
				.catch((error) => {
					if (
						credentialsRequestSerial === storageCredentialsRequestSerial.current
					) {
						handleApiError(error);
						setStorageCredentials([]);
					}
				})
				.finally(() => {
					if (
						credentialsRequestSerial === storageCredentialsRequestSerial.current
					) {
						setStorageCredentialsLoading(false);
					}
				});
		},
		[descriptorController.storageDriverDescriptors],
	);

	useEffect(() => {
		if (!editingPolicy) {
			return;
		}
		loadStorageCredentials(editingPolicy.id, editingPolicy.driver_type);
	}, [editingPolicy, loadStorageCredentials]);

	const openEdit = useCallback(
		(policy: StoragePolicy) => {
			setEditingId(policy.id);
			setEditingPolicy(policy);
			resetDialogState();
			setForm(getPolicyForm(policy));
			void descriptorController.refreshRemoteNodeLookup();
			loadPolicyCapacity(policy.id);
			setDialogOpen(true);
		},
		[descriptorController, loadPolicyCapacity, resetDialogState],
	);

	const openPolicyById = useCallback(
		async (policyId: number) => {
			const policy = await adminPolicyService.get(policyId);
			openEdit(policy);
			policyList.setPolicies((prev) => {
				const exists = prev.some((item) => item.id === policy.id);
				return exists
					? prev.map((item) => (item.id === policy.id ? policy : item))
					: prev;
			});
		},
		[openEdit, policyList],
	);

	useEffect(() => {
		const callback = consumeStorageAuthorizationSearchParams(searchParams);
		if (!callback) {
			consumedStorageAuthorizationSearchRef.current = null;
			return;
		}

		const callbackKey = searchParams.toString();
		if (consumedStorageAuthorizationSearchRef.current === callbackKey) {
			return;
		}
		consumedStorageAuthorizationSearchRef.current = callbackKey;

		setSearchParams(callback.nextSearchParams, { replace: true });
		if (callback.status === "success") {
			toast.success(t("onedrive_authorization_completed"), {
				description: callback.policyId
					? t("onedrive_authorization_completed_policy", {
							id: callback.policyId,
						})
					: undefined,
			});
			void policyList.reload().catch(handleApiError);
			const policyId = Number(callback.policyId);
			if (Number.isSafeInteger(policyId) && policyId > 0) {
				void openPolicyById(policyId).catch(handleApiError);
			}
			return;
		}

		toast.error(t(storageAuthorizationFailureI18nKey(callback.reason)));
	}, [openPolicyById, policyList, searchParams, setSearchParams, t]);

	const handleDialogOpenChange = (open: boolean) => {
		setDialogOpen(open);
		if (!open) {
			resetDialogState();
		}
	};

	const setField = <K extends keyof PolicyFormData>(
		key: K,
		value: PolicyFormData[K],
	) => {
		setSaveAnywayConfirmOpen(false);
		actionController.clearActionConfirms();
		setForm((prev) => applyPolicyFormFieldChange(prev, key, value));
	};

	function setDriverType(driverType: DriverType) {
		setSaveAnywayConfirmOpen(false);
		actionController.resetActionState();
		setCreateStepTouched(false);
		setForm((prev) => {
			const nextDriverDescriptor = getStorageDriverDescriptor(
				descriptorController.storageDriverDescriptors,
				driverType,
			);
			return applyPolicyDriverTransition(
				prev,
				driverType,
				nextDriverDescriptor,
			);
		});
	}

	function syncNormalizedPolicyForm() {
		const descriptor = getStorageDriverDescriptor(
			descriptorController.storageDriverDescriptors,
			form.driver_type,
		);
		const normalizedForm = normalizePolicyForm(form, descriptor);
		if (normalizedForm !== form) {
			setForm(normalizedForm);
		}
		return normalizedForm;
	}

	const deletePolicyName =
		policyList.deleteId !== null
			? (policyList.policies.find((policy) => policy.id === policyList.deleteId)
					?.name ?? "")
			: "";
	const forceDeletePolicyName =
		policyList.forceDeleteId !== null
			? (policyList.policies.find(
					(policy) => policy.id === policyList.forceDeleteId,
				)?.name ?? "")
			: "";
	const handleRefresh = async () => {
		try {
			const [policyPage] = await Promise.all([
				adminPolicyService.list({
					limit: policyList.pageSize,
					offset: policyList.offset,
					sort_by: policyList.sortBy,
					sort_order: policyList.sortOrder,
				}),
				descriptorController.refreshLookups(),
			]);
			policyList.setPolicies(policyPage.items);
			policyList.setTotal(policyPage.total);
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
								<Icon name="Plus" className="mr-1 size-4" />
								{t("new_policy")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void migrationController.openDialog()}
								disabled={policyList.total < 2}
							>
								<Icon name="ArrowsClockwise" className="mr-1 size-3.5" />
								{t("policy_migration_action")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void handleRefresh()}
								disabled={policyList.loading}
							>
								<Icon
									name={policyList.loading ? "Spinner" : "ArrowsClockwise"}
									className={`mr-1 size-3.5 ${policyList.loading ? "animate-spin" : ""}`}
								/>
								{t("core:refresh")}
							</Button>
						</>
					}
				/>

				<PoliciesTable
					loading={policyList.loading}
					deletingPolicyId={policyList.deletingPolicyId}
					onDeletePolicy={policyList.requestDeleteConfirm}
					onEditPolicy={openEdit}
					policies={policyList.policies}
					remoteNodeNameById={remoteNodeNameById}
					sortBy={policyList.sortBy}
					sortOrder={policyList.sortOrder}
					storageDriverDescriptors={
						descriptorController.storageDriverDescriptors
					}
					onSortChange={policyList.handleSortChange}
				/>

				<AdminOffsetPagination
					total={policyList.total}
					currentPage={policyList.currentPage}
					totalPages={policyList.totalPages}
					pageSize={String(policyList.pageSize)}
					pageSizeOptions={policyList.pageSizeOptions}
					onPageSizeChange={policyList.handlePageSizeChange}
					prevDisabled={policyList.prevPageDisabled}
					nextDisabled={policyList.nextPageDisabled}
					onPrevious={() =>
						policyList.setOffset((current) =>
							Math.max(0, current - policyList.pageSize),
						)
					}
					onNext={() =>
						policyList.setOffset((current) => current + policyList.pageSize)
					}
				/>

				<PolicyDialogs
					deleteDialogProps={policyList.deleteDialogProps}
					deletePolicyName={deletePolicyName}
					forceDeleteDialogProps={policyList.forceDeleteDialogProps}
					forceDeletePolicyName={forceDeletePolicyName}
					dialogOpen={dialogOpen}
					editMode={editingId !== null}
					form={form}
					storageDriverDescriptor={currentStorageDriverDescriptor}
					storageDriverDescriptors={
						descriptorController.storageDriverDescriptors
					}
					storageDriverDescriptorsError={
						descriptorController.storageDriverDescriptorsError
					}
					storageDriverDescriptorsLoading={
						descriptorController.storageDriverDescriptorsLoading
					}
					policyCapacity={policyCapacity}
					policyCapacityLoading={policyCapacityLoading}
					storageCredentials={storageCredentials}
					storageCredentialsLoading={storageCredentialsLoading}
					storageAuthorizationSubmitting={
						actionController.storageAuthorizationSubmitting
					}
					storageCredentialValidationSubmitting={
						actionController.storageCredentialValidationSubmitting
					}
					storageAuthorizationRedirectUri={storageAuthorizationRedirectUri}
					cosCorsConfirmOpen={actionController.cosCorsConfirmOpen}
					cosCorsSubmitting={actionController.cosCorsSubmitting}
					cosCorsUsesDraftValues={actionController.cosCorsUsesDraftValues}
					canConfigureTencentCosCors={
						actionController.canConfigureTencentCosCors
					}
					s3CompatibleDriverSuggestionTargetLabel={
						actionController.s3CompatibleDriverSuggestionTargetLabel
					}
					s3DriverPromotionBlocked={actionController.s3DriverPromotionBlocked}
					s3DriverPromotionConfirmOpen={
						actionController.s3DriverPromotionConfirmOpen
					}
					s3DriverPromotionSubmitting={
						actionController.s3DriverPromotionSubmitting
					}
					s3DriverPromotionTargetLabel={
						actionController.s3DriverPromotionTargetLabel
					}
					remoteNodes={descriptorController.remoteNodes}
					remoteStorageTargetDriverDescriptors={
						descriptorController.remoteStorageTargetDriverDescriptors
					}
					remoteStorageTargetDriverDescriptorsError={
						descriptorController.remoteStorageTargetDriverDescriptorsError
					}
					remoteStorageTargetDriverDescriptorsLoading={
						descriptorController.remoteStorageTargetDriverDescriptorsLoading
					}
					remoteStorageTargets={descriptorController.remoteStorageTargets}
					remoteStorageTargetsError={
						descriptorController.remoteStorageTargetsError
					}
					remoteStorageTargetsLoading={
						descriptorController.remoteStorageTargetsLoading
					}
					submitting={submitting}
					createStep={createStep}
					createStepTouched={createStepTouched}
					endpointValidationMessage={endpointValidationMessage}
					saveAnywayConfirmOpen={saveAnywayConfirmOpen}
					onApplyS3CompatibleDriverSuggestion={
						actionController.applyS3CompatibleDriverSuggestion
					}
					onCancelCosCorsConfigure={actionController.cancelCosCorsConfigure}
					onCancelSaveAnyway={editorController.cancelSaveAnyway}
					onCancelS3DriverPromotion={actionController.cancelS3DriverPromotion}
					onConfirmSaveAnyway={() =>
						editorController.confirmSaveAnyway(actionController)
					}
					onConfirmCosCorsConfigure={() => {
						setSaveAnywayConfirmOpen(false);
						actionController.requestOrConfirmCosCorsConfigure();
					}}
					onConfirmS3DriverPromotion={actionController.confirmS3DriverPromotion}
					onStartStorageAuthorization={
						actionController.startStorageAuthorization
					}
					onValidateStorageCredential={
						actionController.validateStorageCredential
					}
					onCreateRemoteStorageTarget={
						descriptorController.createRemoteStorageTargetForPolicy
					}
					onDialogOpenChange={handleDialogOpenChange}
					onSubmit={() => editorController.handleSubmit(actionController)}
					onRequestS3DriverPromotion={() => {
						setSaveAnywayConfirmOpen(false);
						actionController.requestS3DriverPromotion();
					}}
					onRunConnectionTest={() => actionController.runConnectionTest()}
					onFieldChange={setField}
					onDriverTypeChange={setDriverType}
					onCreateBack={editorController.handleCreateBack}
					onCreateStepChange={editorController.handleCreateStepChange}
					onCreateNext={editorController.handleCreateNext}
					onSyncNormalizedObjectStorageForm={syncNormalizedPolicyForm}
				/>
				<StoragePolicyMigrationDialog
					dryRun={migrationController.dryRun}
					dryRunLoading={migrationController.dryRunLoading}
					open={migrationController.open}
					policies={migrationController.policies}
					sourcePolicyId={migrationController.sourcePolicyId}
					targetPolicyId={migrationController.targetPolicyId}
					submitting={migrationController.submitting}
					onDryRun={() => void migrationController.dryRunMigration()}
					onOpenChange={migrationController.setOpen}
					onSourcePolicyChange={migrationController.handleSourcePolicyChange}
					onTargetPolicyChange={migrationController.handleTargetPolicyChange}
					onSubmit={() => void migrationController.createMigration()}
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}

export default function AdminPoliciesPage() {
	return useAdminPoliciesPageContent();
}
