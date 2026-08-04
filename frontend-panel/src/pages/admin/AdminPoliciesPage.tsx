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
	applyPolicyConnectorTransition,
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
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { getStorageConnectorDescriptor } from "@/lib/adminStorageDriverDescriptors";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { adminPolicyService } from "@/services/adminService";
import { useAuthStore } from "@/stores/authStore";
import { useSystemSetupStore } from "@/stores/systemSetupStore";
import type {
	StorageConnectorCredentialInfo,
	StoragePolicy,
	StoragePolicyCapacityInfo,
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
			return "storage_authorization_failed_invalid_state";
		case "provider_error":
			return "storage_authorization_failed_provider";
		case "token_exchange_failed":
			return "storage_authorization_failed_token_exchange";
		case "drive_resolution_failed":
			return "storage_authorization_failed_target_resolution";
		case "unsupported_provider":
			return "storage_authorization_failed_unsupported_provider";
		case "invalid_request":
			return "storage_authorization_failed_invalid_request";
		case "server_error":
			return "storage_authorization_failed_server";
		default:
			return "storage_authorization_failed";
	}
}

export type AdminPoliciesPageVariant = "admin" | "setup";

function useAdminPoliciesPageContent(variant: AdminPoliciesPageVariant) {
	const { t } = useTranslation("admin");
	const setupMode = variant === "setup";
	usePageTitle(setupMode ? t("auth:storage_setup_page_title") : t("policies"));
	const logout = useAuthStore((state) => state.logout);
	const refreshSetupState = useSystemSetupStore((state) => state.refresh);
	const [searchParams, setSearchParams] = useSearchParams();
	const policyList = useStoragePolicyListController();
	const migrationController = useStoragePolicyMigrationController();
	const [dialogOpen, setDialogOpen] = useState(setupMode);
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
		StorageConnectorCredentialInfo[]
	>([]);
	const [storageCredentialsLoading, setStorageCredentialsLoading] =
		useState(false);
	const storageCredentialsRequestSerial = useRef(0);
	const storageCredentialValidationRequestSerial = useRef(0);
	const consumedStorageAuthorizationSearchRef = useRef<string | null>(null);
	const [form, setForm] = useState<PolicyFormData>(() =>
		setupMode ? { ...emptyForm, is_default: true } : emptyForm,
	);
	const descriptorController = useStoragePolicyDescriptorController({
		dialogOpen,
		form,
		setForm,
		setupMode,
	});
	useEffect(() => {
		if (editingId !== null || !dialogOpen) {
			return;
		}
		const creatableDescriptors =
			descriptorController.creatableStorageDriverDescriptors;
		if (
			creatableDescriptors.length === 0 ||
			creatableDescriptors.some(
				(descriptor) => descriptor.connector_id === form.connector_id,
			)
		) {
			return;
		}

		const firstDescriptor = creatableDescriptors[0];
		setForm((current) => {
			const transitioned = applyPolicyConnectorTransition(
				current,
				firstDescriptor.connector_id,
				firstDescriptor,
			);
			return setupMode ? { ...transitioned, is_default: true } : transitioned;
		});
	}, [
		descriptorController.creatableStorageDriverDescriptors,
		dialogOpen,
		editingId,
		form.connector_id,
		setupMode,
	]);
	const [submitting, setSubmitting] = useState(false);

	currentEditingIdRef.current = editingId;
	const [saveAnywayConfirmOpen, setSaveAnywayConfirmOpen] = useState(false);
	const [createStep, setCreateStep] = useState(0);
	const [createStepTouched, setCreateStepTouched] = useState(false);
	const currentStorageDriverDescriptor =
		descriptorController.currentStorageDriverDescriptor;
	const endpointValidationMessage = getEndpointValidationMessage(
		form,
		(key) =>
			translateStorageConnectorMessage(
				t,
				currentStorageDriverDescriptor?.connector_id,
				key,
			),
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
		setStorageCredentials,
		storageCredentialValidationRequestSerial,
		storageDriverDescriptors: descriptorController.storageDriverDescriptors,
		syncNormalizedPolicyForm,
	});
	const editorController = useStoragePolicyEditorController({
		allowSaveWithoutConnectionTest: !setupMode,
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
		onPolicyCreated: setupMode
			? async () => {
					await refreshSetupState().catch(handleApiError);
				}
			: undefined,
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
		setForm(setupMode ? { ...emptyForm, is_default: true } : emptyForm);
		void descriptorController.refreshRemoteNodeLookup();
		setDialogOpen(true);
	};

	const loadStorageCredentials = useCallback(
		(policyId: number, connectorId: string) => {
			const descriptor = getStorageConnectorDescriptor(
				descriptorController.storageDriverDescriptors,
				connectorId,
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
		loadStorageCredentials(editingPolicy.id, editingPolicy.connector_id);
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
			toast.success(t("storage_authorization_completed"), {
				description: callback.policyId
					? t("storage_authorization_completed_policy", {
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
		if (setupMode && !open) return;
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
		setForm((prev) => {
			const next = applyPolicyFormFieldChange(prev, key, value);
			return setupMode ? { ...next, is_default: true } : next;
		});
	};

	function setConnectorId(connectorId: string) {
		setSaveAnywayConfirmOpen(false);
		actionController.resetActionState();
		setCreateStepTouched(false);
		setForm((prev) => {
			const nextDriverDescriptor = getStorageConnectorDescriptor(
				descriptorController.storageDriverDescriptors,
				connectorId,
			);
			const next = applyPolicyConnectorTransition(
				prev,
				connectorId,
				nextDriverDescriptor,
			);
			return setupMode ? { ...next, is_default: true } : next;
		});
	}

	function syncNormalizedPolicyForm() {
		const descriptor = getStorageConnectorDescriptor(
			descriptorController.storageDriverDescriptors,
			form.connector_id,
		);
		const normalizedForm = normalizePolicyForm(
			setupMode ? { ...form, is_default: true } : form,
			descriptor,
		);
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
		await Promise.all([
			policyList
				.reload()
				.then(() => {
					invalidateAdminPolicyLookup();
				})
				.catch(handleApiError),
			descriptorController.refreshLookups().catch(handleApiError),
		]);
	};
	const policyDialogs = (
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
				editingId !== null
					? descriptorController.storageDriverDescriptors
					: descriptorController.creatableStorageDriverDescriptors
			}
			storageDriverDescriptorsError={
				editingId !== null
					? descriptorController.storageDriverDescriptorsError
					: descriptorController.creatableStorageDriverDescriptorsError
			}
			storageDriverDescriptorsLoading={
				editingId !== null
					? descriptorController.storageDriverDescriptorsLoading
					: descriptorController.creatableStorageDriverDescriptorsLoading
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
			connectorActionConfirmId={actionController.connectorActionConfirmId}
			connectorActionSubmittingId={actionController.connectorActionSubmittingId}
			connectorActionValues={actionController.connectorActionValues}
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
			remoteStorageTargetsError={descriptorController.remoteStorageTargetsError}
			remoteStorageTargetsLoading={
				descriptorController.remoteStorageTargetsLoading
			}
			submitting={submitting}
			createStep={createStep}
			createStepTouched={createStepTouched}
			endpointValidationMessage={endpointValidationMessage}
			saveAnywayConfirmOpen={saveAnywayConfirmOpen}
			showStorageDialogCloseButton={!setupMode}
			forceDefaultPolicy={setupMode}
			storageDialogPresentation={setupMode ? "setup" : "dialog"}
			onStorageSetupLogout={setupMode ? () => void logout() : undefined}
			onCancelConnectorAction={actionController.cancelConnectorAction}
			onCancelSaveAnyway={editorController.cancelSaveAnyway}
			onConfirmSaveAnyway={() =>
				editorController.confirmSaveAnyway(actionController)
			}
			onConfirmConnectorAction={(actionId) => {
				setSaveAnywayConfirmOpen(false);
				void actionController.executeConnectorAction(actionId);
			}}
			onStartStorageAuthorization={actionController.startStorageAuthorization}
			onValidateStorageCredential={actionController.validateStorageCredential}
			onCreateRemoteStorageTarget={
				descriptorController.createRemoteStorageTargetForPolicy
			}
			onDialogOpenChange={handleDialogOpenChange}
			onConnectorActionValueChange={actionController.setConnectorActionValue}
			onSubmit={() => editorController.handleSubmit(actionController)}
			onRequestConnectorAction={(actionId) => {
				setSaveAnywayConfirmOpen(false);
				actionController.requestConnectorAction(actionId);
			}}
			onRunConnectionTest={() => actionController.runConnectionTest()}
			onFieldChange={setField}
			onConnectorIdChange={setConnectorId}
			onCreateBack={editorController.handleCreateBack}
			onCreateStepChange={editorController.handleCreateStepChange}
			onCreateNext={editorController.handleCreateNext}
		/>
	);

	if (setupMode) {
		return policyDialogs;
	}

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

				{policyDialogs}
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

export default function AdminPoliciesPage({
	variant = "admin",
}: {
	variant?: AdminPoliciesPageVariant;
}) {
	return useAdminPoliciesPageContent(variant);
}
