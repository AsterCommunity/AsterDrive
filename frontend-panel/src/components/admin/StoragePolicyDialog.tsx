import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { getPolicyDriverBadgeClass } from "@/components/admin/admin-policies-page/policyPresentation";
import type { StoragePolicyDriverOption } from "@/components/admin/StoragePolicyDialogFields";
import { microsoftGraphCredentials } from "@/components/admin/storage-policy-dialog/applicationCredentials";
import {
	supportsApplicationCredentials,
	supportsContentDedupPolicyOption,
	supportsDraftConnectionTest,
	supportsObjectStorageConnection,
	supportsObjectStorageTransferStrategy,
	supportsOneDrivePolicyOptions,
	supportsRemoteNodeBinding,
	supportsSavedConnectionTest,
	supportsStorageNativeProcessing,
} from "@/components/admin/storage-policy-dialog/descriptorPredicates";
import type { PolicyFormData } from "@/components/admin/storage-policy-dialog/formTypes";
import { InlineConfirm } from "@/components/common/ManagerDialogShell";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	DriverType,
	RemoteCreateStorageTargetRequest,
	RemoteNodeInfo,
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetInfo,
	StorageConnectorDescriptor,
	StorageConnectorUiDescriptor,
	StoragePolicyCapacityInfo,
	StoragePolicyCredentialInfo,
} from "@/types/api";
import { StoragePolicyCreateWizard } from "./storage-policy-dialog/StoragePolicyCreateWizard";
import type { StoragePolicyDialogStep } from "./storage-policy-dialog/StoragePolicyDialogTypes";
import { StoragePolicyEditForm } from "./storage-policy-dialog/StoragePolicyEditForm";
import { StoragePolicyTestConnectionButton } from "./storage-policy-dialog/StoragePolicyTestConnectionButton";

interface StoragePolicyDialogProps {
	open: boolean;
	mode: "create" | "edit";
	form: PolicyFormData;
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	storageDriverDescriptors: StorageConnectorDescriptor[];
	storageDriverDescriptorsError: string | null;
	storageDriverDescriptorsLoading: boolean;
	policyCapacity: StoragePolicyCapacityInfo | null;
	policyCapacityLoading: boolean;
	storageCredentials: StoragePolicyCredentialInfo[];
	storageCredentialsLoading: boolean;
	storageAuthorizationSubmitting: boolean;
	storageCredentialValidationSubmitting: boolean;
	storageAuthorizationRedirectUri: string;
	s3CompatibleDriverSuggestionTargetLabel: string | null;
	s3DriverPromotionBlocked: boolean;
	s3DriverPromotionConfirmOpen: boolean;
	s3DriverPromotionSubmitting: boolean;
	s3DriverPromotionTargetLabel: string | null;
	remoteNodes: RemoteNodeInfo[];
	remoteStorageTargetDriverDescriptors: RemoteStorageTargetDriverDescriptor[];
	remoteStorageTargetDriverDescriptorsError: string | null;
	remoteStorageTargetDriverDescriptorsLoading: boolean;
	remoteStorageTargets: RemoteStorageTargetInfo[];
	remoteStorageTargetsError: string | null;
	remoteStorageTargetsLoading: boolean;
	submitting: boolean;
	createStep: number;
	createStepTouched: boolean;
	endpointValidationMessage: string | null;
	cosCorsConfirmOpen: boolean;
	cosCorsSubmitting: boolean;
	cosCorsUsesDraftValues: boolean;
	canConfigureTencentCosCors: boolean;
	saveAnywayConfirmOpen: boolean;
	onApplyS3CompatibleDriverSuggestion: () => void;
	onCancelCosCorsConfigure: () => void;
	onOpenChange: (open: boolean) => void;
	onCancelSaveAnyway: () => void;
	onCancelS3DriverPromotion: () => void;
	onConfirmSaveAnyway: () => void;
	onConfirmCosCorsConfigure: () => void;
	onConfirmS3DriverPromotion: () => void;
	onStartStorageAuthorization: () => void;
	onValidateStorageCredential: () => void;
	onCreateRemoteStorageTarget: (
		payload: RemoteCreateStorageTargetRequest,
	) => Promise<void>;
	onSubmit: () => void;
	onRequestS3DriverPromotion: () => void;
	onRunConnectionTest: () => Promise<boolean>;
	onFieldChange: <K extends keyof PolicyFormData>(
		key: K,
		value: PolicyFormData[K],
	) => void;
	onDriverTypeChange: (driverType: DriverType) => void;
	onCreateBack: () => void;
	onCreateStepChange: (step: number) => void;
	onCreateNext: () => void;
	onSyncNormalizedObjectStorageForm: () => void;
}

interface StorageNativeLabelOptions {
	enabled: boolean;
	extensions: string[];
	disabledLabel: string;
}

function getStorageNativeLabel({
	enabled,
	extensions,
	disabledLabel,
}: StorageNativeLabelOptions) {
	return enabled && extensions.length > 0
		? extensions.join(", ")
		: disabledLabel;
}

export function StoragePolicyDialog(props: StoragePolicyDialogProps) {
	return useStoragePolicyDialogContent(props);
}

function useStoragePolicyDialogContent({
	open,
	mode,
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
	s3CompatibleDriverSuggestionTargetLabel,
	s3DriverPromotionBlocked,
	s3DriverPromotionConfirmOpen,
	s3DriverPromotionSubmitting,
	s3DriverPromotionTargetLabel,
	remoteNodes,
	remoteStorageTargetDriverDescriptors,
	remoteStorageTargetDriverDescriptorsError,
	remoteStorageTargetDriverDescriptorsLoading,
	remoteStorageTargets,
	remoteStorageTargetsError,
	remoteStorageTargetsLoading,
	submitting,
	createStep,
	createStepTouched,
	endpointValidationMessage,
	cosCorsConfirmOpen,
	cosCorsSubmitting,
	cosCorsUsesDraftValues,
	canConfigureTencentCosCors,
	saveAnywayConfirmOpen,
	onApplyS3CompatibleDriverSuggestion,
	onCancelCosCorsConfigure,
	onOpenChange,
	onCancelSaveAnyway,
	onCancelS3DriverPromotion,
	onConfirmSaveAnyway,
	onConfirmCosCorsConfigure,
	onConfirmS3DriverPromotion,
	onStartStorageAuthorization,
	onValidateStorageCredential,
	onCreateRemoteStorageTarget,
	onSubmit,
	onRequestS3DriverPromotion,
	onRunConnectionTest,
	onFieldChange,
	onDriverTypeChange,
	onCreateBack,
	onCreateStepChange,
	onCreateNext,
	onSyncNormalizedObjectStorageForm,
}: StoragePolicyDialogProps) {
	const { t } = useTranslation("admin");
	const isCreateMode = mode === "create";
	const storageOptions = buildStoragePolicyDriverOptions(
		storageDriverDescriptors,
		t,
	);
	const canUseObjectStorageConnection = supportsObjectStorageConnection(
		storageDriverDescriptor,
	);
	const canUseRemoteNodeBinding = supportsRemoteNodeBinding(
		storageDriverDescriptor,
	);
	const canUseApplicationCredentials = supportsApplicationCredentials(
		storageDriverDescriptor,
	);
	const canUseOneDrivePolicyOptions = supportsOneDrivePolicyOptions(
		storageDriverDescriptor,
	);
	const canUseOneDriveConnection =
		canUseApplicationCredentials || canUseOneDrivePolicyOptions;
	const canUseObjectStorageTransferStrategy =
		supportsObjectStorageTransferStrategy(storageDriverDescriptor);
	const canUseContentDedupPolicyOption = supportsContentDedupPolicyOption(
		storageDriverDescriptor,
	);
	const supportsStorageNative = supportsStorageNativeProcessing(
		storageDriverDescriptor,
	);
	const currentStorageUi =
		storageDriverDescriptor?.ui ?? fallbackStorageConnectorUi();
	const createSteps: StoragePolicyDialogStep[] = [
		{
			title: t("policy_wizard_step_storage_title"),
			description: t("policy_wizard_step_storage_desc"),
		},
		{
			title: t(currentStorageUi.config_step_title_key),
			description: t(currentStorageUi.config_step_description_key),
		},
		{
			title: t("policy_wizard_step_rules_title"),
			description: t("policy_wizard_step_rules_desc"),
		},
	];
	const createLastStep = createSteps.length - 1;
	const previousCreateStepRef = useRef(createStep);
	const stepAnimationRef = useRef<{
		direction: "idle" | "forward" | "backward";
		step: number;
	}>({
		direction: "idle",
		step: createStep,
	});
	if (createStep !== previousCreateStepRef.current) {
		stepAnimationRef.current = {
			direction:
				createStep > previousCreateStepRef.current ? "forward" : "backward",
			step: createStep,
		};
	}
	const createStepDirection = stepAnimationRef.current.direction;
	const stepAnimationKey = `${stepAnimationRef.current.step}-${stepAnimationRef.current.direction}`;
	const currentStorageOption =
		storageOptions.find((option) => option.type === form.driver_type) ??
		storagePolicyDriverOptionFromUi(
			form.driver_type,
			fallbackStorageConnectorUi(),
			t,
		);
	const currentDriverBadgeClass = getPolicyDriverBadgeClass(form.driver_type);
	const createNameError =
		isCreateMode && createStep === 1 && createStepTouched && !form.name.trim()
			? t("policy_wizard_name_required")
			: null;
	const createBucketError =
		isCreateMode &&
		createStep === 1 &&
		createStepTouched &&
		canUseObjectStorageConnection &&
		!form.bucket.trim()
			? t(
					storageDriverDescriptor?.fields.find(
						(field) => field.scope === "connection" && field.name === "bucket",
					)?.required_message_key ?? "policy_wizard_bucket_required",
				)
			: null;
	const createOneDriveClientIdError =
		isCreateMode &&
		createStep === 1 &&
		createStepTouched &&
		canUseApplicationCredentials &&
		!microsoftGraphCredentials(form).client_id.trim()
			? t("onedrive_client_id_required")
			: null;
	const createOneDriveClientSecretError =
		isCreateMode &&
		createStep === 1 &&
		createStepTouched &&
		canUseApplicationCredentials &&
		!microsoftGraphCredentials(form).client_secret.trim()
			? t("onedrive_client_secret_required")
			: null;
	const createEndpointError =
		canUseObjectStorageConnection && !form.endpoint.trim()
			? isCreateMode
				? createStep === 1 && createStepTouched
					? t("policy_wizard_endpoint_required")
					: null
				: t("policy_wizard_endpoint_required")
			: endpointValidationMessage;
	const showRemoteBindingValidation =
		canUseRemoteNodeBinding &&
		createStepTouched &&
		(!isCreateMode || createStep === 1);
	const createRemoteNodeError =
		showRemoteBindingValidation && !form.remote_node_id
			? t("policy_wizard_remote_node_required")
			: null;
	const createRemoteTargetError =
		showRemoteBindingValidation &&
		form.remote_node_id &&
		!form.remote_storage_target_key
			? t("policy_wizard_remote_storage_target_required")
			: null;
	const createRemoteBindingError =
		createRemoteNodeError ?? createRemoteTargetError;
	const selectedRemoteNode =
		remoteNodes.find((node) => String(node.id) === form.remote_node_id) ?? null;
	const s3UploadStrategyLabel =
		form.object_storage_upload_strategy === "relay_stream"
			? t("upload_strategy_relay_stream")
			: t("upload_strategy_presigned");
	const s3DownloadStrategyLabel =
		form.object_storage_download_strategy === "relay_stream"
			? t("download_strategy_relay_stream")
			: t("download_strategy_presigned");
	const remoteUploadStrategyLabel =
		form.remote_upload_strategy === "relay_stream"
			? t("upload_strategy_relay_stream")
			: t("upload_strategy_presigned");
	const remoteDownloadStrategyLabel =
		form.remote_download_strategy === "relay_stream"
			? t("download_strategy_relay_stream")
			: t("download_strategy_presigned");
	const contentDedupLabel = form.content_dedup
		? t("policy_wizard_enabled")
		: t("policy_wizard_disabled");
	const storageNativeThumbnailExtensionsLabel = getStorageNativeLabel({
		enabled:
			form.storage_native_processing_enabled &&
			form.thumbnail_processor === "storage_native",
		extensions: form.thumbnail_extensions,
		disabledLabel: t("policy_wizard_disabled"),
	});
	const storageNativeMediaMetadataExtensionsLabel = getStorageNativeLabel({
		enabled:
			form.storage_native_processing_enabled &&
			form.storage_native_media_metadata_enabled === true,
		extensions: form.media_metadata_extensions ?? [],
		disabledLabel: t("policy_wizard_disabled"),
	});
	const showTencentCosCorsAction = canConfigureTencentCosCors;
	const showCreateTencentCosCorsConfirm =
		isCreateMode && showTencentCosCorsAction && cosCorsConfirmOpen;
	const canRunDraftConnectionTest = supportsDraftConnectionTest(
		storageDriverDescriptor,
	);
	const canRunSavedConnectionTest = supportsSavedConnectionTest(
		storageDriverDescriptor,
	);
	const canRunConnectionTest = isCreateMode
		? canRunDraftConnectionTest
		: canRunDraftConnectionTest || canRunSavedConnectionTest;
	const cosNativeSummaryItems = supportsStorageNative
		? [
				{
					label: t("storage_native_processing_enabled"),
					value: form.storage_native_processing_enabled
						? t("policy_wizard_enabled")
						: t("policy_wizard_disabled"),
				},
				{
					label: t("storage_native_thumbnail_extensions"),
					value: storageNativeThumbnailExtensionsLabel,
				},
				{
					label: t("storage_native_media_metadata_extensions"),
					value: storageNativeMediaMetadataExtensionsLabel,
				},
			]
		: [];
	const createSummaryItems = [
		{ label: t("driver_type"), value: currentStorageOption.title },
		{
			label: t("base_path"),
			value:
				form.base_path ||
				translateStorageConnectorUiValue(
					currentStorageUi.base_path_empty_display,
					t,
				),
		},
		{
			label: t("max_file_size"),
			value:
				form.max_file_size === "" || Number(form.max_file_size) === 0
					? t("core:unlimited")
					: `${form.max_file_size} bytes`,
		},
		{
			label: t("chunk_size"),
			value: `${form.chunk_size || "0"} MB`,
		},
		{
			label: t("set_as_default"),
			value: form.is_default
				? t("policy_wizard_enabled")
				: t("policy_wizard_disabled"),
		},
		...(canUseContentDedupPolicyOption
			? [
					{
						label: t("content_dedup"),
						value: contentDedupLabel,
					},
				]
			: []),
		...(canUseObjectStorageConnection
			? [
					{
						label: t("endpoint"),
						value: form.endpoint || t("policy_wizard_default_endpoint"),
					},
					{ label: t("bucket"), value: form.bucket || "—" },
					...(canUseObjectStorageTransferStrategy
						? [
								{
									label: t("object_storage_upload_strategy"),
									value: s3UploadStrategyLabel,
								},
								{
									label: t("object_storage_download_strategy"),
									value: s3DownloadStrategyLabel,
								},
							]
						: []),
					...cosNativeSummaryItems,
				]
			: []),
		...(canUseRemoteNodeBinding
			? [
					{
						label: t("remote_node"),
						value:
							selectedRemoteNode?.name ??
							t("policy_wizard_remote_node_unselected"),
					},
					{
						label: t("remote_download_strategy"),
						value: remoteDownloadStrategyLabel,
					},
					{
						label: t("remote_upload_strategy"),
						value: remoteUploadStrategyLabel,
					},
				]
			: []),
		...(canUseOneDriveConnection
			? [
					...(canUseOneDrivePolicyOptions
						? [
								{
									label: t("onedrive_cloud"),
									value: t(`onedrive_cloud_${form.onedrive_cloud}`),
								},
								{
									label: isCreateMode
										? t("onedrive_target_summary")
										: t("onedrive_account_mode"),
									value: isCreateMode
										? t("onedrive_target_summary_auto")
										: t(`onedrive_account_mode_${form.onedrive_account_mode}`),
								},
								...(!isCreateMode
									? [
											{
												label: t("onedrive_drive_id"),
												value:
													form.onedrive_drive_id ||
													t("policy_wizard_default_drive"),
											},
											{
												label: t("onedrive_root_item_id"),
												value: form.onedrive_root_item_id || "root",
											},
											...(form.onedrive_account_mode === "sharepoint_site"
												? [
														{
															label: t("onedrive_site_id"),
															value: form.onedrive_site_id || "—",
														},
													]
												: []),
											...(form.onedrive_account_mode === "group_drive"
												? [
														{
															label: t("onedrive_group_id"),
															value: form.onedrive_group_id || "—",
														},
													]
												: []),
										]
									: []),
							]
						: []),
				]
			: []),
	];
	const driverOptionsError =
		storageOptions.length === 0 ? storageDriverDescriptorsError : null;
	useEffect(() => {
		if (!open || !isCreateMode) {
			previousCreateStepRef.current = 0;
			stepAnimationRef.current = {
				direction: "idle",
				step: 0,
			};
			return;
		}

		previousCreateStepRef.current = createStep;
	}, [createStep, isCreateMode, open]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="flex max-h-[min(90vh,calc(100vh-2rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-[calc(100%-2rem)] lg:max-w-4xl">
				<DialogHeader className="shrink-0 px-6 pt-5 pb-0 pr-14">
					<DialogTitle>
						{isCreateMode ? t("create_policy") : t("edit_policy")}
					</DialogTitle>
					{isCreateMode ? null : (
						<DialogDescription>{t("policies_intro")}</DialogDescription>
					)}
				</DialogHeader>
				<form
					onSubmit={(e) => e.preventDefault()}
					autoComplete="off"
					className="flex min-h-0 flex-1 flex-col overflow-hidden"
				>
					<div className="min-h-0 flex-1 overflow-y-auto px-6 pt-6 pb-5">
						{isCreateMode ? (
							<StoragePolicyCreateWizard
								createBucketError={createBucketError}
								createNameError={createNameError}
								createOneDriveClientIdError={createOneDriveClientIdError}
								createOneDriveClientSecretError={
									createOneDriveClientSecretError
								}
								createRemoteTargetError={createRemoteBindingError}
								createStep={createStep}
								createStepDirection={createStepDirection}
								createSteps={createSteps}
								currentStorageOption={currentStorageOption}
								endpointValidationMessage={createEndpointError}
								form={form}
								storageDriverDescriptorsError={driverOptionsError}
								storageDriverDescriptorsLoading={
									storageDriverDescriptorsLoading && storageOptions.length === 0
								}
								storageDriverDescriptor={storageDriverDescriptor}
								onCreateStepChange={onCreateStepChange}
								onDriverTypeChange={onDriverTypeChange}
								onFieldChange={onFieldChange}
								onCreateRemoteStorageTarget={onCreateRemoteStorageTarget}
								onApplyS3CompatibleDriverSuggestion={
									onApplyS3CompatibleDriverSuggestion
								}
								onSyncNormalizedObjectStorageForm={
									onSyncNormalizedObjectStorageForm
								}
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
								s3CompatibleDriverSuggestionTargetLabel={
									s3CompatibleDriverSuggestionTargetLabel
								}
								stepAnimationKey={stepAnimationKey}
								storageOptions={storageOptions}
								summaryItems={createSummaryItems}
							/>
						) : (
							<StoragePolicyEditForm
								createBucketError={createBucketError}
								createNameError={createNameError}
								createRemoteTargetError={createRemoteBindingError}
								currentDriverBadgeClass={currentDriverBadgeClass}
								currentStorageOption={currentStorageOption}
								endpointValidationMessage={endpointValidationMessage}
								form={form}
								storageDriverDescriptor={storageDriverDescriptor}
								policyCapacity={policyCapacity}
								policyCapacityLoading={policyCapacityLoading}
								storageCredentials={storageCredentials}
								storageCredentialsLoading={storageCredentialsLoading}
								storageAuthorizationSubmitting={storageAuthorizationSubmitting}
								storageCredentialValidationSubmitting={
									storageCredentialValidationSubmitting
								}
								storageAuthorizationRedirectUri={
									storageAuthorizationRedirectUri
								}
								s3DriverPromotionBlocked={s3DriverPromotionBlocked}
								s3DriverPromotionConfirmOpen={s3DriverPromotionConfirmOpen}
								s3DriverPromotionSubmitting={s3DriverPromotionSubmitting}
								s3DriverPromotionTargetLabel={s3DriverPromotionTargetLabel}
								onFieldChange={onFieldChange}
								onCancelS3DriverPromotion={onCancelS3DriverPromotion}
								onCancelCosCorsConfigure={onCancelCosCorsConfigure}
								onConfirmCosCorsConfigure={onConfirmCosCorsConfigure}
								onConfirmS3DriverPromotion={onConfirmS3DriverPromotion}
								onStartStorageAuthorization={onStartStorageAuthorization}
								onValidateStorageCredential={onValidateStorageCredential}
								onCreateRemoteStorageTarget={onCreateRemoteStorageTarget}
								onRequestS3DriverPromotion={onRequestS3DriverPromotion}
								onSyncNormalizedObjectStorageForm={
									onSyncNormalizedObjectStorageForm
								}
								cosCorsConfirmOpen={cosCorsConfirmOpen}
								canConfigureTencentCosCors={canConfigureTencentCosCors}
								cosCorsSubmitting={cosCorsSubmitting}
								cosCorsUsesDraftValues={cosCorsUsesDraftValues}
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
							/>
						)}
					</div>
					{showCreateTencentCosCorsConfirm ? (
						<div className="shrink-0 border-t px-6 py-3">
							<InlineConfirm>
								<div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
									<div>
										<p className="text-sm font-medium">
											{t("policy_cos_cors_confirm_title")}
										</p>
										<p className="mt-1 text-xs leading-5 text-muted-foreground">
											{t("policy_cos_cors_confirm_desc")}
										</p>
									</div>
									<div className="flex shrink-0 items-center gap-2">
										<Button
											type="button"
											variant="outline"
											className={ADMIN_CONTROL_HEIGHT_CLASS}
											onClick={onCancelCosCorsConfigure}
											disabled={cosCorsSubmitting}
										>
											{t("core:cancel")}
										</Button>
										<Button
											type="button"
											className={ADMIN_CONTROL_HEIGHT_CLASS}
											onClick={onConfirmCosCorsConfigure}
											disabled={cosCorsSubmitting}
										>
											{cosCorsSubmitting ? (
												<Icon
													name="Spinner"
													className="mr-1 size-3.5 animate-spin"
												/>
											) : null}
											{t("policy_cos_cors_confirm")}
										</Button>
									</div>
								</div>
							</InlineConfirm>
						</div>
					) : null}
					{saveAnywayConfirmOpen ? (
						<div className="shrink-0 border-t px-6 py-3">
							<InlineConfirm>
								<div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
									<div>
										<p className="text-sm font-medium">
											{t("connection_test_failed")}
										</p>
										<p className="mt-1 text-xs text-muted-foreground">
											{t("policy_test_failed_confirm_desc")}
										</p>
									</div>
									<div className="flex shrink-0 items-center gap-2">
										<Button
											type="button"
											variant="outline"
											className={ADMIN_CONTROL_HEIGHT_CLASS}
											onClick={onCancelSaveAnyway}
											disabled={submitting}
										>
											{t("core:cancel")}
										</Button>
										<Button
											type="button"
											className={ADMIN_CONTROL_HEIGHT_CLASS}
											onClick={onConfirmSaveAnyway}
											disabled={submitting}
										>
											{t("save_anyway")}
										</Button>
									</div>
								</div>
							</InlineConfirm>
						</div>
					) : null}
					<DialogFooter className="mx-0 mb-0 w-full shrink-0 flex-row items-center gap-2 rounded-b-xl px-6 py-3">
						<div className="mr-auto flex shrink-0 gap-2">
							{isCreateMode && createStep > 0 ? (
								<Button
									type="button"
									variant="outline"
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									onClick={onCreateBack}
									disabled={submitting}
								>
									{t("core:back")}
								</Button>
							) : null}
						</div>

						<div className="ml-auto flex shrink-0 flex-nowrap items-center justify-end gap-2">
							{isCreateMode ? (
								createStep === 0 ? null : createStep === createLastStep ? (
									<>
										{showTencentCosCorsAction ? (
											<TencentCosCorsButton
												disabled={
													submitting ||
													cosCorsSubmitting ||
													showCreateTencentCosCorsConfirm
												}
												onClick={onConfirmCosCorsConfigure}
												t={t}
											/>
										) : null}
										{canRunConnectionTest ? (
											<StoragePolicyTestConnectionButton
												onTest={onRunConnectionTest}
												disabled={submitting}
											/>
										) : null}
										<Button
											type="button"
											className={ADMIN_CONTROL_HEIGHT_CLASS}
											disabled={submitting}
											onClick={onSubmit}
										>
											{t("core:create")}
										</Button>
									</>
								) : (
									<>
										{createStep === 1 && canRunDraftConnectionTest ? (
											<StoragePolicyTestConnectionButton
												onTest={onRunConnectionTest}
												disabled={submitting}
											/>
										) : null}
										<Button
											type="button"
											className={ADMIN_CONTROL_HEIGHT_CLASS}
											onClick={onCreateNext}
											disabled={submitting}
										>
											{createStep === createLastStep - 1
												? t("policy_wizard_review")
												: t("policy_wizard_next")}
										</Button>
									</>
								)
							) : (
								<>
									{canRunConnectionTest ? (
										<StoragePolicyTestConnectionButton
											onTest={onRunConnectionTest}
											disabled={submitting}
										/>
									) : null}
									<Button
										type="button"
										className={ADMIN_CONTROL_HEIGHT_CLASS}
										disabled={submitting}
										onClick={onSubmit}
									>
										{t("save_changes")}
									</Button>
								</>
							)}
						</div>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}

function TencentCosCorsButton({
	disabled,
	onClick,
	t,
}: {
	disabled: boolean;
	onClick: () => void;
	t: (key: string) => string;
}) {
	return (
		<Button
			type="button"
			variant="outline"
			className={ADMIN_CONTROL_HEIGHT_CLASS}
			disabled={disabled}
			onClick={onClick}
		>
			{t("policy_cos_cors_action_short")}
		</Button>
	);
}

function buildStoragePolicyDriverOptions(
	descriptors: StorageConnectorDescriptor[],
	t: (key: string) => string,
): StoragePolicyDriverOption[] {
	if (descriptors.length > 0) {
		return descriptors.map((descriptor) =>
			storagePolicyDriverOptionFromUi(
				descriptor.driver_type,
				descriptor.ui ?? fallbackStorageConnectorUi(),
				t,
			),
		);
	}

	return [];
}

function storagePolicyDriverOptionFromUi(
	driverType: DriverType,
	ui: StorageConnectorUiDescriptor,
	t: (key: string) => string,
): StoragePolicyDriverOption {
	return {
		type: driverType,
		title: t(ui.label_key),
		description: t(ui.description_key),
		iconSrc: ui.icon_src ?? undefined,
		iconName:
			(ui.icon_name as StoragePolicyDriverOption["iconName"] | null) ??
			undefined,
	};
}

function fallbackStorageConnectorUi(): StorageConnectorUiDescriptor {
	return {
		label_key: "driver_type",
		description_key: "policy_wizard_step_storage_desc",
		helper_key: "policy_wizard_step_storage_desc",
		config_step_title_key: "policy_wizard_step_connection_title",
		config_step_description_key: "policy_wizard_step_connection_desc",
		edit_context_key: "policy_edit_context_local_desc",
		base_path_empty_display: "core:root",
		base_path_placeholder: "tenant/prefix",
	};
}

export function translateStorageConnectorUiValue(
	value: string,
	t: (key: string) => string,
) {
	return t(value);
}
