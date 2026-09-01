import { type Dispatch, type SetStateAction, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { PoliciesTable } from "@/components/admin/admin-policies-page/PoliciesTable";
import { StoragePolicyMigrationDialog } from "@/components/admin/admin-policies-page/StoragePolicyMigrationDialog";
import type { PolicyFormData } from "@/components/admin/storage-policy-editor/formTypes";
import { emptyForm } from "@/components/admin/storage-policy-editor/formTypes";
import { StoragePolicyEditorActions } from "@/components/admin/storage-policy-editor/StoragePolicyEditorActions";
import { StoragePolicyEditorForm } from "@/components/admin/storage-policy-editor/StoragePolicyEditorForm";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { useAuthStore } from "@/stores/authStore";
import { useSystemSetupStore } from "@/stores/systemSetupStore";
import type { StoragePolicy } from "@/types/api";
import { useStoragePolicyDescriptorController } from "./admin-policies-page/useStoragePolicyDescriptorController";
import { useStoragePolicyEditorWorkspace } from "./admin-policies-page/useStoragePolicyEditorWorkspace";
import { useStoragePolicyListController } from "./admin-policies-page/useStoragePolicyListController";
import { useStoragePolicyMigrationController } from "./admin-policies-page/useStoragePolicyMigrationController";

export type AdminPoliciesPageVariant = "admin" | "setup";

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

const noopSetPolicyForm: Dispatch<SetStateAction<PolicyFormData>> = () =>
	undefined;

function AdminPoliciesListPage() {
	const { t } = useTranslation("admin");
	const navigate = useNavigate();
	usePageTitle(t("policies"));
	const [searchParams, setSearchParams] = useSearchParams();
	const policyList = useStoragePolicyListController();
	const migrationController = useStoragePolicyMigrationController();
	// 列表页只需要驱动目录与远端节点 lookup（表格徽标、节点名列、手动刷新）；
	// 空表单常量与稳定 noop 使远端存储目标加载永不触发（内联箭头会导致
	// controller 内的 effect 因 setForm 引用漂移而反复重跑）。
	const descriptorController = useStoragePolicyDescriptorController({
		form: emptyForm,
		setForm: noopSetPolicyForm,
		setupMode: false,
	});
	const consumedStorageAuthorizationSearchRef = useRef<string | null>(null);

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
				navigate(`/admin/policies/${policyId}`, { viewTransition: false });
			}
			return;
		}

		toast.error(t(storageAuthorizationFailureI18nKey(callback.reason)));
	}, [navigate, policyList, searchParams, setSearchParams, t]);

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
								onClick={() =>
									navigate("/admin/policies/new", { viewTransition: false })
								}
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
					onEditPolicy={(policy: StoragePolicy) =>
						navigate(`/admin/policies/${policy.id}`, { viewTransition: false })
					}
					policies={policyList.policies}
					remoteNodeNameById={
						new Map(
							descriptorController.remoteNodes.map(
								(node) => [node.id, node.name] as const,
							),
						)
					}
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

				<ConfirmDialog
					{...policyList.deleteDialogProps}
					title={`${t("delete_policy")} "${deletePolicyName}"?`}
					description={t("delete_policy_desc")}
					confirmLabel={t("core:delete")}
					variant="destructive"
				/>
				<ConfirmDialog
					{...policyList.forceDeleteDialogProps}
					title={`${t("force_delete_policy")} "${forceDeletePolicyName}"?`}
					description={t("force_delete_policy_desc")}
					confirmLabel={t("force_delete_policy_confirm")}
					variant="destructive"
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

function StoragePolicySetupWorkspace() {
	const { t } = useTranslation(["admin", "auth", "core"]);
	const logout = useAuthStore((state) => state.logout);
	const refreshSetupState = useSystemSetupStore((state) => state.refresh);
	usePageTitle(t("auth:storage_setup_page_title"));

	const workspace = useStoragePolicyEditorWorkspace({
		variant: "setup",
		policyId: null,
		onExit: () => undefined,
		onPolicyCreated: async () => {
			await refreshSetupState().catch(handleApiError);
		},
	});

	return (
		<div className="min-h-svh bg-background">
			<div className="flex shrink-0 items-center justify-between gap-4 border-b border-border/70 px-6 py-4">
				<AsterDriveWordmark alt="AsterDrive" className="h-8 w-auto max-w-44" />
				<Button
					type="button"
					variant="outline"
					size="sm"
					onClick={() => void logout()}
				>
					{t("core:logout")}
				</Button>
			</div>
			<main className="mx-auto w-full max-w-4xl px-6 py-8">
				<form
					autoComplete="off"
					noValidate
					onSubmit={(event) => {
						event.preventDefault();
						workspace.submit();
					}}
				>
					<div className="flex flex-wrap items-end justify-between gap-4">
						<div className="space-y-2">
							<p className="text-xs font-semibold tracking-[0.18em] text-primary uppercase">
								{t("auth:storage_setup_eyebrow")}
							</p>
							<h1 className="font-heading text-2xl font-semibold tracking-tight sm:text-3xl">
								{t("auth:storage_setup_page_title")}
							</h1>
							<p className="text-sm leading-6 text-muted-foreground sm:text-base">
								{t("auth:storage_setup_page_desc")}
							</p>
						</div>
						<div className="flex items-center gap-2">
							<StoragePolicyEditorActions
								mode="create"
								createStep={workspace.createStep}
								submitting={workspace.submitting}
								descriptor={
									workspace.descriptorController.currentStorageDriverDescriptor
								}
								onBack={workspace.editorController.handleCreateBack}
								onRunConnectionTest={workspace.runConnectionTest}
							/>
						</div>
					</div>
					<div className="mt-8">
						<StoragePolicyEditorForm
							mode="create"
							setup
							forceDefaultPolicy
							form={workspace.form}
							storageDriverDescriptor={
								workspace.descriptorController.currentStorageDriverDescriptor
							}
							storageDriverDescriptors={
								workspace.descriptorController.creatableStorageDriverDescriptors
							}
							storageDriverDescriptorsError={
								workspace.descriptorController
									.creatableStorageDriverDescriptorsError
							}
							storageDriverDescriptorsLoading={
								workspace.descriptorController
									.creatableStorageDriverDescriptorsLoading
							}
							policyCapacity={workspace.policyCapacity}
							policyCapacityLoading={workspace.policyCapacityLoading}
							storageCredentials={workspace.credentialController.credentials}
							storageCredentialsLoading={workspace.credentialController.loading}
							storageAuthorizationSubmitting={
								workspace.credentialController.authorizationSubmitting
							}
							storageCredentialValidationSubmitting={
								workspace.credentialController.validationSubmitting
							}
							storageAuthorizationRedirectUri={
								workspace.storageAuthorizationRedirectUri
							}
							connectorActionConfirmId={
								workspace.actionController.connectorActionConfirmId
							}
							connectorActionSubmittingId={
								workspace.actionController.connectorActionSubmittingId
							}
							connectorActionValues={
								workspace.actionController.connectorActionValues
							}
							connectorPromotionBlocked={workspace.promotionController.blocked}
							connectorPromotionCandidates={
								workspace.promotionController.candidates
							}
							connectorPromotionConfirmKey={
								workspace.promotionController.confirmKey
							}
							connectorPromotionSubmittingKey={
								workspace.promotionController.submittingKey
							}
							remoteNodes={workspace.descriptorController.remoteNodes}
							remoteStorageTargetDriverDescriptors={
								workspace.descriptorController
									.remoteStorageTargetDriverDescriptors
							}
							remoteStorageTargetDriverDescriptorsError={
								workspace.descriptorController
									.remoteStorageTargetDriverDescriptorsError
							}
							remoteStorageTargetDriverDescriptorsLoading={
								workspace.descriptorController
									.remoteStorageTargetDriverDescriptorsLoading
							}
							remoteStorageTargets={
								workspace.descriptorController.remoteStorageTargets
							}
							remoteStorageTargetsError={
								workspace.descriptorController.remoteStorageTargetsError
							}
							remoteStorageTargetsLoading={
								workspace.descriptorController.remoteStorageTargetsLoading
							}
							createStep={workspace.createStep}
							createStepTouched={workspace.createStepTouched}
							endpointValidationMessage={workspace.endpointValidationMessage}
							saveAnywayConfirmOpen={workspace.saveAnywayConfirmOpen}
							onCancelConnectorAction={
								workspace.actionController.cancelConnectorAction
							}
							onApplyDraftConnectorPromotion={
								workspace.promotionController.applyDraft
							}
							onCancelConnectorPromotion={workspace.promotionController.cancel}
							onCancelSaveAnyway={workspace.editorController.cancelSaveAnyway}
							onConfirmSaveAnyway={workspace.confirmSaveAnyway}
							onConfirmConnectorAction={workspace.confirmConnectorAction}
							onConfirmConnectorPromotion={(candidate) =>
								void workspace.promotionController.confirm(candidate)
							}
							onStartStorageAuthorization={
								workspace.credentialController.startAuthorization
							}
							onValidateStorageCredential={
								workspace.credentialController.validate
							}
							onCreateRemoteStorageTarget={
								workspace.descriptorController
									.createRemoteStorageTargetForPolicy
							}
							onFieldChange={workspace.setField}
							onConnectorActionValueChange={workspace.setConnectorActionValue}
							onRequestConnectorAction={workspace.requestConnectorAction}
							onRequestConnectorPromotion={
								workspace.promotionController.request
							}
							onConnectorIdChange={workspace.setConnectorId}
							onCreateStepChange={
								workspace.editorController.handleCreateStepChange
							}
						/>
					</div>
				</form>
			</main>
		</div>
	);
}

export default function AdminPoliciesPage({
	variant = "admin",
}: {
	variant?: AdminPoliciesPageVariant;
}) {
	return variant === "setup" ? (
		<StoragePolicySetupWorkspace />
	) : (
		<AdminPoliciesListPage />
	);
}
