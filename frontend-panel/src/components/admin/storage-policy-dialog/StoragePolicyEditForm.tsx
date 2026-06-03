import { useTranslation } from "react-i18next";
import {
	DefaultPolicyToggle,
	DriverTypeBadge,
	GoogleDriveAuthorizationPanel,
	GoogleDriveConnectionFields,
	LimitsFields,
	LocalContentDedupField,
	PolicyBasePathField,
	PolicyCapacityCard,
	PolicyNameField,
	PolicySectionIntro,
	PolicySummaryCard,
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
import { cn } from "@/lib/utils";
import type { RemoteNodeInfo, StoragePolicyCapacityInfo } from "@/types/api";
import type {
	StoragePolicyFieldChangeHandler,
	StoragePolicySummaryItem,
} from "./StoragePolicyDialogTypes";

interface StoragePolicyEditFormProps {
	createBucketError: string | null;
	createNameError: string | null;
	createRemoteNodeError: string | null;
	currentDriverBadgeClass: string;
	currentStorageOption: StoragePolicyDriverOption;
	editingPolicyId: number | null;
	endpointValidationMessage: string | null;
	form: PolicyFormData;
	policyCapacity: StoragePolicyCapacityInfo | null;
	policyCapacityLoading: boolean;
	onFieldChange: StoragePolicyFieldChangeHandler;
	onSyncNormalizedS3Form: () => void;
	remoteNodes: RemoteNodeInfo[];
	summaryItems: StoragePolicySummaryItem[];
}

export function StoragePolicyEditForm({
	createBucketError,
	createNameError,
	createRemoteNodeError,
	currentDriverBadgeClass,
	currentStorageOption,
	editingPolicyId,
	endpointValidationMessage,
	form,
	policyCapacity,
	policyCapacityLoading,
	onFieldChange,
	onSyncNormalizedS3Form,
	remoteNodes,
	summaryItems,
}: StoragePolicyEditFormProps) {
	const { t } = useTranslation("admin");

	return (
		<div
			data-testid="policy-edit-shell"
			className="grid gap-6 lg:grid-cols-[300px_minmax(0,1fr)]"
		>
			<div className="order-2 space-y-4 lg:sticky lg:top-0 lg:order-1 lg:self-start">
				<PolicySummaryCard
					currentStorageOption={currentStorageOption}
					description={t("policy_editor_summary_desc")}
					formName={form.name}
					items={summaryItems}
					t={t}
				/>
				<PolicyCapacityCard
					capacity={policyCapacity}
					loading={policyCapacityLoading}
					t={t}
				/>
			</div>

			<div className="order-1 space-y-4 lg:order-2">
				<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
					<PolicySectionIntro
						title={t("policy_editor_overview_title")}
						description={t("policy_editor_overview_desc")}
					/>
					<div className="grid gap-5 md:grid-cols-2">
						<div className="space-y-2">
							<p className="text-sm font-medium">{t("driver_type")}</p>
							<div className="flex h-8 items-center">
								<DriverTypeBadge
									title={currentStorageOption.title}
									className={cn("shadow-sm", currentDriverBadgeClass)}
								/>
							</div>
						</div>
						<PolicyNameField
							form={form}
							error={createNameError}
							t={t}
							onFieldChange={onFieldChange}
						/>
						<div className="md:col-span-2">
							<PolicyBasePathField
								form={form}
								t={t}
								onFieldChange={onFieldChange}
							/>
						</div>
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
				) : form.driver_type === "google_drive" ? (
					<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
						<PolicySectionIntro
							title={t("policy_editor_google_drive_title")}
							description={t("policy_editor_google_drive_desc")}
						/>
						<div className="space-y-4">
							<GoogleDriveConnectionFields
								form={form}
								clientIdError={null}
								clientSecretError={null}
								isCreateMode={false}
								t={t}
								onFieldChange={onFieldChange}
							/>
							<GoogleDriveAuthorizationPanel policyId={editingPolicyId} t={t} />
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
						) : form.driver_type === "local" ? (
							<LocalContentDedupField
								form={form}
								t={t}
								onFieldChange={onFieldChange}
							/>
						) : null}
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
