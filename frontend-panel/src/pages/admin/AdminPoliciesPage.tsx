import {
	type RefObject,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import {
	Navigate,
	useBlocker,
	useNavigate,
	useSearchParams,
} from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { PoliciesTable } from "@/components/admin/admin-policies-page/PoliciesTable";
import { PolicyDialogs } from "@/components/admin/admin-policies-page/PolicyDialogs";
import { StoragePolicyMigrationDialog } from "@/components/admin/admin-policies-page/StoragePolicyMigrationDialog";
import { StoragePolicyRecoveryDialog } from "@/components/admin/admin-policies-page/StoragePolicyRecoveryDialog";
import {
	getEndpointValidationMessage,
	normalizePolicyForm,
} from "@/components/admin/storage-policy-dialog/connectionNormalization";
import {
	emptyForm,
	getPolicyForm,
	type PolicyFormData,
} from "@/components/admin/storage-policy-dialog/formTypes";
import {
	applyPolicyConnectorTransition,
	applyPolicyFormFieldChange,
} from "@/components/admin/storage-policy-dialog/policyFormTransition";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
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
	StorageConnectorFieldValue,
	StoragePolicy,
	StoragePolicyCapacityInfo,
} from "@/types/api";
import { useStoragePolicyActionController } from "./admin-policies-page/useStoragePolicyActionController";
import { useStoragePolicyCredentialController } from "./admin-policies-page/useStoragePolicyCredentialController";
import { useStoragePolicyDescriptorController } from "./admin-policies-page/useStoragePolicyDescriptorController";
import { useStoragePolicyEditorController } from "./admin-policies-page/useStoragePolicyEditorController";
import { useStoragePolicyListController } from "./admin-policies-page/useStoragePolicyListController";
import { useStoragePolicyMigrationController } from "./admin-policies-page/useStoragePolicyMigrationController";
import { useStoragePolicyPromotionController } from "./admin-policies-page/useStoragePolicyPromotionController";
import { useStoragePolicyRecoveryController } from "./admin-policies-page/useStoragePolicyRecoveryController";

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

export type AdminPoliciesPageVariant = "admin" | "create" | "detail" | "setup";

function useAdminPoliciesPageContent(
	variant: AdminPoliciesPageVariant,
	detailPolicyId?: number,
) {
	const { t } = useTranslation("admin");
	const setupMode = variant === "setup";
	const createMode = variant === "create";
	const detailMode = variant === "detail";
	const navigate = useNavigate();
	const logout = useAuthStore((state) => state.logout);
	const refreshSetupState = useSystemSetupStore((state) => state.refresh);
	const [searchParams, setSearchParams] = useSearchParams();
	const recoveryController = useStoragePolicyRecoveryController();
	const policyList = useStoragePolicyListController({
		onBlobReferencesBlocked: recoveryController.openForPolicy,
	});
	const migrationController = useStoragePolicyMigrationController();
	const [dialogOpen, setDialogOpen] = useState(
		setupMode || createMode || detailMode,
	);
	const allowCreateNavigationRef = useRef(false);
	const [editingId, setEditingId] = useState<number | null>(
		detailMode ? (detailPolicyId ?? null) : null,
	);
	const [editingPolicy, setEditingPolicy] = useState<StoragePolicy | null>(
		null,
	);
	usePageTitle(
		setupMode
			? t("auth:storage_setup_page_title")
			: createMode
				? t("create_policy")
				: detailMode
					? (editingPolicy?.name ?? t("edit_policy"))
					: t("policies"),
	);
	const [detailLoading, setDetailLoading] = useState(detailMode);
	const [detailNotFound, setDetailNotFound] = useState(false);
	const [policyCapacity, setPolicyCapacity] =
		useState<StoragePolicyCapacityInfo | null>(null);
	const [policyCapacityLoading, setPolicyCapacityLoading] = useState(false);
	const policyCapacityRequestSerial = useRef(0);
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
		currentStorageDriverDescriptor,
		editingId,
		editingPolicy,
		storageDriverDescriptors: descriptorController.storageDriverDescriptors,
		syncNormalizedPolicyForm,
	});
	const credentialController = useStoragePolicyCredentialController({
		currentStorageDriverDescriptor,
		dialogOpen,
		editingPolicy,
		form,
		loadPolicyCapacity,
	});
	const setConnectorActionValue = useCallback(
		(
			actionId: string,
			fieldName: string,
			value: StorageConnectorFieldValue | undefined,
		) => {
			actionController.setConnectorActionValue(actionId, fieldName, value);
			const action = currentStorageDriverDescriptor?.actions.find(
				(candidate) => candidate.action_id === actionId,
			);
			const controlsRemoteTargets = action?.fields?.some(
				(field) =>
					field.select?.data_source === "remote_storage_targets" &&
					field.select.depends_on === fieldName,
			);
			if (!controlsRemoteTargets) {
				return;
			}
			if (
				typeof value === "number" &&
				Number.isSafeInteger(value) &&
				value > 0
			) {
				void descriptorController.loadRemoteStorageTargetsForPolicy(value, {
					showErrorToast: false,
					syncPolicySelection: false,
				});
			} else {
				descriptorController.resetRemoteStorageTargets();
			}
		},
		[actionController, currentStorageDriverDescriptor, descriptorController],
	);
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
			: createMode
				? (created) => {
						allowCreateNavigationRef.current = true;
						navigate(`/admin/policies/${created.id}`, {
							replace: true,
							viewTransition: false,
						});
						return true;
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
	const promotionController = useStoragePolicyPromotionController({
		currentDescriptor: currentStorageDriverDescriptor,
		editingId,
		editingPolicy,
		form,
		loadPolicyCapacity,
		onDraftApplied: () => {
			actionController.resetActionState();
			credentialController.reset();
			setSaveAnywayConfirmOpen(false);
			setCreateStepTouched(false);
		},
		onPromoted: () => {
			actionController.resetActionState();
			credentialController.reset();
			setSaveAnywayConfirmOpen(false);
		},
		setEditingPolicy,
		setForm,
		setPolicies: policyList.setPolicies,
		storageDriverDescriptors:
			editingId !== null
				? descriptorController.storageDriverDescriptors
				: descriptorController.creatableStorageDriverDescriptors,
	});

	useEffect(() => {
		if (!detailMode || detailPolicyId == null) return;

		let cancelled = false;
		setDetailLoading(true);
		setDetailNotFound(false);
		setEditingId(detailPolicyId);
		setDialogOpen(true);
		void adminPolicyService
			.get(detailPolicyId)
			.then((policy) => {
				if (cancelled) return;
				setEditingPolicy(policy);
				setForm(getPolicyForm(policy));
				loadPolicyCapacity(policy.id);
				void descriptorController.refreshRemoteNodeLookup();
			})
			.catch(() => {
				if (!cancelled) setDetailNotFound(true);
			})
			.finally(() => {
				if (!cancelled) setDetailLoading(false);
			});

		return () => {
			cancelled = true;
		};
	}, [
		detailMode,
		detailPolicyId,
		descriptorController.refreshRemoteNodeLookup,
		loadPolicyCapacity,
	]);

	const resetDialogState = useCallback(() => {
		policyCapacityRequestSerial.current += 1;
		setSaveAnywayConfirmOpen(false);
		setPolicyCapacity(null);
		setPolicyCapacityLoading(false);
		credentialController.reset();
		actionController.resetActionState();
		promotionController.reset();
		descriptorController.resetRemoteStorageTargets();
		setCreateStep(0);
		setCreateStepTouched(false);
	}, [
		actionController,
		credentialController,
		descriptorController,
		promotionController,
	]);

	const openCreate = () => {
		if (!setupMode && !createMode) {
			navigate("/admin/policies/new", { viewTransition: false });
			return;
		}
		setEditingId(null);
		setEditingPolicy(null);
		resetDialogState();
		setForm(setupMode ? { ...emptyForm, is_default: true } : emptyForm);
		void descriptorController.refreshRemoteNodeLookup();
		setDialogOpen(true);
	};

	const openEdit = useCallback(
		(policy: StoragePolicy) => {
			if (!setupMode && !detailMode) {
				navigate(`/admin/policies/${policy.id}`, { viewTransition: false });
				return;
			}
			setEditingId(policy.id);
			setEditingPolicy(policy);
			resetDialogState();
			setForm(getPolicyForm(policy));
			void descriptorController.refreshRemoteNodeLookup();
			loadPolicyCapacity(policy.id);
			setDialogOpen(true);
		},
		[
			descriptorController,
			detailMode,
			loadPolicyCapacity,
			navigate,
			resetDialogState,
			setupMode,
		],
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
				if (detailMode && policyId === detailPolicyId) {
					void openPolicyById(policyId).catch(handleApiError);
				} else {
					navigate(`/admin/policies/${policyId}`, {
						viewTransition: false,
					});
				}
			}
			return;
		}

		toast.error(t(storageAuthorizationFailureI18nKey(callback.reason)));
	}, [
		detailMode,
		detailPolicyId,
		navigate,
		openPolicyById,
		policyList,
		searchParams,
		setSearchParams,
		t,
	]);

	const handleDialogOpenChange = (open: boolean) => {
		if (setupMode && !open) return;
		if (createMode && !open) {
			navigate("/admin/policies", { viewTransition: false });
			return;
		}
		if (detailMode && !open) {
			navigate("/admin/policies", { viewTransition: false });
			return;
		}
		setDialogOpen(open);
		if (!open) {
			resetDialogState();
		}
	};

	const setField = <K extends keyof PolicyFormData>(
		key: K,
		value: PolicyFormData[K],
	) => {
		actionController.clearConnectionFieldErrors();
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
			storageCredentials={credentialController.credentials}
			storageCredentialsLoading={credentialController.loading}
			storageAuthorizationSubmitting={
				credentialController.authorizationSubmitting
			}
			storageCredentialValidationSubmitting={
				credentialController.validationSubmitting
			}
			storageAuthorizationRedirectUri={storageAuthorizationRedirectUri}
			connectorActionConfirmId={actionController.connectorActionConfirmId}
			connectorActionSubmittingId={actionController.connectorActionSubmittingId}
			connectorActionValues={actionController.connectorActionValues}
			connectorPromotionBlocked={promotionController.blocked}
			connectorPromotionCandidates={promotionController.candidates}
			connectorPromotionConfirmKey={promotionController.confirmKey}
			connectorPromotionSubmittingKey={promotionController.submittingKey}
			remoteNodes={descriptorController.remoteNodes}
			remoteStorageTargetConnectorDescriptors={
				descriptorController.remoteStorageTargetConnectorDescriptors
			}
			remoteStorageTargetConnectorDescriptorsError={
				descriptorController.remoteStorageTargetConnectorDescriptorsError
			}
			remoteStorageTargetConnectorDescriptorsLoading={
				descriptorController.remoteStorageTargetConnectorDescriptorsLoading
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
			connectionFieldErrors={actionController.connectionFieldErrors}
			saveAnywayConfirmOpen={saveAnywayConfirmOpen}
			forceDefaultPolicy={setupMode}
			storageDialogPresentation={
				setupMode ? "setup" : createMode || detailMode ? "page" : undefined
			}
			storageDialogPageBackLabel={t("back_to_policies")}
			onStorageSetupLogout={setupMode ? () => void logout() : undefined}
			onCancelConnectorAction={actionController.cancelConnectorAction}
			onApplyDraftConnectorPromotion={promotionController.applyDraft}
			onCancelConnectorPromotion={promotionController.cancel}
			onCancelSaveAnyway={editorController.cancelSaveAnyway}
			onConfirmSaveAnyway={() =>
				editorController.confirmSaveAnyway(actionController)
			}
			onConfirmConnectorAction={(actionId) => {
				setSaveAnywayConfirmOpen(false);
				void actionController.executeConnectorAction(actionId);
			}}
			onConfirmConnectorPromotion={(candidate) => {
				void promotionController.confirm(candidate);
			}}
			onStartStorageAuthorization={credentialController.startAuthorization}
			onValidateStorageCredential={credentialController.validate}
			onCreateRemoteStorageTarget={
				descriptorController.createRemoteStorageTargetForPolicy
			}
			onDialogOpenChange={handleDialogOpenChange}
			onConnectorActionValueChange={setConnectorActionValue}
			onSubmit={() => editorController.handleSubmit(actionController)}
			onRequestConnectorAction={(actionId) => {
				setSaveAnywayConfirmOpen(false);
				actionController.requestConnectorAction(actionId);
			}}
			onRequestConnectorPromotion={promotionController.request}
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
	if (createMode) {
		const hasCreateDraft = createStep > 0 || form.name.trim().length > 0;
		return (
			<AdminLayout>
				<AdminPageShell>
					{policyDialogs}
					<PolicyCreateNavigationGuard
						allowNavigationRef={allowCreateNavigationRef}
						dirty={hasCreateDraft}
					/>
				</AdminPageShell>
			</AdminLayout>
		);
	}
	if (detailMode) {
		if (detailPolicyId == null) {
			return <Navigate to="/admin/policies" replace />;
		}
		return (
			<AdminLayout>
				<AdminPageShell>
					{detailLoading ? (
						<div className="flex items-center justify-center gap-2 py-16 text-sm text-muted-foreground">
							<Icon name="Spinner" className="size-4 animate-spin" />
							{t("core:loading")}
						</div>
					) : detailNotFound || !editingPolicy ? (
						<div className="flex flex-col items-center gap-4 py-16 text-center">
							<p className="text-sm text-muted-foreground">
								{t("policy_not_found")}
							</p>
							<Button
								variant="outline"
								onClick={() => navigate("/admin/policies")}
							>
								<Icon name="ArrowLeft" className="mr-1 size-4" />
								{t("back_to_policies")}
							</Button>
						</div>
					) : (
						policyDialogs
					)}
				</AdminPageShell>
			</AdminLayout>
		);
	}

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					className="px-0 md:px-0"
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
				<StoragePolicyRecoveryDialog
					confirmation={recoveryController.confirmation}
					loading={recoveryController.loading}
					open={recoveryController.open}
					policies={recoveryController.policies}
					policy={recoveryController.policy}
					probe={recoveryController.probe}
					purgePreview={recoveryController.purgePreview}
					reason={recoveryController.reason}
					submitting={recoveryController.submitting}
					targetPolicyId={recoveryController.targetPolicyId}
					onConfirmationChange={recoveryController.setConfirmation}
					onOpenChange={recoveryController.setOpen}
					onPreviewForcedPurge={recoveryController.previewForcedPurge}
					onReasonChange={recoveryController.setReason}
					onRetryProbe={recoveryController.retryProbe}
					onStartForcedPurge={recoveryController.startForcedPurge}
					onStartRecovery={recoveryController.startRecovery}
					onTargetPolicyChange={recoveryController.setTargetPolicyId}
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}

function PolicyCreateNavigationGuard({
	allowNavigationRef,
	dirty,
}: {
	allowNavigationRef: RefObject<boolean>;
	dirty: boolean;
}) {
	const { t } = useTranslation("admin");
	const blocker = useBlocker(() => dirty && !allowNavigationRef.current);
	const resetTimerRef = useRef<number | null>(null);

	useEffect(() => {
		return () => {
			if (resetTimerRef.current !== null) {
				window.clearTimeout(resetTimerRef.current);
			}
		};
	}, []);

	useEffect(() => {
		if (!dirty || allowNavigationRef.current) return;
		const handleBeforeUnload = (event: BeforeUnloadEvent) => {
			event.preventDefault();
		};
		window.addEventListener("beforeunload", handleBeforeUnload);
		return () => window.removeEventListener("beforeunload", handleBeforeUnload);
	}, [allowNavigationRef, dirty]);

	return (
		<ConfirmDialog
			open={blocker.state === "blocked"}
			onOpenChange={(open) => {
				if (open || blocker.state !== "blocked") return;
				if (resetTimerRef.current !== null) {
					window.clearTimeout(resetTimerRef.current);
				}
				resetTimerRef.current = window.setTimeout(() => {
					resetTimerRef.current = null;
					if (!allowNavigationRef.current && blocker.state === "blocked") {
						blocker.reset();
					}
				}, 0);
			}}
			title={t("policy_create_discard_title")}
			description={t("policy_create_discard_desc")}
			confirmLabel={t("policy_create_discard_confirm")}
			variant="destructive"
			onConfirm={() => {
				allowNavigationRef.current = true;
				if (blocker.state === "blocked") blocker.proceed();
			}}
		/>
	);
}

export default function AdminPoliciesPage({
	variant = "admin",
	detailPolicyId,
}: {
	variant?: AdminPoliciesPageVariant;
	detailPolicyId?: number;
}) {
	return useAdminPoliciesPageContent(variant, detailPolicyId);
}
