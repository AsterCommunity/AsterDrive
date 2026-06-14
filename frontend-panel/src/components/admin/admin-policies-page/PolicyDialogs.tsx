import { useTranslation } from "react-i18next";
import { StoragePolicyDialog } from "@/components/admin/StoragePolicyDialog";
import type { PolicyFormData } from "@/components/admin/storagePolicyDialogShared";
import type { ConfirmDialogProps } from "@/components/common/ConfirmDialog";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import type {
	DriverType,
	RemoteNodeInfo,
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
	policyCapacity: StoragePolicyCapacityInfo | null;
	policyCapacityLoading: boolean;
	cosCorsConfirmOpen: boolean;
	cosCorsSubmitting: boolean;
	cosCorsTargetOrigin: string;
	cosCorsUsesDraftValues: boolean;
	s3CompatibleDriverSuggestionTargetLabel: string | null;
	s3DriverPromotionBlocked: boolean;
	s3DriverPromotionConfirmOpen: boolean;
	s3DriverPromotionSubmitting: boolean;
	s3DriverPromotionTargetLabel: string | null;
	remoteNodes: RemoteNodeInfo[];
	saveAnywayConfirmOpen: boolean;
	submitting: boolean;
	onApplyS3CompatibleDriverSuggestion: () => void;
	onCancelCosCorsConfigure: () => void;
	onCancelSaveAnyway: () => void;
	onCancelS3DriverPromotion: () => void;
	onConfirmSaveAnyway: () => void;
	onConfirmCosCorsConfigure: () => void;
	onConfirmS3DriverPromotion: () => void;
	onCreateBack: () => void;
	onCreateNext: () => void;
	onCreateStepChange: (step: number) => void;
	onDialogOpenChange: (open: boolean) => void;
	onDriverTypeChange: (driverType: DriverType) => void;
	onFieldChange: <K extends keyof PolicyFormData>(
		key: K,
		value: PolicyFormData[K],
	) => void;
	onRequestS3DriverPromotion: () => void;
	onRunConnectionTest: () => Promise<boolean>;
	onSubmit: () => void;
	onSyncNormalizedS3Form: () => void;
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
	policyCapacity,
	policyCapacityLoading,
	cosCorsConfirmOpen,
	cosCorsSubmitting,
	cosCorsTargetOrigin,
	cosCorsUsesDraftValues,
	s3CompatibleDriverSuggestionTargetLabel,
	s3DriverPromotionBlocked,
	s3DriverPromotionConfirmOpen,
	s3DriverPromotionSubmitting,
	s3DriverPromotionTargetLabel,
	remoteNodes,
	saveAnywayConfirmOpen,
	submitting,
	onApplyS3CompatibleDriverSuggestion,
	onCancelCosCorsConfigure,
	onCancelSaveAnyway,
	onCancelS3DriverPromotion,
	onConfirmSaveAnyway,
	onConfirmCosCorsConfigure,
	onConfirmS3DriverPromotion,
	onCreateBack,
	onCreateNext,
	onCreateStepChange,
	onDialogOpenChange,
	onDriverTypeChange,
	onFieldChange,
	onRequestS3DriverPromotion,
	onRunConnectionTest,
	onSubmit,
	onSyncNormalizedS3Form,
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
				policyCapacity={policyCapacity}
				policyCapacityLoading={policyCapacityLoading}
				cosCorsConfirmOpen={cosCorsConfirmOpen}
				cosCorsSubmitting={cosCorsSubmitting}
				cosCorsTargetOrigin={cosCorsTargetOrigin}
				cosCorsUsesDraftValues={cosCorsUsesDraftValues}
				s3CompatibleDriverSuggestionTargetLabel={
					s3CompatibleDriverSuggestionTargetLabel
				}
				s3DriverPromotionBlocked={s3DriverPromotionBlocked}
				s3DriverPromotionConfirmOpen={s3DriverPromotionConfirmOpen}
				s3DriverPromotionSubmitting={s3DriverPromotionSubmitting}
				s3DriverPromotionTargetLabel={s3DriverPromotionTargetLabel}
				remoteNodes={remoteNodes}
				submitting={submitting}
				createStep={createStep}
				createStepTouched={createStepTouched}
				endpointValidationMessage={endpointValidationMessage}
				saveAnywayConfirmOpen={saveAnywayConfirmOpen}
				onApplyS3CompatibleDriverSuggestion={
					onApplyS3CompatibleDriverSuggestion
				}
				onCancelCosCorsConfigure={onCancelCosCorsConfigure}
				onOpenChange={onDialogOpenChange}
				onCancelSaveAnyway={onCancelSaveAnyway}
				onCancelS3DriverPromotion={onCancelS3DriverPromotion}
				onConfirmSaveAnyway={onConfirmSaveAnyway}
				onConfirmCosCorsConfigure={onConfirmCosCorsConfigure}
				onConfirmS3DriverPromotion={onConfirmS3DriverPromotion}
				onSubmit={onSubmit}
				onRequestS3DriverPromotion={onRequestS3DriverPromotion}
				onRunConnectionTest={onRunConnectionTest}
				onFieldChange={onFieldChange}
				onDriverTypeChange={onDriverTypeChange}
				onCreateBack={onCreateBack}
				onCreateStepChange={onCreateStepChange}
				onCreateNext={onCreateNext}
				onSyncNormalizedS3Form={onSyncNormalizedS3Form}
			/>
		</>
	);
}
