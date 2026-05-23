import { useTranslation } from "react-i18next";
import {
	DefaultPolicyToggle,
	LimitsFields,
	LocalContentDedupField,
	PolicyBasePathField,
	PolicyNameField,
	PolicySummaryCard,
	RemoteDownloadStrategyField,
	RemoteNodeField,
	RemoteRulesHelper,
	RemoteUploadStrategyField,
	S3ConnectionFields,
	S3DownloadStrategyField,
	S3UploadStrategyField,
	StorageDriverVisual,
	type StoragePolicyDriverOption,
} from "@/components/admin/StoragePolicyDialogFields";
import type { PolicyFormData } from "@/components/admin/storagePolicyDialogShared";
import { cn } from "@/lib/utils";
import type { DriverType, RemoteNodeInfo } from "@/types/api";
import type {
	StoragePolicyDialogStep,
	StoragePolicyFieldChangeHandler,
	StoragePolicySummaryItem,
} from "./StoragePolicyDialogTypes";

interface StoragePolicyCreateWizardProps {
	createBucketError: string | null;
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
														className={
															option.type === "local" ? "max-h-8" : "max-h-10"
														}
													/>
												</div>
												<div className="min-w-0 flex-1">
													<div className="flex flex-wrap items-center gap-2">
														<p className="text-base font-semibold">
															{option.title}
														</p>
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
						) : createStep === 1 ? (
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
										t={t}
										onFieldChange={onFieldChange}
									/>
									{form.driver_type === "s3" ? (
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
									) : null}
								</div>
								<div className="rounded-3xl border border-border/70 bg-muted/20 p-5">
									<div className="flex items-center gap-3">
										<div className="flex size-14 items-center justify-center rounded-2xl bg-white shadow-sm ring-1 ring-black/5">
											<StorageDriverVisual option={currentStorageOption} />
										</div>
										<div>
											<p className="text-sm font-medium">
												{currentStorageOption.title}
											</p>
											<p className="text-xs text-muted-foreground">
												{t("policy_wizard_driver_panel_title")}
											</p>
										</div>
									</div>
									<p className="mt-4 text-sm leading-6 text-muted-foreground">
										{currentStorageOption.description}
									</p>
									<p className="mt-4 text-xs leading-5 text-muted-foreground">
										{form.driver_type === "s3"
											? t("policy_wizard_s3_helper")
											: form.driver_type === "remote"
												? t("policy_wizard_remote_helper")
												: t("policy_wizard_local_helper")}
									</p>
								</div>
							</div>
						) : (
							<div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_300px]">
								<div className="space-y-4">
									{form.driver_type === "s3" ? (
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
									) : (
										<LocalContentDedupField
											form={form}
											t={t}
											onFieldChange={onFieldChange}
										/>
									)}
									<LimitsFields
										form={form}
										t={t}
										onFieldChange={onFieldChange}
									/>
									<DefaultPolicyToggle
										form={form}
										t={t}
										onFieldChange={onFieldChange}
									/>
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
						)}
					</div>
				</div>
			</div>
		</div>
	);
}
