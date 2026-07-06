import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { RemoteNodeRemoteStorageTargetSection } from "@/components/admin/admin-remote-nodes-page/RemoteNodeRemoteStorageTargetSection";
import {
	DefaultPolicyToggle,
	LimitsFields,
	LocalContentDedupField,
	ObjectStorageConnectionFields,
	ObjectStorageDownloadStrategyField,
	ObjectStorageUploadStrategyField,
	OneDriveConnectionFields,
	PolicyBasePathField,
	PolicyNameField,
	PolicySectionIntro,
	PolicySummaryCard,
	RemoteDownloadStrategyField,
	RemoteNodeField,
	RemoteRulesHelper,
	RemoteUploadStrategyField,
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
	RemoteCreateStorageTargetRequest,
	RemoteNodeInfo,
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetInfo,
	StorageConnectorDescriptor,
} from "@/types/api";
import {
	supportsApplicationCredentials,
	supportsContentDedupPolicyOption,
	supportsObjectStorageConnection,
	supportsObjectStorageTransferStrategy,
	supportsOneDrivePolicyOptions,
	supportsRemoteNodeBinding,
	supportsStaticSecretConnection,
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
	createRemoteTargetError: string | null;
	createStep: number;
	createStepDirection: "idle" | "forward" | "backward";
	createSteps: StoragePolicyDialogStep[];
	currentStorageOption: StoragePolicyDriverOption;
	endpointValidationMessage: string | null;
	form: PolicyFormData;
	storageDriverDescriptorsError: string | null;
	storageDriverDescriptorsLoading: boolean;
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	onCreateStepChange: (step: number) => void;
	onDriverTypeChange: (driverType: DriverType) => void;
	onFieldChange: StoragePolicyFieldChangeHandler;
	onCreateRemoteStorageTarget: (
		payload: RemoteCreateStorageTargetRequest,
	) => Promise<void>;
	onApplyS3CompatibleDriverSuggestion: () => void;
	onSyncNormalizedObjectStorageForm: () => void;
	remoteNodes: RemoteNodeInfo[];
	remoteStorageTargetDriverDescriptors: RemoteStorageTargetDriverDescriptor[];
	remoteStorageTargetDriverDescriptorsError: string | null;
	remoteStorageTargetDriverDescriptorsLoading: boolean;
	remoteStorageTargets: RemoteStorageTargetInfo[];
	remoteStorageTargetsError: string | null;
	remoteStorageTargetsLoading: boolean;
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
	createRemoteTargetError,
	createStep,
	createStepDirection,
	createSteps,
	currentStorageOption,
	endpointValidationMessage,
	form,
	storageDriverDescriptorsError,
	storageDriverDescriptorsLoading,
	storageDriverDescriptor,
	onCreateStepChange,
	onDriverTypeChange,
	onFieldChange,
	onCreateRemoteStorageTarget,
	onApplyS3CompatibleDriverSuggestion,
	onSyncNormalizedObjectStorageForm,
	remoteNodes,
	remoteStorageTargetDriverDescriptors,
	remoteStorageTargetDriverDescriptorsError,
	remoteStorageTargetDriverDescriptorsLoading,
	remoteStorageTargets,
	remoteStorageTargetsError,
	remoteStorageTargetsLoading,
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
								storageDriverDescriptorsError={storageDriverDescriptorsError}
								storageDriverDescriptorsLoading={
									storageDriverDescriptorsLoading
								}
								storageOptions={storageOptions}
								t={t}
							/>
						) : createStep === 1 ? (
							<ConnectionStep
								createBucketError={createBucketError}
								createNameError={createNameError}
								createOneDriveClientIdError={createOneDriveClientIdError}
								createOneDriveClientSecretError={
									createOneDriveClientSecretError
								}
								createRemoteTargetError={createRemoteTargetError}
								currentStorageOption={currentStorageOption}
								endpointValidationMessage={endpointValidationMessage}
								form={form}
								storageDriverDescriptor={storageDriverDescriptor}
								s3CompatibleDriverSuggestionTargetLabel={
									s3CompatibleDriverSuggestionTargetLabel
								}
								onCreateRemoteStorageTarget={onCreateRemoteStorageTarget}
								onFieldChange={onFieldChange}
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
								t={t}
							/>
						) : (
							<BehaviorStep
								createRemoteTargetError={createRemoteTargetError}
								currentStorageOption={currentStorageOption}
								form={form}
								storageDriverDescriptor={storageDriverDescriptor}
								onFieldChange={onFieldChange}
								remoteNodes={remoteNodes}
								remoteStorageTargets={remoteStorageTargets}
								remoteStorageTargetsError={remoteStorageTargetsError}
								remoteStorageTargetsLoading={remoteStorageTargetsLoading}
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
	storageDriverDescriptorsError: string | null;
	storageDriverDescriptorsLoading: boolean;
	storageOptions: StoragePolicyDriverOption[];
	t: Translate;
}

function DriverSelectionStep({
	form,
	onCreateStepChange,
	onDriverTypeChange,
	storageDriverDescriptorsError,
	storageDriverDescriptorsLoading,
	storageOptions,
	t,
}: DriverSelectionStepProps) {
	if (storageDriverDescriptorsLoading) {
		return (
			<div className="flex min-h-32 items-center justify-center gap-2 rounded-lg border border-dashed border-border text-sm text-muted-foreground">
				<Icon name="Spinner" className="size-4 animate-spin" />
				<span>{t("core:loading")}</span>
			</div>
		);
	}

	if (storageDriverDescriptorsError != null) {
		return (
			<div className="rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-sm text-destructive">
				{storageDriverDescriptorsError}
			</div>
		);
	}

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
	createRemoteTargetError: string | null;
	currentStorageOption: StoragePolicyDriverOption;
	endpointValidationMessage: string | null;
	form: PolicyFormData;
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	s3CompatibleDriverSuggestionTargetLabel: string | null;
	onCreateRemoteStorageTarget: (
		payload: RemoteCreateStorageTargetRequest,
	) => Promise<void>;
	onApplyS3CompatibleDriverSuggestion: () => void;
	onFieldChange: StoragePolicyFieldChangeHandler;
	onSyncNormalizedObjectStorageForm: () => void;
	remoteNodes: RemoteNodeInfo[];
	remoteStorageTargetDriverDescriptors: RemoteStorageTargetDriverDescriptor[];
	remoteStorageTargetDriverDescriptorsError: string | null;
	remoteStorageTargetDriverDescriptorsLoading: boolean;
	remoteStorageTargets: RemoteStorageTargetInfo[];
	remoteStorageTargetsError: string | null;
	remoteStorageTargetsLoading: boolean;
	t: Translate;
}

function ConnectionStep({
	createBucketError,
	createNameError,
	createOneDriveClientIdError,
	createOneDriveClientSecretError,
	createRemoteTargetError,
	currentStorageOption,
	endpointValidationMessage,
	form,
	storageDriverDescriptor,
	s3CompatibleDriverSuggestionTargetLabel,
	onCreateRemoteStorageTarget,
	onApplyS3CompatibleDriverSuggestion,
	onFieldChange,
	onSyncNormalizedObjectStorageForm,
	remoteNodes,
	remoteStorageTargetDriverDescriptors,
	remoteStorageTargetDriverDescriptorsError,
	remoteStorageTargetDriverDescriptorsLoading,
	remoteStorageTargets,
	remoteStorageTargetsError,
	remoteStorageTargetsLoading,
	t,
}: ConnectionStepProps) {
	const canUseStaticSecretConnection = supportsStaticSecretConnection(
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
				{canUseStaticSecretConnection ? (
					<ObjectStorageConnectionFields
						form={form}
						bucketError={createBucketError}
						endpointValidationMessage={endpointValidationMessage}
						isCreateMode
						showCreateValidation
						storageDriverDescriptor={storageDriverDescriptor}
						t={t}
						onFieldChange={onFieldChange}
						onSyncNormalizedObjectStorageForm={
							onSyncNormalizedObjectStorageForm
						}
					/>
				) : canUseRemoteNodeBinding ? (
					<div className="space-y-4">
						<RemoteNodeField
							form={form}
							error={createRemoteTargetError}
							remoteNodes={remoteNodes}
							remoteStorageTargets={remoteStorageTargets}
							remoteStorageTargetsError={remoteStorageTargetsError}
							remoteStorageTargetsLoading={remoteStorageTargetsLoading}
							showCreateValidation
							t={t}
							onFieldChange={onFieldChange}
						/>
						{form.remote_node_id ? (
							<RemoteNodeRemoteStorageTargetSection
								allowCreate
								createLabelKey="policy_remote_storage_targets_quick_create"
								descriptionKey="policy_remote_storage_targets_view_desc"
								driverDescriptors={remoteStorageTargetDriverDescriptors}
								errorMessage={
									remoteStorageTargetsError ??
									remoteStorageTargetDriverDescriptorsError
								}
								loading={
									remoteStorageTargetsLoading ||
									remoteStorageTargetDriverDescriptorsLoading
								}
								onCreateTarget={onCreateRemoteStorageTarget}
								readOnly
								surface="plain"
								targets={remoteStorageTargets}
								titleKey="policy_remote_storage_targets_view_title"
							/>
						) : null}
					</div>
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
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	s3CompatibleDriverSuggestionTargetLabel: string | null;
	onApplyS3CompatibleDriverSuggestion: () => void;
	t: Translate;
}

function DriverHelperPanel({
	currentStorageOption,
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
	const renderedSuggestionTargetLabel =
		s3CompatibleDriverSuggestionTargetLabel ??
		renderedS3CompatibleDriverSuggestionTargetLabelRef.current;
	const helperKey = getDriverHelperKey(storageDriverDescriptor);
	const hasSpecializedDriverSuggestion =
		s3CompatibleDriverSuggestionTargetLabel != null;

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
				open={hasSpecializedDriverSuggestion}
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
		return "policy_wizard_object_storage_helper";
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
	createRemoteTargetError: string | null;
	currentStorageOption: StoragePolicyDriverOption;
	form: PolicyFormData;
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	onFieldChange: StoragePolicyFieldChangeHandler;
	remoteNodes: RemoteNodeInfo[];
	remoteStorageTargets: RemoteStorageTargetInfo[];
	remoteStorageTargetsError: string | null;
	remoteStorageTargetsLoading: boolean;
	summaryItems: StoragePolicySummaryItem[];
	t: Translate;
}

function BehaviorStep({
	createRemoteTargetError,
	currentStorageOption,
	form,
	storageDriverDescriptor,
	onFieldChange,
	remoteNodes,
	remoteStorageTargets,
	remoteStorageTargetsError,
	remoteStorageTargetsLoading,
	summaryItems,
	t,
}: BehaviorStepProps) {
	return (
		<div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_300px]">
			<div className="space-y-4">
				<DriverBehaviorFields
					createRemoteTargetError={createRemoteTargetError}
					form={form}
					storageDriverDescriptor={storageDriverDescriptor}
					onFieldChange={onFieldChange}
					remoteNodes={remoteNodes}
					remoteStorageTargets={remoteStorageTargets}
					remoteStorageTargetsError={remoteStorageTargetsError}
					remoteStorageTargetsLoading={remoteStorageTargetsLoading}
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
	createRemoteTargetError: string | null;
	form: PolicyFormData;
	storageDriverDescriptor: StorageConnectorDescriptor | null;
	onFieldChange: StoragePolicyFieldChangeHandler;
	remoteNodes: RemoteNodeInfo[];
	remoteStorageTargets: RemoteStorageTargetInfo[];
	remoteStorageTargetsError: string | null;
	remoteStorageTargetsLoading: boolean;
	t: Translate;
}

function DriverBehaviorFields({
	createRemoteTargetError,
	form,
	storageDriverDescriptor,
	onFieldChange,
	remoteNodes,
	remoteStorageTargets,
	remoteStorageTargetsError,
	remoteStorageTargetsLoading,
	t,
}: DriverBehaviorFieldsProps) {
	if (supportsObjectStorageTransferStrategy(storageDriverDescriptor)) {
		return (
			<div className="space-y-4">
				<ObjectStorageUploadStrategyField
					form={form}
					t={t}
					onFieldChange={onFieldChange}
				/>
				<ObjectStorageDownloadStrategyField
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
					error={createRemoteTargetError}
					remoteNodes={remoteNodes}
					remoteStorageTargets={remoteStorageTargets}
					remoteStorageTargetsError={remoteStorageTargetsError}
					remoteStorageTargetsLoading={remoteStorageTargetsLoading}
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
