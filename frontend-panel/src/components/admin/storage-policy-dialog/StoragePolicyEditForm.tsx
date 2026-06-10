import { useRef } from "react";
import { useTranslation } from "react-i18next";
import {
	DefaultPolicyToggle,
	LimitsFields,
	LocalContentDedupField,
	PolicyBasePathField,
	PolicyNameField,
	PolicySectionIntro,
	RemoteDownloadStrategyField,
	RemoteNodeField,
	RemoteRulesHelper,
	RemoteUploadStrategyField,
	S3ConnectionFields,
	S3DownloadStrategyField,
	S3UploadStrategyField,
	StorageNativeProcessingField,
	type StoragePolicyDriverOption,
} from "@/components/admin/StoragePolicyDialogFields";
import {
	isS3CompatibleDriver,
	type PolicyFormData,
} from "@/components/admin/storagePolicyDialogShared";
import { AnimatedCollapsible } from "@/components/common/AnimatedCollapsible";
import { InlineConfirm } from "@/components/common/ManagerDialogShell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { RemoteNodeInfo, StoragePolicyCapacityInfo } from "@/types/api";
import type { StoragePolicyFieldChangeHandler } from "./StoragePolicyDialogTypes";

interface StoragePolicyEditFormProps {
	createBucketError: string | null;
	createNameError: string | null;
	createRemoteNodeError: string | null;
	currentDriverBadgeClass: string;
	currentStorageOption: StoragePolicyDriverOption;
	endpointValidationMessage: string | null;
	form: PolicyFormData;
	policyCapacity: StoragePolicyCapacityInfo | null;
	policyCapacityLoading: boolean;
	s3DriverPromotionBlocked: boolean;
	s3DriverPromotionConfirmOpen: boolean;
	s3DriverPromotionSubmitting: boolean;
	s3DriverPromotionTargetLabel: string | null;
	onFieldChange: StoragePolicyFieldChangeHandler;
	onCancelS3DriverPromotion: () => void;
	onConfirmS3DriverPromotion: () => void;
	onRequestS3DriverPromotion: () => void;
	onSyncNormalizedS3Form: () => void;
	remoteNodes: RemoteNodeInfo[];
}

export function StoragePolicyEditForm({
	createBucketError,
	createNameError,
	createRemoteNodeError,
	currentDriverBadgeClass,
	currentStorageOption,
	endpointValidationMessage,
	form,
	policyCapacity,
	policyCapacityLoading,
	s3DriverPromotionBlocked,
	s3DriverPromotionConfirmOpen,
	s3DriverPromotionSubmitting,
	s3DriverPromotionTargetLabel,
	onFieldChange,
	onCancelS3DriverPromotion,
	onConfirmS3DriverPromotion,
	onRequestS3DriverPromotion,
	onSyncNormalizedS3Form,
	remoteNodes,
}: StoragePolicyEditFormProps) {
	const { t } = useTranslation("admin");
	const renderedS3DriverPromotionTargetLabelRef = useRef(
		s3DriverPromotionTargetLabel,
	);
	if (s3DriverPromotionTargetLabel != null) {
		renderedS3DriverPromotionTargetLabelRef.current =
			s3DriverPromotionTargetLabel;
	}
	const renderedPromotionTargetLabel =
		s3DriverPromotionTargetLabel ??
		renderedS3DriverPromotionTargetLabelRef.current;

	return (
		<div data-testid="policy-edit-shell" className="space-y-4">
			<PolicyEditContextBar
				currentDriverBadgeClass={currentDriverBadgeClass}
				currentStorageOption={currentStorageOption}
				form={form}
				policyCapacity={policyCapacity}
				policyCapacityLoading={policyCapacityLoading}
				t={t}
			/>

			<div className="space-y-4">
				<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
					<PolicySectionIntro
						title={t("policy_editor_overview_title")}
						description={t("policy_editor_overview_desc")}
					/>
					<div className="grid gap-5 md:grid-cols-2">
						<PolicyNameField
							form={form}
							error={createNameError}
							t={t}
							onFieldChange={onFieldChange}
						/>
						<PolicyBasePathField
							form={form}
							t={t}
							onFieldChange={onFieldChange}
						/>
					</div>
				</section>

				{isS3CompatibleDriver(form.driver_type) ? (
					<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
						<PolicySectionIntro
							title={t("policy_editor_connection_title")}
							description={t("policy_editor_connection_desc")}
						/>
						<div className="space-y-4">
							<S3ConnectionFields
								form={form}
								bucketError={createBucketError}
								endpointValidationMessage={endpointValidationMessage}
								isCreateMode={false}
								t={t}
								onFieldChange={onFieldChange}
								onSyncNormalizedS3Form={onSyncNormalizedS3Form}
							/>
							<AnimatedCollapsible open={s3DriverPromotionTargetLabel != null}>
								{renderedPromotionTargetLabel ? (
									<S3DriverPromotionPanel
										blocked={s3DriverPromotionBlocked}
										confirmOpen={s3DriverPromotionConfirmOpen}
										submitting={s3DriverPromotionSubmitting}
										targetLabel={renderedPromotionTargetLabel}
										t={t}
										onCancel={onCancelS3DriverPromotion}
										onConfirm={onConfirmS3DriverPromotion}
										onRequest={onRequestS3DriverPromotion}
									/>
								) : null}
							</AnimatedCollapsible>
						</div>
					</section>
				) : form.driver_type === "remote" ? (
					<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
						<PolicySectionIntro
							title={t("policy_editor_remote_title")}
							description={t("policy_editor_remote_desc")}
						/>
						<div className="space-y-4">
							<RemoteNodeField
								form={form}
								error={createRemoteNodeError}
								remoteNodes={remoteNodes}
								t={t}
								onFieldChange={onFieldChange}
							/>
							<RemoteRulesHelper t={t} />
						</div>
					</section>
				) : null}

				<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
					<PolicySectionIntro
						title={t("policy_editor_rules_title")}
						description={t("policy_editor_rules_desc")}
					/>
					<div className="space-y-4">
						{isS3CompatibleDriver(form.driver_type) ? (
							<>
								<S3UploadStrategyField
									form={form}
									t={t}
									onFieldChange={onFieldChange}
								/>
								<S3DownloadStrategyField
									form={form}
									t={t}
									onFieldChange={onFieldChange}
								/>
							</>
						) : form.driver_type === "remote" ? (
							<>
								<RemoteDownloadStrategyField
									form={form}
									t={t}
									onFieldChange={onFieldChange}
								/>
								<RemoteUploadStrategyField
									form={form}
									t={t}
									onFieldChange={onFieldChange}
								/>
							</>
						) : (
							<LocalContentDedupField
								form={form}
								t={t}
								onFieldChange={onFieldChange}
							/>
						)}
						<LimitsFields form={form} t={t} onFieldChange={onFieldChange} />
						<DefaultPolicyToggle
							form={form}
							t={t}
							onFieldChange={onFieldChange}
						/>
					</div>
				</section>

				{form.driver_type === "tencent_cos" ? (
					<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
						<PolicySectionIntro
							title={t("policy_storage_native_section_title")}
							description={t("policy_storage_native_section_desc")}
						/>
						<StorageNativeProcessingField
							form={form}
							t={t}
							onFieldChange={onFieldChange}
						/>
					</section>
				) : null}
			</div>
		</div>
	);
}

function PolicyEditContextBar({
	currentDriverBadgeClass,
	currentStorageOption,
	form,
	policyCapacity,
	policyCapacityLoading,
	t,
}: {
	currentDriverBadgeClass: string;
	currentStorageOption: StoragePolicyDriverOption;
	form: PolicyFormData;
	policyCapacity: StoragePolicyCapacityInfo | null;
	policyCapacityLoading: boolean;
	t: (key: string, values?: Record<string, number | string>) => string;
}) {
	const capacity = policyCapacity?.capacity;
	const blobTotalBytes = policyCapacity?.blob_total_bytes;
	const blobCount = policyCapacity?.blob_count;
	const displayName = form.name.trim() || t("new_policy");
	const displayBasePath =
		form.base_path.trim() ||
		(form.driver_type === "local" ? "./data" : t("core:root"));
	const capacityStatus = policyCapacityLoading
		? t("policy_capacity_checking")
		: capacity
			? t(`policy_capacity_status_${capacity.status}`)
			: t("policy_capacity_status_unavailable");
	const capacityDescription = policyCapacityLoading
		? t("policy_capacity_loading")
		: typeof blobTotalBytes === "number"
			? t("policy_edit_usage_summary", {
					size: formatBytes(blobTotalBytes),
					count: typeof blobCount === "number" ? blobCount : 0,
				})
			: capacity?.status === "unsupported"
				? t("policy_capacity_unsupported_desc")
				: t("policy_capacity_unavailable_desc");
	const availableDescription =
		capacity &&
		typeof capacity.available_bytes === "number" &&
		typeof capacity.total_bytes === "number"
			? t("policy_edit_capacity_available_summary", {
					available: formatBytes(capacity.available_bytes),
					total: formatBytes(capacity.total_bytes),
				})
			: null;

	return (
		<section
			data-testid="policy-edit-context-bar"
			className="rounded-2xl border border-border/70 bg-muted/20 p-4"
		>
			<div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_minmax(220px,0.85fr)]">
				<div className="min-w-0">
					<p className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
						{t("policy_edit_context_title")}
					</p>
					<h3
						data-testid="policy-edit-context-name"
						className="mt-1 truncate text-lg font-semibold text-foreground"
					>
						{displayName}
					</h3>
					<div className="mt-2 flex flex-wrap items-center gap-2">
						<Badge
							variant="outline"
							data-testid="policy-edit-driver-badge"
							className={cn("shadow-sm", currentDriverBadgeClass)}
						>
							{currentStorageOption.title}
						</Badge>
						<span
							className={cn(
								"rounded-full border px-2 py-0.5 text-xs font-medium",
								form.is_default
									? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
									: "border-border bg-background/80 text-muted-foreground",
							)}
						>
							{form.is_default
								? t("policy_edit_default_enabled")
								: t("policy_edit_default_disabled")}
						</span>
					</div>
					<p className="mt-2 truncate text-sm text-muted-foreground">
						{t("base_path")}: {displayBasePath}
					</p>
					<p className="mt-1 text-sm leading-6 text-muted-foreground">
						{isS3CompatibleDriver(form.driver_type)
							? t("policy_edit_context_s3_desc")
							: form.driver_type === "remote"
								? t("policy_edit_context_remote_desc")
								: t("policy_edit_context_local_desc")}
					</p>
				</div>

				<div
					data-testid="policy-edit-capacity-summary"
					className="min-w-0 border-border/70 md:border-l md:pl-4"
				>
					<div className="flex items-start justify-between gap-3">
						<div className="min-w-0">
							<p className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
								{t("policy_capacity_title")}
							</p>
							<p className="mt-1 text-sm font-medium text-foreground">
								{capacityDescription}
							</p>
						</div>
						<span className="shrink-0 rounded-full border border-border bg-background/80 px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
							{capacityStatus}
						</span>
					</div>
					{availableDescription ? (
						<p className="mt-2 text-xs text-muted-foreground">
							{availableDescription}
						</p>
					) : null}
				</div>
			</div>
		</section>
	);
}

function S3DriverPromotionPanel({
	blocked,
	confirmOpen,
	submitting,
	targetLabel,
	t,
	onCancel,
	onConfirm,
	onRequest,
}: {
	blocked: boolean;
	confirmOpen: boolean;
	submitting: boolean;
	targetLabel: string;
	t: (key: string, values?: Record<string, number | string>) => string;
	onCancel: () => void;
	onConfirm: () => void;
	onRequest: () => void;
}) {
	return (
		<div className="rounded-lg border border-amber-500/25 bg-amber-500/5 p-3">
			<div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
				<div className="min-w-0 space-y-1">
					<div className="flex items-center gap-2 text-sm font-medium">
						<Icon
							name="Shield"
							className="size-4 shrink-0 text-amber-600 dark:text-amber-300"
						/>
						<span>{t("policy_s3_driver_promotion_title")}</span>
					</div>
					<p className="text-xs text-muted-foreground">
						{t("policy_s3_driver_promotion_desc", {
							driver: targetLabel,
						})}
					</p>
					{blocked ? (
						<p className="text-xs text-amber-700 dark:text-amber-300">
							{t("policy_s3_driver_promotion_unsaved_blocked")}
						</p>
					) : null}
				</div>
				<Button
					type="button"
					variant="outline"
					className={cn(ADMIN_CONTROL_HEIGHT_CLASS, "shrink-0")}
					disabled={blocked || submitting || confirmOpen}
					onClick={onRequest}
				>
					{t("policy_s3_driver_promotion_action", {
						driver: targetLabel,
					})}
				</Button>
			</div>
			{confirmOpen ? (
				<InlineConfirm className="mt-3">
					<div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
						<div>
							<p className="text-sm font-medium">
								{t("policy_s3_driver_promotion_confirm_title")}
							</p>
							<p className="mt-1 text-xs text-muted-foreground">
								{t("policy_s3_driver_promotion_confirm_desc", {
									driver: targetLabel,
								})}
							</p>
						</div>
						<div className="flex shrink-0 items-center gap-2">
							<Button
								type="button"
								variant="outline"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={onCancel}
								disabled={submitting}
							>
								{t("core:cancel")}
							</Button>
							<Button
								type="button"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={onConfirm}
								disabled={submitting}
							>
								{submitting ? (
									<Icon name="Spinner" className="mr-1 size-3.5 animate-spin" />
								) : null}
								{t("policy_s3_driver_promotion_confirm")}
							</Button>
						</div>
					</div>
				</InlineConfirm>
			) : null}
		</div>
	);
}
