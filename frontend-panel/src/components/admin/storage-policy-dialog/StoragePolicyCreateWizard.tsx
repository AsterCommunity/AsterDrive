import { useTranslation } from "react-i18next";
import {
	DefaultPolicyToggle,
	GoogleDriveConnectionFields,
	LimitsFields,
	LocalContentDedupField,
	PolicyBasePathField,
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
	StorageDriverVisual,
	StorageNativeProcessingField,
	type StoragePolicyDriverOption,
	type Translate,
} from "@/components/admin/StoragePolicyDialogFields";
import {
	isS3CompatibleDriver,
	type PolicyFormData,
} from "@/components/admin/storagePolicyDialogShared";
import { cn } from "@/lib/utils";
import type { DriverType, RemoteNodeInfo } from "@/types/api";
import type {
	StoragePolicyDialogStep,
	StoragePolicyFieldChangeHandler,
	StoragePolicySummaryItem,
} from "./StoragePolicyDialogTypes";

interface StoragePolicyCreateWizardProps {
	createBucketError: string | null;
	createGoogleDriveClientIdError: string | null;
	createGoogleDriveClientSecretError: string | null;
	createNameError: string | null;
	createRemoteNodeError: string | null;
	createStep: number;
	createStepDirection: "idle" | "forward" | "backward";
	createSteps: StoragePolicyDialogStep[];
	currentStorageOption: StoragePolicyDriverOption;
	endpointValidationMessage: string | null;
	form: PolicyFormData;
	onCreateStepChange: (step: number) => void;
	onDriverTypeChange: (driverType: DriverType) => void;
	onFieldChange: StoragePolicyFieldChangeHandler;
	onSyncNormalizedS3Form: () => void;
	remoteNodes: RemoteNodeInfo[];
	stepAnimationKey: string;
	storageOptions: StoragePolicyDriverOption[];
	summaryItems: StoragePolicySummaryItem[];
}

export function StoragePolicyCreateWizard({
	createBucketError,
	createGoogleDriveClientIdError,
	createGoogleDriveClientSecretError,
	createNameError,
	createRemoteNodeError,
	createStep,
	createStepDirection,
	createSteps,
	currentStorageOption,
	endpointValidationMessage,
	form,
	onCreateStepChange,
	onDriverTypeChange,
	onFieldChange,
	onSyncNormalizedS3Form,
	remoteNodes,
	stepAnimationKey,
	storageOptions,
	summaryItems,
}: StoragePolicyCreateWizardProps) {
	const { t } = useTranslation("admin");
	const createLastStep = createSteps.length - 1;
	const currentCreateStep = createSteps[Math.min(createStep, createLastStep)];

	return (
		<div className="space-y-6">
			<WizardProgress
				createStep={createStep}
				createSteps={createSteps}
				currentCreateStep={currentCreateStep}
				onCreateStepChange={onCreateStepChange}
				t={t}
			/>
			<div className="rounded-2xl border border-border/70 bg-background/70 p-5">
				<div className="relative overflow-hidden">
					<div
						key={stepAnimationKey}
						data-testid="policy-step-panel"
						className={cn(
							createStepDirection === "idle"
								? undefined
								: "animate-in fade-in duration-[360ms] motion-reduce:animate-none",
							createStepDirection === "forward"
								? "slide-in-from-right-6"
								: createStepDirection === "backward"
									? "slide-in-from-left-6"
									: undefined,
						)}
					>
						{createStep === 0 ? (
							<DriverSelectionStep
								form={form}
								onDriverTypeChange={onDriverTypeChange}
								storageOptions={storageOptions}
								t={t}
							/>
						) : createStep === 1 ? (
							<ConnectionStep
								createBucketError={createBucketError}
								createGoogleDriveClientIdError={createGoogleDriveClientIdError}
								createGoogleDriveClientSecretError={
									createGoogleDriveClientSecretError
								}
								createNameError={createNameError}
								createRemoteNodeError={createRemoteNodeError}
								currentStorageOption={currentStorageOption}
								endpointValidationMessage={endpointValidationMessage}
								form={form}
								onFieldChange={onFieldChange}
								onSyncNormalizedS3Form={onSyncNormalizedS3Form}
								remoteNodes={remoteNodes}
								t={t}
							/>
						) : (
							<BehaviorStep
								createRemoteNodeError={createRemoteNodeError}
								currentStorageOption={currentStorageOption}
								form={form}
								onFieldChange={onFieldChange}
								remoteNodes={remoteNodes}
								summaryItems={summaryItems}
								t={t}
							/>
						)}
					</div>
				</div>
			</div>
		</div>
	);
}

interface WizardProgressProps {
	createStep: number;
	createSteps: StoragePolicyDialogStep[];
	currentCreateStep: StoragePolicyDialogStep;
	onCreateStepChange: (step: number) => void;
	t: Translate;
}

function WizardProgress({
	createStep,
	createSteps,
	currentCreateStep,
	onCreateStepChange,
	t,
}: WizardProgressProps) {
	return (
		<div className="space-y-3">
			<div className="rounded-2xl border border-border/70 bg-muted/20 p-3 sm:p-4">
				<div className="flex items-start justify-between gap-3">
					<div className="space-y-1">
						<p className="text-[11px] font-medium uppercase tracking-[0.2em] text-muted-foreground">
							{t("policy_wizard_progress", {
								current: createStep + 1,
								total: createSteps.length,
							})}
						</p>
						<h3 className="text-sm font-semibold sm:text-base">
							{currentCreateStep.title}
						</h3>
						<p className="hidden text-sm text-muted-foreground sm:block">
							{currentCreateStep.description}
						</p>
					</div>
					<div className="hidden text-3xl leading-none font-semibold text-foreground/15 md:block">
						{String(createStep + 1).padStart(2, "0")}
					</div>
				</div>
				<div className="mt-3 h-1.5 overflow-hidden rounded-full bg-muted">
					<div
						className="h-full rounded-full bg-primary transition-all"
						style={{
							width: `${((createStep + 1) / createSteps.length) * 100}%`,
						}}
					/>
				</div>
			</div>

			<div className="hidden gap-2 md:grid md:grid-cols-3">
				{createSteps.map((step, index) => (
					<button
						type="button"
						key={step.title}
						disabled={index > createStep}
						onClick={() => onCreateStepChange(index)}
						className={cn(
							"rounded-xl border px-3 py-2.5 text-left transition",
							index === createStep
								? "border-primary bg-primary/5 shadow-sm"
								: index < createStep
									? "border-border bg-background hover:border-primary/40"
									: "border-border/60 bg-muted/20 text-muted-foreground",
						)}
					>
						<div className="flex items-center gap-2">
							<span className="flex size-6 shrink-0 items-center justify-center rounded-full border border-border/70 bg-background/80 text-[10px] font-semibold tracking-[0.16em] text-muted-foreground">
								{index + 1}
							</span>
							<span className="text-sm font-medium leading-5">
								{step.title}
							</span>
						</div>
					</button>
				))}
			</div>
		</div>
	);
}

interface DriverSelectionStepProps {
	form: PolicyFormData;
	onDriverTypeChange: (driverType: DriverType) => void;
	storageOptions: StoragePolicyDriverOption[];
	t: Translate;
}

function DriverSelectionStep({
	form,
	onDriverTypeChange,
	storageOptions,
	t,
}: DriverSelectionStepProps) {
	return (
		<div className="space-y-4">
			<div className="max-w-2xl">
				<h3 className="text-base font-semibold">
					{t("policy_wizard_choose_driver_title")}
				</h3>
				<p className="mt-1 text-sm text-muted-foreground">
					{t("policy_wizard_choose_driver_desc")}
				</p>
			</div>
			<div className="grid gap-4 md:grid-cols-2">
				{storageOptions.map((option) => (
					<button
						type="button"
						key={option.type}
						aria-pressed={form.driver_type === option.type}
						onClick={() => onDriverTypeChange(option.type)}
						className={cn(
							"rounded-3xl border p-5 text-left transition",
							form.driver_type === option.type
								? "border-primary bg-primary/5 shadow-sm"
								: "border-border bg-background hover:border-primary/40 hover:bg-muted/20",
						)}
					>
						<div className="flex items-start gap-4">
							<div className="flex size-16 shrink-0 items-center justify-center rounded-2xl bg-white shadow-sm ring-1 ring-black/5">
								<StorageDriverVisual
									option={option}
									className={option.type === "local" ? "max-h-8" : "max-h-10"}
								/>
							</div>
							<div className="min-w-0 flex-1">
								<div className="flex flex-wrap items-center gap-2">
									<p className="text-base font-semibold">{option.title}</p>
									{form.driver_type === option.type ? (
										<span className="rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">
											{t("policy_wizard_selected")}
										</span>
									) : null}
								</div>
								<p className="mt-2 text-sm leading-6 text-muted-foreground">
									{option.description}
								</p>
							</div>
						</div>
					</button>
				))}
			</div>
		</div>
	);
}

interface ConnectionStepProps {
	createBucketError: string | null;
	createGoogleDriveClientIdError: string | null;
	createGoogleDriveClientSecretError: string | null;
	createNameError: string | null;
	createRemoteNodeError: string | null;
	currentStorageOption: StoragePolicyDriverOption;
	endpointValidationMessage: string | null;
	form: PolicyFormData;
	onFieldChange: StoragePolicyFieldChangeHandler;
	onSyncNormalizedS3Form: () => void;
	remoteNodes: RemoteNodeInfo[];
	t: Translate;
}

function ConnectionStep({
	createBucketError,
	createGoogleDriveClientIdError,
	createGoogleDriveClientSecretError,
	createNameError,
	createRemoteNodeError,
	currentStorageOption,
	endpointValidationMessage,
	form,
	onFieldChange,
	onSyncNormalizedS3Form,
	remoteNodes,
	t,
}: ConnectionStepProps) {
	return (
		<div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_280px]">
			<div className="space-y-4">
				<PolicyNameField
					form={form}
					error={createNameError}
					showCreateValidation
					t={t}
					onFieldChange={onFieldChange}
				/>
				<PolicyBasePathField form={form} t={t} onFieldChange={onFieldChange} />
				{isS3CompatibleDriver(form.driver_type) ? (
					<S3ConnectionFields
						form={form}
						bucketError={createBucketError}
						endpointValidationMessage={endpointValidationMessage}
						isCreateMode
						showCreateValidation
						t={t}
						onFieldChange={onFieldChange}
						onSyncNormalizedS3Form={onSyncNormalizedS3Form}
					/>
				) : form.driver_type === "remote" ? (
					<RemoteNodeField
						form={form}
						error={createRemoteNodeError}
						remoteNodes={remoteNodes}
						showCreateValidation
						t={t}
						onFieldChange={onFieldChange}
					/>
				) : form.driver_type === "google_drive" ? (
					<GoogleDriveConnectionFields
						form={form}
						clientIdError={createGoogleDriveClientIdError}
						clientSecretError={createGoogleDriveClientSecretError}
						isCreateMode
						showCreateValidation
						t={t}
						onFieldChange={onFieldChange}
					/>
				) : null}
			</div>
			<DriverHelperPanel
				currentStorageOption={currentStorageOption}
				driverType={form.driver_type}
				t={t}
			/>
		</div>
	);
}

interface DriverHelperPanelProps {
	currentStorageOption: StoragePolicyDriverOption;
	driverType: DriverType;
	t: Translate;
}

function DriverHelperPanel({
	currentStorageOption,
	driverType,
	t,
}: DriverHelperPanelProps) {
	return (
		<div className="rounded-3xl border border-border/70 bg-muted/20 p-5">
			<div className="flex items-center gap-3">
				<div className="flex size-14 items-center justify-center rounded-2xl bg-white shadow-sm ring-1 ring-black/5">
					<StorageDriverVisual option={currentStorageOption} />
				</div>
				<div>
					<p className="text-sm font-medium">{currentStorageOption.title}</p>
					<p className="text-xs text-muted-foreground">
						{t("policy_wizard_driver_panel_title")}
					</p>
				</div>
			</div>
			<p className="mt-4 text-sm leading-6 text-muted-foreground">
				{currentStorageOption.description}
			</p>
			<p className="mt-4 text-xs leading-5 text-muted-foreground">
				{isS3CompatibleDriver(driverType)
					? driverType === "tencent_cos"
						? t("policy_wizard_tencent_cos_helper")
						: t("policy_wizard_s3_helper")
					: driverType === "remote"
						? t("policy_wizard_remote_helper")
						: driverType === "google_drive"
							? t("policy_wizard_google_drive_helper")
							: t("policy_wizard_local_helper")}
			</p>
		</div>
	);
}

interface BehaviorStepProps {
	createRemoteNodeError: string | null;
	currentStorageOption: StoragePolicyDriverOption;
	form: PolicyFormData;
	onFieldChange: StoragePolicyFieldChangeHandler;
	remoteNodes: RemoteNodeInfo[];
	summaryItems: StoragePolicySummaryItem[];
	t: Translate;
}

function BehaviorStep({
	createRemoteNodeError,
	currentStorageOption,
	form,
	onFieldChange,
	remoteNodes,
	summaryItems,
	t,
}: BehaviorStepProps) {
	return (
		<div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_300px]">
			<div className="space-y-4">
				<DriverBehaviorFields
					createRemoteNodeError={createRemoteNodeError}
					form={form}
					onFieldChange={onFieldChange}
					remoteNodes={remoteNodes}
					t={t}
				/>
				<LimitsFields form={form} t={t} onFieldChange={onFieldChange} />
				<DefaultPolicyToggle form={form} t={t} onFieldChange={onFieldChange} />
				{form.driver_type === "tencent_cos" ? (
					<div className="space-y-3 border-t border-border/70 pt-4">
						<PolicySectionIntro
							title={t("policy_storage_native_section_title")}
							description={t("policy_storage_native_section_desc")}
						/>
						<StorageNativeProcessingField
							form={form}
							t={t}
							onFieldChange={onFieldChange}
						/>
					</div>
				) : null}
			</div>
			<div className="space-y-4 lg:sticky lg:top-0 lg:self-start">
				<PolicySummaryCard
					currentStorageOption={currentStorageOption}
					description={t("policy_wizard_summary_desc")}
					formName={form.name}
					items={summaryItems}
					t={t}
				/>
			</div>
		</div>
	);
}

interface DriverBehaviorFieldsProps {
	createRemoteNodeError: string | null;
	form: PolicyFormData;
	onFieldChange: StoragePolicyFieldChangeHandler;
	remoteNodes: RemoteNodeInfo[];
	t: Translate;
}

function DriverBehaviorFields({
	createRemoteNodeError,
	form,
	onFieldChange,
	remoteNodes,
	t,
}: DriverBehaviorFieldsProps) {
	if (isS3CompatibleDriver(form.driver_type)) {
		return (
			<div className="space-y-4">
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
			</div>
		);
	}

	if (form.driver_type === "remote") {
		return (
			<>
				<RemoteRulesHelper t={t} />
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
				<RemoteNodeField
					form={form}
					error={createRemoteNodeError}
					remoteNodes={remoteNodes}
					t={t}
					onFieldChange={onFieldChange}
				/>
			</>
		);
	}

	if (form.driver_type === "local") {
		return (
			<LocalContentDedupField form={form} t={t} onFieldChange={onFieldChange} />
		);
	}

	return null;
}
