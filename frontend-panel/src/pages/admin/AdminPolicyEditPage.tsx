import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useNavigate, useParams } from "react-router-dom";
import { StoragePolicyEditorActions } from "@/components/admin/storage-policy-editor/StoragePolicyEditorActions";
import { StoragePolicyEditorForm } from "@/components/admin/storage-policy-editor/StoragePolicyEditorForm";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { usePageTitle } from "@/hooks/usePageTitle";
import { adminPageEnterAnimationClass } from "@/lib/constants";
import { cn } from "@/lib/utils";
import { useStoragePolicyEditorWorkspace } from "./admin-policies-page/useStoragePolicyEditorWorkspace";

export default function AdminPolicyEditPage() {
	const { t } = useTranslation(["admin", "core"]);
	const navigate = useNavigate();
	const { policyId } = useParams<{ policyId?: string }>();
	const isCreate = policyId === "new";
	const parsedPolicyId = Number(policyId);
	const isValidRoute =
		isCreate || (Number.isSafeInteger(parsedPolicyId) && parsedPolicyId > 0);
	const editingPolicyId = isCreate ? null : parsedPolicyId;

	const backToList = useCallback(() => {
		navigate("/admin/policies", { viewTransition: false });
	}, [navigate]);

	const workspace = useStoragePolicyEditorWorkspace({
		variant: "admin",
		policyId: isValidRoute ? editingPolicyId : null,
		onExit: backToList,
		onEnterCredentialSetup: (createdPolicyId) =>
			navigate(`/admin/policies/${createdPolicyId}`, {
				replace: true,
				viewTransition: false,
			}),
	});

	usePageTitle(
		workspace.isCreateMode
			? t("create_policy")
			: (workspace.editingPolicy?.name ?? t("edit_policy")),
	);

	if (!isValidRoute) {
		return <Navigate to="/admin/policies" replace />;
	}

	if (workspace.policyNotFound) {
		return (
			<AdminLayout>
				<AdminPageShell>
					<div className="flex flex-col items-center gap-4 py-16 text-center">
						<p className="text-sm text-muted-foreground">
							{t("policy_not_found")}
						</p>
						<Button variant="outline" size="sm" onClick={backToList}>
							<Icon name="ArrowLeft" className="mr-1 size-4" />
							{t("policy_back_to_list")}
						</Button>
					</div>
				</AdminPageShell>
			</AdminLayout>
		);
	}

	const pageTitle = workspace.isCreateMode
		? t("create_policy")
		: (workspace.editingPolicy?.name ?? t("edit_policy"));

	return (
		<AdminLayout>
			<AdminPageShell>
				<form
					autoComplete="off"
					noValidate
					onSubmit={(event) => {
						event.preventDefault();
						workspace.submit();
					}}
				>
					<div className={cn(adminPageEnterAnimationClass(), "mb-2")}>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className="-ml-2 text-muted-foreground"
							onClick={backToList}
						>
							<Icon name="ArrowLeft" className="mr-1 size-4" />
							{t("policy_back_to_list")}
						</Button>
					</div>
					<AdminPageHeader
						className={cn(adminPageEnterAnimationClass(), "px-0 md:px-0")}
						title={pageTitle}
						description={t("policies_intro")}
						actions={
							<StoragePolicyEditorActions
								mode={workspace.isCreateMode ? "create" : "edit"}
								createStep={workspace.createStep}
								submitting={workspace.submitting}
								descriptor={
									workspace.descriptorController.currentStorageDriverDescriptor
								}
								onBack={workspace.editorController.handleCreateBack}
								onCancel={backToList}
								onRunConnectionTest={workspace.runConnectionTest}
							/>
						}
					/>
					<StoragePolicyEditorForm
						mode={workspace.isCreateMode ? "create" : "edit"}
						form={workspace.form}
						storageDriverDescriptor={
							workspace.descriptorController.currentStorageDriverDescriptor
						}
						storageDriverDescriptors={
							workspace.isCreateMode
								? workspace.descriptorController
										.creatableStorageDriverDescriptors
								: workspace.descriptorController.storageDriverDescriptors
						}
						storageDriverDescriptorsError={
							workspace.isCreateMode
								? workspace.descriptorController
										.creatableStorageDriverDescriptorsError
								: workspace.descriptorController.storageDriverDescriptorsError
						}
						storageDriverDescriptorsLoading={
							workspace.isCreateMode
								? workspace.descriptorController
										.creatableStorageDriverDescriptorsLoading
								: workspace.descriptorController.storageDriverDescriptorsLoading
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
							workspace.descriptorController.createRemoteStorageTargetForPolicy
						}
						onFieldChange={workspace.setField}
						onConnectorActionValueChange={workspace.setConnectorActionValue}
						onRequestConnectorAction={workspace.requestConnectorAction}
						onRequestConnectorPromotion={workspace.promotionController.request}
						onConnectorIdChange={workspace.setConnectorId}
						onCreateStepChange={
							workspace.editorController.handleCreateStepChange
						}
					/>
				</form>
			</AdminPageShell>
		</AdminLayout>
	);
}
