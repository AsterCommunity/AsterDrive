import { useTranslation } from "react-i18next";
import { StoragePolicyDialog } from "@/components/admin/StoragePolicyDialog";
import type { PolicyFormData } from "@/components/admin/storage-policy-dialog/formTypes";
import type { StorageConnectorActionValues } from "@/components/admin/storage-policy-dialog/StorageConnectorActionsPanel";
import type { ConfirmDialogProps } from "@/components/common/ConfirmDialog";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteNodeInfo,
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetInfo,
	StorageConnectorCredentialInfo,
	StorageConnectorDescriptor,
	StorageConnectorFieldValue,
	StoragePolicyCapacityInfo,
} from "@/types/api";

interface PolicyDialogsProps {
	createStep: number;
	createStepTouched: boolean;
	deleteDialogProps: Pick<
		ConfirmDialogProps,
		"onConfirm" | "onOpenChange" | "open"
	>;
	deletePolicyName: string;
	forceDeleteDialogProps: Pick<
		ConfirmDialogProps,
		"onConfirm" | "onOpenChange" | "open"
	>;
	forceDeletePolicyName: string;
	dialogOpen: boolean;
	editMode: boolean;
	endpointValidationMessage: string | null;
	form: PolicyFormData;
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	storageDriverDescriptors: StorageConnectorDescriptor[];
	storageDriverDescriptorsError: string | null;
	storageDriverDescriptorsLoading: boolean;
	policyCapacity: StoragePolicyCapacityInfo | null;
	policyCapacityLoading: boolean;
	storageCredentials: StorageConnectorCredentialInfo[];
	storageCredentialsLoading: boolean;
	storageAuthorizationSubmitting: boolean;
	storageCredentialValidationSubmitting: boolean;
	storageAuthorizationRedirectUri: string;
	connectorActionConfirmId: string | null;
	connectorActionSubmittingId: string | null;
	connectorActionValues: StorageConnectorActionValues;
	remoteNodes: RemoteNodeInfo[];
	remoteStorageTargetDriverDescriptors: RemoteStorageTargetDriverDescriptor[];
	remoteStorageTargetDriverDescriptorsError: string | null;
	remoteStorageTargetDriverDescriptorsLoading: boolean;
	remoteStorageTargets: RemoteStorageTargetInfo[];
	remoteStorageTargetsError: string | null;
	remoteStorageTargetsLoading: boolean;
	saveAnywayConfirmOpen: boolean;
	submitting: boolean;
	showStorageDialogCloseButton?: boolean;
	forceDefaultPolicy?: boolean;
	storageDialogPresentation?: "dialog" | "setup";
	onStorageSetupLogout?: () => void;
	onCancelConnectorAction: () => void;
	onCancelSaveAnyway: () => void;
	onConfirmSaveAnyway: () => void;
	onConfirmConnectorAction: (actionId: string) => void;
	onStartStorageAuthorization: () => void;
	onValidateStorageCredential: () => void;
	onCreateRemoteStorageTarget: (
		payload: RemoteCreateStorageTargetRequest,
	) => Promise<void>;
	onCreateBack: () => void;
	onCreateNext: () => void;
	onCreateStepChange: (step: number) => void;
	onDialogOpenChange: (open: boolean) => void;
	onConnectorIdChange: (connectorId: string) => void;
	onFieldChange: <K extends keyof PolicyFormData>(
		key: K,
		value: PolicyFormData[K],
	) => void;
	onConnectorActionValueChange: (
		actionId: string,
		fieldName: string,
		value: StorageConnectorFieldValue | undefined,
	) => void;
	onRequestConnectorAction: (actionId: string) => void;
	onRunConnectionTest: () => Promise<boolean>;
	onSubmit: () => void;
}

export function PolicyDialogs({
	createStep,
	createStepTouched,
	deleteDialogProps,
	deletePolicyName,
	forceDeleteDialogProps,
	forceDeletePolicyName,
	dialogOpen,
	editMode,
	endpointValidationMessage,
	form,
	storageDriverDescriptor,
	storageDriverDescriptors,
	storageDriverDescriptorsError,
	storageDriverDescriptorsLoading,
	policyCapacity,
	policyCapacityLoading,
	storageCredentials,
	storageCredentialsLoading,
	storageAuthorizationSubmitting,
	storageCredentialValidationSubmitting,
	storageAuthorizationRedirectUri,
	connectorActionConfirmId,
	connectorActionSubmittingId,
	connectorActionValues,
	remoteNodes,
	remoteStorageTargetDriverDescriptors,
	remoteStorageTargetDriverDescriptorsError,
	remoteStorageTargetDriverDescriptorsLoading,
	remoteStorageTargets,
	remoteStorageTargetsError,
	remoteStorageTargetsLoading,
	saveAnywayConfirmOpen,
	submitting,
	showStorageDialogCloseButton = true,
	forceDefaultPolicy = false,
	storageDialogPresentation = "dialog",
	onStorageSetupLogout,
	onCancelConnectorAction,
	onCancelSaveAnyway,
	onConfirmSaveAnyway,
	onConfirmConnectorAction,
	onStartStorageAuthorization,
	onValidateStorageCredential,
	onCreateRemoteStorageTarget,
	onCreateBack,
	onCreateNext,
	onCreateStepChange,
	onDialogOpenChange,
	onConnectorIdChange,
	onFieldChange,
	onConnectorActionValueChange,
	onRequestConnectorAction,
	onRunConnectionTest,
	onSubmit,
}: PolicyDialogsProps) {
	const { t } = useTranslation("admin");

	return (
		<>
			<ConfirmDialog
				{...deleteDialogProps}
				title={`${t("delete_policy")} "${deletePolicyName}"?`}
				description={t("delete_policy_desc")}
				confirmLabel={t("core:delete")}
				variant="destructive"
			/>
			<ConfirmDialog
				{...forceDeleteDialogProps}
				title={`${t("force_delete_policy")} "${forceDeletePolicyName}"?`}
				description={t("force_delete_policy_desc")}
				confirmLabel={t("force_delete_policy_confirm")}
				variant="destructive"
			/>
			<StoragePolicyDialog
				open={dialogOpen}
				mode={editMode ? "edit" : "create"}
				form={form}
				storageDriverDescriptor={storageDriverDescriptor}
				storageDriverDescriptors={storageDriverDescriptors}
				storageDriverDescriptorsError={storageDriverDescriptorsError}
				storageDriverDescriptorsLoading={storageDriverDescriptorsLoading}
				policyCapacity={policyCapacity}
				policyCapacityLoading={policyCapacityLoading}
				storageCredentials={storageCredentials}
				storageCredentialsLoading={storageCredentialsLoading}
				storageAuthorizationSubmitting={storageAuthorizationSubmitting}
				storageCredentialValidationSubmitting={
					storageCredentialValidationSubmitting
				}
				storageAuthorizationRedirectUri={storageAuthorizationRedirectUri}
				connectorActionConfirmId={connectorActionConfirmId}
				connectorActionSubmittingId={connectorActionSubmittingId}
				connectorActionValues={connectorActionValues}
				remoteNodes={remoteNodes}
				remoteStorageTargetDriverDescriptors={
					remoteStorageTargetDriverDescriptors
				}
				remoteStorageTargetDriverDescriptorsError={
					remoteStorageTargetDriverDescriptorsError
				}
				remoteStorageTargetDriverDescriptorsLoading={
					remoteStorageTargetDriverDescriptorsLoading
				}
				remoteStorageTargets={remoteStorageTargets}
				remoteStorageTargetsError={remoteStorageTargetsError}
				remoteStorageTargetsLoading={remoteStorageTargetsLoading}
				submitting={submitting}
				createStep={createStep}
				createStepTouched={createStepTouched}
				endpointValidationMessage={endpointValidationMessage}
				saveAnywayConfirmOpen={saveAnywayConfirmOpen}
				onCancelConnectorAction={onCancelConnectorAction}
				onOpenChange={onDialogOpenChange}
				onCancelSaveAnyway={onCancelSaveAnyway}
				onConfirmSaveAnyway={onConfirmSaveAnyway}
				onConfirmConnectorAction={onConfirmConnectorAction}
				onStartStorageAuthorization={onStartStorageAuthorization}
				onValidateStorageCredential={onValidateStorageCredential}
				onCreateRemoteStorageTarget={onCreateRemoteStorageTarget}
				onSubmit={onSubmit}
				onRunConnectionTest={onRunConnectionTest}
				onFieldChange={onFieldChange}
				onConnectorActionValueChange={onConnectorActionValueChange}
				onRequestConnectorAction={onRequestConnectorAction}
				onConnectorIdChange={onConnectorIdChange}
				onCreateBack={onCreateBack}
				onCreateStepChange={onCreateStepChange}
				onCreateNext={onCreateNext}
				showCloseButton={showStorageDialogCloseButton}
				forceDefaultPolicy={forceDefaultPolicy}
				presentation={storageDialogPresentation}
				onSetupLogout={onStorageSetupLogout}
			/>
		</>
	);
}
