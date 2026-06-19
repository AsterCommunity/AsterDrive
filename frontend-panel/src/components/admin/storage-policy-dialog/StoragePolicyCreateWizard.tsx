import { useRef } from "react";
import { useTranslation } from "react-i18next";
import {
	DefaultPolicyToggle,
	LimitsFields,
	LocalContentDedupField,
	OneDriveConnectionFields,
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
import { AnimatedCollapsible } from "@/components/common/AnimatedCollapsible";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import type {
	DriverType,
	RemoteNodeInfo,
	StorageConnectorDescriptor,
} from "@/types/api";
import {
	supportsApplicationCredentials,
	supportsContentDedupPolicyOption,
	supportsObjectStorageConnection,
	supportsOneDrivePolicyOptions,
	supportsRemoteNodeBinding,
	supportsS3TransferStrategy,
	supportsStorageNativeProcessing,
} from "./descriptorPredicates";
import type { PolicyFormData } from "./formTypes";
import type {
	StoragePolicyDialogStep,
	StoragePolicyFieldChangeHandler,
	StoragePolicySummaryItem,
} from "./StoragePolicyDialogTypes";

interface StoragePolicyCreateWizardProps {
	createBucketError: string | null;
	createNameError: string | null;
	createOneDriveClientIdError: string | null;
	createOneDriveClientSecretError: string | null;
	createRemoteNodeError: string | null;
	createStep: number;
	createStepDirection: "idle" | "forward" | "backward";
	createSteps: StoragePolicyDialogStep[];
	currentStorageOption: StoragePolicyDriverOption;
	endpointValidationMessage: string | null;
	form: PolicyFormData;
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	onCreateStepChange: (step: number) => void;
	onDriverTypeChange: (driverType: DriverType) => void;
	onFieldChange: StoragePolicyFieldChangeHandler;
	onApplyS3CompatibleDriverSuggestion: () => void;
	onSyncNormalizedS3Form: () => void;
	remoteNodes: RemoteNodeInfo[];
	s3CompatibleDriverSuggestionTargetLabel: string | null;
	stepAnimationKey: string;
	storageOptions: StoragePolicyDriverOption[];
	summaryItems: StoragePolicySummaryItem[];
}

export function StoragePolicyCreateWizard({
	createBucketError,
	createNameError,
	createOneDriveClientIdError,
	createOneDriveClientSecretError,
	createRemoteNodeError,
	createStep,
	createStepDirection,
	createSteps,
	currentStorageOption,
	endpointValidationMessage,
	form,
	storageDriverDescriptor,
	onCreateStepChange,
	onDriverTypeChange,
	onFieldChange,
	onApplyS3CompatibleDriverSuggestion,
	onSyncNormalizedS3Form,
	remoteNodes,
	s3CompatibleDriverSuggestionTargetLabel,
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
								onCreateStepChange={onCreateStepChange}
								onDriverTypeChange={onDriverTypeChange}
								storageOptions={storageOptions}
							/>
						) : createStep === 1 ? (
							<ConnectionStep
								createBucketError={createBucketError}
								createNameError={createNameError}
								createOneDriveClientIdError={createOneDriveClientIdError}
								createOneDriveClientSecretError={
									createOneDriveClientSecretError
								}
								createRemoteNodeError={createRemoteNodeError}
								currentStorageOption={currentStorageOption}
								endpointValidationMessage={endpointValidationMessage}
								form={form}
								storageDriverDescriptor={storageDriverDescriptor}
								s3CompatibleDriverSuggestionTargetLabel={
									s3CompatibleDriverSuggestionTargetLabel
								}
								onFieldChange={onFieldChange}
								onApplyS3CompatibleDriverSuggestion={
									onApplyS3CompatibleDriverSuggestion
								}
								onSyncNormalizedS3Form={onSyncNormalizedS3Form}
								remoteNodes={remoteNodes}
								t={t}
							/>
						) : (
							<BehaviorStep
								createRemoteNodeError={createRemoteNodeError}
								currentStorageOption={currentStorageOption}
								form={form}
								storageDriverDescriptor={storageDriverDescriptor}
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
	onCreateStepChange: (step: number) => void;
	onDriverTypeChange: (driverType: DriverType) => void;
	storageOptions: StoragePolicyDriverOption[];
}

function DriverSelectionStep({
	form,
	onCreateStepChange,
	onDriverTypeChange,
	storageOptions,
}: DriverSelectionStepProps) {
	return (
		<div>
			<div className="grid gap-3 md:grid-cols-2">
				{storageOptions.map((option) => (
					<button
						type="button"
						key={option.type}
						aria-pressed={form.driver_type === option.type}
						onClick={() => {
							onDriverTypeChange(option.type);
							onCreateStepChange(1);
						}}
						className={cn(
							"rounded-2xl border border-border p-4 text-left transition hover:border-primary/40 hover:bg-muted/20 focus-visible:border-ring focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/30",
							form.driver_type === option.type
								? "bg-muted/15"
								: "bg-background",
						)}
					>
						<div className="flex items-start gap-4">
							<div className="flex size-14 shrink-0 items-center justify-center rounded-2xl bg-white shadow-sm ring-1 ring-black/5">
								<StorageDriverVisual
									option={option}
									className={option.type === "local" ? "max-h-7" : "max-h-9"}
								/>
							</div>
							<div className="min-w-0 flex-1">
								<div className="flex flex-wrap items-center gap-2">
									<p className="text-base font-semibold">{option.title}</p>
								</div>
								<p className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">
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
	createNameError: string | null;
	createOneDriveClientIdError: string | null;
	createOneDriveClientSecretError: string | null;
	createRemoteNodeError: string | null;
	currentStorageOption: StoragePolicyDriverOption;
	endpointValidationMessage: string | null;
	form: PolicyFormData;
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	s3CompatibleDriverSuggestionTargetLabel: string | null;
	onApplyS3CompatibleDriverSuggestion: () => void;
	onFieldChange: StoragePolicyFieldChangeHandler;
	onSyncNormalizedS3Form: () => void;
	remoteNodes: RemoteNodeInfo[];
	t: Translate;
}

function ConnectionStep({
	createBucketError,
	createNameError,
	createOneDriveClientIdError,
	createOneDriveClientSecretError,
	createRemoteNodeError,
	currentStorageOption,
	endpointValidationMessage,
	form,
	storageDriverDescriptor,
	s3CompatibleDriverSuggestionTargetLabel,
	onApplyS3CompatibleDriverSuggestion,
	onFieldChange,
	onSyncNormalizedS3Form,
	remoteNodes,
	t,
}: ConnectionStepProps) {
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
				<PolicyBasePathField
					form={form}
					storageDriverDescriptor={storageDriverDescriptor}
					t={t}
					onFieldChange={onFieldChange}
				/>
				{canUseObjectStorageConnection ? (
					<S3ConnectionFields
						form={form}
						bucketError={createBucketError}
						endpointValidationMessage={endpointValidationMessage}
						isCreateMode
						showCreateValidation
						storageDriverDescriptor={storageDriverDescriptor}
						t={t}
						onFieldChange={onFieldChange}
						onSyncNormalizedS3Form={onSyncNormalizedS3Form}
					/>
				) : canUseRemoteNodeBinding ? (
					<RemoteNodeField
						form={form}
						error={createRemoteNodeError}
						remoteNodes={remoteNodes}
						showCreateValidation
						t={t}
						onFieldChange={onFieldChange}
					/>
				) : canUseOneDriveConnection ? (
					<OneDriveConnectionFields
						clientIdError={createOneDriveClientIdError}
						clientSecretError={createOneDriveClientSecretError}
						form={form}
						mode="create"
						showApplicationFields={canUseApplicationCredentials}
						showCreateValidation
						showPolicyOptionFields={canUseOneDrivePolicyOptions}
						t={t}
						onFieldChange={onFieldChange}
					/>
				) : null}
			</div>
			<DriverHelperPanel
				currentStorageOption={currentStorageOption}
				driverType={form.driver_type}
				storageDriverDescriptor={storageDriverDescriptor}
				s3CompatibleDriverSuggestionTargetLabel={
					s3CompatibleDriverSuggestionTargetLabel
				}
				onApplyS3CompatibleDriverSuggestion={
					onApplyS3CompatibleDriverSuggestion
				}
				t={t}
			/>
		</div>
	);
}

interface DriverHelperPanelProps {
	currentStorageOption: StoragePolicyDriverOption;
	driverType: DriverType;
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	s3CompatibleDriverSuggestionTargetLabel: string | null;
	onApplyS3CompatibleDriverSuggestion: () => void;
	t: Translate;
}

function DriverHelperPanel({
	currentStorageOption,
	driverType,
	storageDriverDescriptor,
	s3CompatibleDriverSuggestionTargetLabel,
	onApplyS3CompatibleDriverSuggestion,
	t,
}: DriverHelperPanelProps) {
	const renderedS3CompatibleDriverSuggestionTargetLabelRef = useRef(
		s3CompatibleDriverSuggestionTargetLabel,
	);
	if (s3CompatibleDriverSuggestionTargetLabel != null) {
		renderedS3CompatibleDriverSuggestionTargetLabelRef.current =
			s3CompatibleDriverSuggestionTargetLabel;
	}
	const showSpecializedDriverSuggestion =
		driverType === "s3" && s3CompatibleDriverSuggestionTargetLabel != null;
	const renderedSuggestionTargetLabel =
		s3CompatibleDriverSuggestionTargetLabel ??
		renderedS3CompatibleDriverSuggestionTargetLabelRef.current;
	const helperKey = getDriverHelperKey(storageDriverDescriptor);

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
				{t(helperKey)}
			</p>
			<AnimatedCollapsible
				open={showSpecializedDriverSuggestion}
				contentClassName="pt-4"
			>
				{renderedSuggestionTargetLabel ? (
					<div className="rounded-xl border border-amber-500/25 bg-amber-500/5 p-3">
						<p className="text-xs font-medium text-amber-800 dark:text-amber-200">
							{t("policy_s3_driver_suggestion_title", {
								driver: renderedSuggestionTargetLabel,
							})}
						</p>
						<p className="mt-1 text-xs leading-5 text-muted-foreground">
							{t("policy_s3_driver_suggestion_desc", {
								driver: renderedSuggestionTargetLabel,
							})}
						</p>
						<Button
							type="button"
							variant="outline"
							className="mt-3 h-8 border-amber-500/30 bg-background/80 px-2.5 text-xs text-amber-800 hover:bg-amber-500/10 dark:text-amber-200"
							onClick={onApplyS3CompatibleDriverSuggestion}
						>
							<Icon name="ArrowsClockwise" className="mr-1 size-3.5" />
							{t("policy_s3_driver_suggestion_action", {
								driver: renderedSuggestionTargetLabel,
							})}
						</Button>
					</div>
				) : null}
			</AnimatedCollapsible>
		</div>
	);
}

function getDriverHelperKey(descriptor: StorageConnectorDescriptor | null) {
	if (descriptor?.ui?.helper_key) {
		return descriptor.ui.helper_key;
	}
	if (supportsObjectStorageConnection(descriptor)) {
		const endpointField = descriptor?.fields.find(
			(field) => field.scope === "connection" && field.name === "endpoint",
		);
		if (endpointField?.help_key) {
			return endpointField.help_key;
		}
		return "policy_wizard_s3_helper";
	}
	if (supportsRemoteNodeBinding(descriptor)) {
		return "policy_wizard_remote_helper";
	}
	if (
		supportsApplicationCredentials(descriptor) ||
		supportsOneDrivePolicyOptions(descriptor)
	) {
		return "policy_wizard_onedrive_helper";
	}
	return "policy_wizard_local_helper";
}

interface BehaviorStepProps {
	createRemoteNodeError: string | null;
	currentStorageOption: StoragePolicyDriverOption;
	form: PolicyFormData;
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	onFieldChange: StoragePolicyFieldChangeHandler;
	remoteNodes: RemoteNodeInfo[];
	summaryItems: StoragePolicySummaryItem[];
	t: Translate;
}

function BehaviorStep({
	createRemoteNodeError,
	currentStorageOption,
	form,
	storageDriverDescriptor,
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
					storageDriverDescriptor={storageDriverDescriptor}
					onFieldChange={onFieldChange}
					remoteNodes={remoteNodes}
					t={t}
				/>
				<LimitsFields form={form} t={t} onFieldChange={onFieldChange} />
				<DefaultPolicyToggle form={form} t={t} onFieldChange={onFieldChange} />
				{supportsStorageNativeProcessing(storageDriverDescriptor) ? (
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
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	onFieldChange: StoragePolicyFieldChangeHandler;
	remoteNodes: RemoteNodeInfo[];
	t: Translate;
}

function DriverBehaviorFields({
	createRemoteNodeError,
	form,
	storageDriverDescriptor,
	onFieldChange,
	remoteNodes,
	t,
}: DriverBehaviorFieldsProps) {
	if (supportsS3TransferStrategy(storageDriverDescriptor)) {
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

	if (supportsRemoteNodeBinding(storageDriverDescriptor)) {
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

	if (
		supportsApplicationCredentials(storageDriverDescriptor) ||
		supportsOneDrivePolicyOptions(storageDriverDescriptor)
	) {
		return (
			<div className="rounded-2xl border border-dashed border-border/80 bg-muted/20 p-4 text-sm leading-6 text-muted-foreground">
				{t("policy_wizard_onedrive_rules_helper")}
			</div>
		);
	}

	return supportsContentDedupPolicyOption(storageDriverDescriptor) ? (
		<LocalContentDedupField form={form} t={t} onFieldChange={onFieldChange} />
	) : null;
}
