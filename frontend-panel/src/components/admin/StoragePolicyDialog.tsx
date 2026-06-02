import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { StoragePolicyDriverOption } from "@/components/admin/StoragePolicyDialogFields";
import {
	isS3CompatibleDriver,
	type PolicyFormData,
} from "@/components/admin/storagePolicyDialogShared";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	DriverType,
	RemoteNodeInfo,
	StoragePolicyCapacityInfo,
} from "@/types/api";
import { StoragePolicyCreateWizard } from "./storage-policy-dialog/StoragePolicyCreateWizard";
import type { StoragePolicyDialogStep } from "./storage-policy-dialog/StoragePolicyDialogTypes";
import { StoragePolicyEditForm } from "./storage-policy-dialog/StoragePolicyEditForm";
import { StoragePolicyTestConnectionButton } from "./storage-policy-dialog/StoragePolicyTestConnectionButton";

interface StoragePolicyDialogProps {
	open: boolean;
	mode: "create" | "edit";
	form: PolicyFormData;
	policyCapacity: StoragePolicyCapacityInfo | null;
	policyCapacityLoading: boolean;
	remoteNodes: RemoteNodeInfo[];
	submitting: boolean;
	createStep: number;
	createStepTouched: boolean;
	endpointValidationMessage: string | null;
	onOpenChange: (open: boolean) => void;
	onSubmit: () => void;
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
	policyCapacity,
	policyCapacityLoading,
	remoteNodes,
	submitting,
	createStep,
	createStepTouched,
	endpointValidationMessage,
	onOpenChange,
	onSubmit,
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
			type: "remote",
			title: t("driver_type_remote"),
			description: t("policy_wizard_remote_storage_desc"),
			iconSrc: "/static/storage/asterdrive-node.svg",
		},
	];
	const createSteps: StoragePolicyDialogStep[] = [
		{
			title: t("policy_wizard_step_storage_title"),
			description: t("policy_wizard_step_storage_desc"),
		},
		{
			title: isS3CompatibleDriver(form.driver_type)
				? t("policy_wizard_step_connection_title")
				: form.driver_type === "remote"
					? t("policy_wizard_step_remote_title")
					: t("policy_wizard_step_local_title"),
			description: isS3CompatibleDriver(form.driver_type)
				? form.driver_type === "tencent_cos"
					? t("policy_wizard_step_tencent_cos_connection_desc")
					: t("policy_wizard_step_connection_desc")
				: form.driver_type === "remote"
					? t("policy_wizard_step_remote_desc")
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
	const currentDriverBadgeClass =
		form.driver_type === "s3"
			? "border-blue-500/60 bg-blue-500/10 text-blue-600 dark:text-blue-300"
			: form.driver_type === "tencent_cos"
				? "border-cyan-500/60 bg-cyan-500/10 text-cyan-700 dark:text-cyan-300"
				: form.driver_type === "remote"
					? "border-amber-500/60 bg-amber-500/10 text-amber-600 dark:text-amber-300"
					: "border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300";
	const createNameError =
		isCreateMode && createStep === 1 && createStepTouched && !form.name.trim()
			? t("policy_wizard_name_required")
			: null;
	const createBucketError =
		isCreateMode &&
		createStep === 1 &&
		createStepTouched &&
		isS3CompatibleDriver(form.driver_type) &&
		!form.bucket.trim()
			? t("policy_wizard_bucket_required")
			: null;
	const createRemoteNodeError =
		isCreateMode &&
		createStep === 1 &&
		createStepTouched &&
		form.driver_type === "remote" &&
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
		...(form.driver_type === "local"
			? [
					{
						label: t("content_dedup"),
						value: contentDedupLabel,
					},
				]
			: []),
		...(isS3CompatibleDriver(form.driver_type)
			? [
					{
						label: t("endpoint"),
						value: form.endpoint || t("policy_wizard_default_endpoint"),
					},
					{ label: t("bucket"), value: form.bucket || "—" },
					{
						label: t("s3_upload_strategy"),
						value: s3UploadStrategyLabel,
					},
					{
						label: t("s3_download_strategy"),
						value: s3DownloadStrategyLabel,
					},
					...cosNativeSummaryItems,
				]
			: []),
		...(form.driver_type === "remote"
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
								createRemoteNodeError={createRemoteNodeError}
								createStep={createStep}
								createStepDirection={createStepDirection}
								createSteps={createSteps}
								currentStorageOption={currentStorageOption}
								endpointValidationMessage={endpointValidationMessage}
								form={form}
								onCreateStepChange={onCreateStepChange}
								onDriverTypeChange={onDriverTypeChange}
								onFieldChange={onFieldChange}
								onSyncNormalizedS3Form={onSyncNormalizedS3Form}
								remoteNodes={remoteNodes}
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
								policyCapacity={policyCapacity}
								policyCapacityLoading={policyCapacityLoading}
								onFieldChange={onFieldChange}
								onSyncNormalizedS3Form={onSyncNormalizedS3Form}
								remoteNodes={remoteNodes}
								summaryItems={createSummaryItems}
							/>
						)}
					</div>
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
								createStep === createLastStep ? (
									<>
										<StoragePolicyTestConnectionButton
											onTest={onRunConnectionTest}
											disabled={submitting}
										/>
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
										{createStep === 1 &&
										(isS3CompatibleDriver(form.driver_type) ||
											form.driver_type === "remote") ? (
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
									<StoragePolicyTestConnectionButton
										onTest={onRunConnectionTest}
										disabled={submitting}
									/>
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
