import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { getPolicyDriverBadgeClass } from "@/components/admin/admin-policies-page/policyPresentation";
import type { StoragePolicyDriverOption } from "@/components/admin/StoragePolicyDialogFields";
import {
	type PolicyFormData,
	supportsApplicationCredentials,
	supportsContentDedupPolicyOption,
	supportsDraftConnectionTest,
	supportsObjectStorageConnection,
	supportsOneDrivePolicyOptions,
	supportsRemoteNodeBinding,
	supportsS3TransferStrategy,
	supportsSavedConnectionTest,
} from "@/components/admin/storagePolicyDialogShared";
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
	RemoteNodeInfo,
	StorageConnectorDescriptor,
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
	onSyncNormalizedS3Form: () => void;
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
	onSubmit,
	onRequestS3DriverPromotion,
	onRunConnectionTest,
	onFieldChange,
	onDriverTypeChange,
	onCreateBack,
	onCreateStepChange,
	onCreateNext,
	onSyncNormalizedS3Form,
}: StoragePolicyDialogProps) {
	const { t } = useTranslation("admin");
	const isCreateMode = mode === "create";
	const storageOptions: StoragePolicyDriverOption[] = [
		{
			type: "local",
			title: t("driver_type_local"),
			description: t("policy_wizard_local_storage_desc"),
			iconSrc: "/static/asterdrive/asterdrive-dark.svg",
		},
		{
			type: "remote",
			title: t("driver_type_remote"),
			description: t("policy_wizard_remote_storage_desc"),
			iconSrc: "/static/storage/asterdrive-node.svg",
		},
		{
			type: "s3",
			title: t("driver_type_s3"),
			description: t("policy_wizard_s3_storage_desc"),
			iconSrc: "/static/storage/amazon-s3.svg",
		},
		{
			type: "tencent_cos",
			title: t("driver_type_tencent_cos"),
			description: t("policy_wizard_tencent_cos_storage_desc"),
			iconSrc: "/static/storage/tencent-cloud-cos.webp",
		},
		{
			type: "azure_blob",
			title: t("driver_type_azure_blob"),
			description: t("policy_wizard_azure_blob_storage_desc"),
			iconSrc: "/static/storage/azure-blob.svg",
		},
		{
			type: "one_drive",
			title: t("driver_type_onedrive"),
			description: t("policy_wizard_onedrive_storage_desc"),
			iconSrc: "/static/storage/onedrive.svg",
		},
	];
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
	const canUseS3TransferStrategy = supportsS3TransferStrategy(
		storageDriverDescriptor,
	);
	const canUseContentDedupPolicyOption = supportsContentDedupPolicyOption(
		storageDriverDescriptor,
	);
	const createSteps: StoragePolicyDialogStep[] = [
		{
			title: t("policy_wizard_step_storage_title"),
			description: t("policy_wizard_step_storage_desc"),
		},
		{
			title: canUseObjectStorageConnection
				? t("policy_wizard_step_connection_title")
				: canUseRemoteNodeBinding
					? t("policy_wizard_step_remote_title")
					: canUseOneDriveConnection
						? t("policy_wizard_step_onedrive_title")
						: t("policy_wizard_step_local_title"),
			description: canUseObjectStorageConnection
				? form.driver_type === "tencent_cos"
					? t("policy_wizard_step_tencent_cos_connection_desc")
					: form.driver_type === "azure_blob"
						? t("policy_wizard_step_azure_blob_connection_desc")
						: t("policy_wizard_step_connection_desc")
				: canUseRemoteNodeBinding
					? t("policy_wizard_step_remote_desc")
					: canUseOneDriveConnection
						? t("policy_wizard_step_onedrive_desc")
						: t("policy_wizard_step_local_desc"),
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
		storageOptions[0];
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
					form.driver_type === "azure_blob"
						? "policy_wizard_container_required"
						: "policy_wizard_bucket_required",
				)
			: null;
	const createOneDriveClientIdError =
		isCreateMode &&
		createStep === 1 &&
		createStepTouched &&
		canUseApplicationCredentials &&
		!form.onedrive_client_id.trim()
			? t("onedrive_client_id_required")
			: null;
	const createOneDriveClientSecretError =
		isCreateMode &&
		createStep === 1 &&
		createStepTouched &&
		canUseApplicationCredentials &&
		!form.onedrive_client_secret.trim()
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
	const createRemoteNodeError =
		isCreateMode &&
		createStep === 1 &&
		createStepTouched &&
		canUseRemoteNodeBinding &&
		!form.remote_node_id
			? t("policy_wizard_remote_node_required")
			: null;
	const selectedRemoteNode =
		remoteNodes.find((node) => String(node.id) === form.remote_node_id) ?? null;
	const s3UploadStrategyLabel =
		form.s3_upload_strategy === "relay_stream"
			? t("upload_strategy_relay_stream")
			: t("upload_strategy_presigned");
	const s3DownloadStrategyLabel =
		form.s3_download_strategy === "relay_stream"
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
	const showTencentCosCorsPanel = form.driver_type === "tencent_cos";
	const showTencentCosCorsAction =
		showTencentCosCorsPanel && canConfigureTencentCosCors;
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
	const cosNativeSummaryItems =
		form.driver_type === "tencent_cos"
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
				(form.driver_type === "local" ? "./data" : t("core:root")),
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
					...(canUseS3TransferStrategy
						? [
								{
									label: t("s3_upload_strategy"),
									value: s3UploadStrategyLabel,
								},
								{
									label: t("s3_download_strategy"),
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
								createRemoteNodeError={createRemoteNodeError}
								createStep={createStep}
								createStepDirection={createStepDirection}
								createSteps={createSteps}
								currentStorageOption={currentStorageOption}
								endpointValidationMessage={createEndpointError}
								form={form}
								storageDriverDescriptor={storageDriverDescriptor}
								onCreateStepChange={onCreateStepChange}
								onDriverTypeChange={onDriverTypeChange}
								onFieldChange={onFieldChange}
								onApplyS3CompatibleDriverSuggestion={
									onApplyS3CompatibleDriverSuggestion
								}
								onSyncNormalizedS3Form={onSyncNormalizedS3Form}
								remoteNodes={remoteNodes}
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
								createRemoteNodeError={createRemoteNodeError}
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
								onRequestS3DriverPromotion={onRequestS3DriverPromotion}
								onSyncNormalizedS3Form={onSyncNormalizedS3Form}
								cosCorsConfirmOpen={cosCorsConfirmOpen}
								canConfigureTencentCosCors={canConfigureTencentCosCors}
								cosCorsSubmitting={cosCorsSubmitting}
								cosCorsUsesDraftValues={cosCorsUsesDraftValues}
								remoteNodes={remoteNodes}
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
