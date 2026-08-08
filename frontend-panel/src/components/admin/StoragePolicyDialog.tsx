import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { getStorageConnectorBadgePresentation } from "@/components/admin/admin-policies-page/policyPresentation";
import { RemoteNodeRemoteStorageTargetSection } from "@/components/admin/admin-remote-nodes-page/RemoteNodeRemoteStorageTargetSection";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { InlineConfirm } from "@/components/common/ManagerDialogShell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon, isIconName } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatBytes, formatDateTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteNodeInfo,
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetInfo,
	StorageConnectorCredentialInfo,
	StorageConnectorCredentialManagementDescriptor,
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
	StorageConnectorFieldValue,
	StorageConnectorTransitionPreview,
	StoragePolicyCapacityInfo,
} from "@/types/api";
import {
	findConnectorFieldByDataSource,
	supportsDraftConnectionTest,
	supportsSavedConnectionTest,
} from "./storage-policy-dialog/descriptorPredicates";
import {
	connectorBooleanValue,
	connectorFormValue,
	connectorNumberValue,
	connectorStringValue,
	type PolicyFormData,
} from "./storage-policy-dialog/formTypes";
import type { StorageConnectorActionValues } from "./storage-policy-dialog/StorageConnectorActionsPanel";
import { StorageConnectorActionsPanel } from "./storage-policy-dialog/StorageConnectorActionsPanel";
import { StorageConnectorFieldsPanel } from "./storage-policy-dialog/StorageConnectorFieldsPanel";
import { StorageConnectorTransitionPanel } from "./storage-policy-dialog/StorageConnectorTransitionPanel";
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
	storageCredentials: StorageConnectorCredentialInfo[];
	storageCredentialsLoading: boolean;
	storageAuthorizationSubmitting: boolean;
	storageCredentialValidationSubmitting: boolean;
	storageAuthorizationRedirectUri: string;
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
	connectorActionConfirmId: string | null;
	connectorActionSubmittingId: string | null;
	connectorActionValues: StorageConnectorActionValues;
	connectorTransitionConfirmKey: string | null;
	connectorTransitionSubmittingKey: string | null;
	connectorTransitions: StorageConnectorTransitionPreview[];
	connectorTransitionsLoading: boolean;
	hasUnsavedChanges: boolean;
	saveAnywayConfirmOpen: boolean;
	onCancelConnectorAction: () => void;
	onCancelConnectorTransition: () => void;
	onOpenChange: (open: boolean) => void;
	onCancelSaveAnyway: () => void;
	onConfirmSaveAnyway: () => void;
	onConfirmConnectorAction: (actionId: string) => void;
	onConfirmConnectorTransition: (
		transition: StorageConnectorTransitionPreview,
	) => void;
	onStartStorageAuthorization: () => void;
	onValidateStorageCredential: () => void;
	onCreateRemoteStorageTarget: (
		payload: RemoteCreateStorageTargetRequest,
	) => Promise<void>;
	onSubmit: () => void;
	onRunConnectionTest: () => Promise<boolean>;
	onFieldChange: <K extends keyof PolicyFormData>(
		key: K,
		value: PolicyFormData[K],
	) => void;
	onConnectorActionValueChange: (
		actionId: string,
		fieldName: string,
		value: StorageConnectorFieldValue | undefined,
	) => void;
	onRequestConnectorAction: (actionId: string) => void;
	onRequestConnectorTransition: (
		transition: StorageConnectorTransitionPreview,
	) => void;
	onConnectorIdChange: (connectorId: string) => void;
	onCreateBack: () => void;
	onCreateStepChange: (step: number) => void;
	onCreateNext: () => void;
	showCloseButton?: boolean;
	forceDefaultPolicy?: boolean;
	presentation?: "dialog" | "setup";
	onSetupLogout?: () => void;
}

export function StoragePolicyDialog({
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
	connectorActionConfirmId,
	connectorActionSubmittingId,
	connectorActionValues,
	connectorTransitionConfirmKey,
	connectorTransitionSubmittingKey,
	connectorTransitions,
	connectorTransitionsLoading,
	hasUnsavedChanges,
	saveAnywayConfirmOpen,
	onCancelConnectorAction,
	onCancelConnectorTransition,
	onOpenChange,
	onCancelSaveAnyway,
	onConfirmSaveAnyway,
	onConfirmConnectorAction,
	onConfirmConnectorTransition,
	onStartStorageAuthorization,
	onValidateStorageCredential,
	onCreateRemoteStorageTarget,
	onSubmit,
	onRunConnectionTest,
	onFieldChange,
	onConnectorActionValueChange,
	onRequestConnectorAction,
	onRequestConnectorTransition,
	onConnectorIdChange,
	onCreateBack,
	onCreateStepChange,
	onCreateNext,
	showCloseButton = true,
	forceDefaultPolicy = false,
	presentation = "dialog",
	onSetupLogout,
}: StoragePolicyDialogProps) {
	const { t } = useTranslation("admin");
	const connectorT = (key: string, values?: Record<string, number | string>) =>
		translateStorageConnectorMessage(
			t,
			storageDriverDescriptor?.connector_id,
			key,
			values,
		);
	const isCreateMode = mode === "create";
	const isSetupPresentation = presentation === "setup";
	const customActions =
		storageDriverDescriptor?.actions.filter(
			(action) => action.kind === "custom",
		) ?? [];
	const authorizationAction = storageDriverDescriptor?.actions.find(
		(action) => action.kind === "authorization",
	);
	const validationAction = storageDriverDescriptor?.actions.find(
		(action) => action.kind === "credential_validation",
	);
	const remoteNodeField = findConnectorFieldByDataSource(
		storageDriverDescriptor,
		"remote_nodes",
	);
	const remoteNodeId = remoteNodeField
		? connectorNumberValue(form, remoteNodeField.name)
		: null;
	const nativeProcessingEnabled = connectorBooleanValue(
		form,
		"storage_native_processing_enabled",
	);
	const descriptorFields =
		storageDriverDescriptor?.fields.filter(
			(field) => field.scope !== "action_input",
		) ?? [];
	const basePathField = descriptorFields.find(
		(field) => field.scope === "connector_config" && field.name === "base_path",
	);
	const connectionFields = descriptorFields.filter(
		(field) => field !== basePathField,
	);
	const createSteps = [
		{
			title: t("policy_wizard_step_storage_title"),
			description: t("policy_wizard_step_storage_desc"),
		},
		{
			title: storageDriverDescriptor
				? connectorT(storageDriverDescriptor.ui.config_step_title_key)
				: t("policy_wizard_step_connection_title"),
			description: storageDriverDescriptor
				? connectorT(storageDriverDescriptor.ui.config_step_description_key)
				: t("policy_wizard_step_connection_desc"),
		},
		{
			title: t("policy_wizard_step_rules_title"),
			description: t("policy_wizard_step_rules_desc"),
		},
	];
	const createLastStep = createSteps.length - 1;
	const canRunDraftConnectionTest = supportsDraftConnectionTest(
		storageDriverDescriptor,
	);
	const canRunConnectionTest = isCreateMode
		? canRunDraftConnectionTest
		: canRunDraftConnectionTest ||
			supportsSavedConnectionTest(storageDriverDescriptor);
	const previousCreateStepRef = useRef(createStep);
	const stepAnimationRef = useRef<{
		direction: "idle" | "forward" | "backward";
		step: number;
	}>({ direction: "idle", step: createStep });
	if (createStep !== previousCreateStepRef.current) {
		stepAnimationRef.current = {
			direction:
				createStep > previousCreateStepRef.current ? "forward" : "backward",
			step: createStep,
		};
		previousCreateStepRef.current = createStep;
	}
	useEffect(() => {
		if (!open || !isCreateMode) {
			previousCreateStepRef.current = 0;
			stepAnimationRef.current = { direction: "idle", step: 0 };
		}
	}, [isCreateMode, open]);
	const summaryItems = buildPolicySummaryItems(
		form,
		storageDriverDescriptor,
		remoteNodes,
		remoteStorageTargets,
		t,
	);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent
				showCloseButton={showCloseButton}
				overlayClassName={
					isSetupPresentation
						? "bg-background backdrop-blur-none dark:bg-background"
						: undefined
				}
				className={cn(
					"flex max-h-[min(90vh,calc(100vh-2rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-[calc(100%-2rem)] lg:max-w-4xl",
					isSetupPresentation && "shadow-xl",
				)}
			>
				{isSetupPresentation ? (
					<div className="flex shrink-0 items-center justify-between gap-4 border-b border-border/70 px-6 py-4">
						<AsterDriveWordmark
							alt="AsterDrive"
							className="h-8 w-auto max-w-44"
						/>
						{onSetupLogout ? (
							<Button
								type="button"
								variant="outline"
								size="sm"
								onClick={onSetupLogout}
							>
								{t("core:logout")}
							</Button>
						) : null}
					</div>
				) : null}
				<DialogHeader
					className={cn(
						"shrink-0 px-6 pt-5 pb-0",
						showCloseButton ? "pr-14" : "pr-6",
					)}
				>
					{isSetupPresentation ? (
						<p className="text-xs font-semibold tracking-[0.18em] text-primary uppercase">
							{t("auth:storage_setup_eyebrow")}
						</p>
					) : null}
					<DialogTitle>
						{isSetupPresentation
							? t("auth:storage_setup_page_title")
							: isCreateMode
								? t("create_policy")
								: t("edit_policy")}
					</DialogTitle>
					{isSetupPresentation ? (
						<DialogDescription>
							{t("auth:storage_setup_page_desc")}
						</DialogDescription>
					) : isCreateMode ? null : (
						<DialogDescription>{t("policies_intro")}</DialogDescription>
					)}
				</DialogHeader>
				<form
					onSubmit={(event) => event.preventDefault()}
					autoComplete="off"
					className="flex min-h-0 flex-1 flex-col overflow-hidden"
				>
					<div className="min-h-0 flex-1 overflow-y-auto px-6 pt-6 pb-5">
						{isCreateMode ? (
							<div className="space-y-6">
								<WizardProgress
									createStep={createStep}
									steps={createSteps}
									onStepChange={onCreateStepChange}
								/>
								<div className="rounded-2xl border border-border/70 bg-background/70 p-5">
									<div className="relative overflow-hidden">
										<div
											key={`${stepAnimationRef.current.step}-${stepAnimationRef.current.direction}`}
											data-testid="policy-step-panel"
											className={cn(
												stepAnimationRef.current.direction !== "idle" &&
													"animate-in fade-in duration-[360ms] motion-reduce:animate-none",
												stepAnimationRef.current.direction === "forward" &&
													"slide-in-from-right-6",
												stepAnimationRef.current.direction === "backward" &&
													"slide-in-from-left-6",
											)}
										>
											{createStep === 0 ? (
												<ConnectorSelection
													descriptors={storageDriverDescriptors}
													error={storageDriverDescriptorsError}
													loading={storageDriverDescriptorsLoading}
													selectedId={form.connector_id}
													setup={isSetupPresentation}
													onSelect={(connectorId) => {
														onConnectorIdChange(connectorId);
														onCreateStepChange(1);
													}}
												/>
											) : createStep === 1 ? (
												<div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_280px]">
													<div className="space-y-4">
														<PolicyNameField
															form={form}
															showError={createStepTouched}
															onChange={(value) => onFieldChange("name", value)}
														/>
														<StorageConnectorFieldsPanel
															descriptor={storageDriverDescriptor}
															form={form}
															mode="create"
															remoteNodes={remoteNodes}
															remoteStorageTargets={remoteStorageTargets}
															showRequiredErrors={createStepTouched}
															t={t}
															onFieldChange={onFieldChange}
														/>
														<StorageConnectorTransitionPanel
															confirmKey={connectorTransitionConfirmKey}
															loading={connectorTransitionsLoading}
															mode="create"
															submittingKey={connectorTransitionSubmittingKey}
															t={t}
															transitions={connectorTransitions}
															unsavedChanges={false}
															onCancel={onCancelConnectorTransition}
															onConfirm={onConfirmConnectorTransition}
															onRequest={onRequestConnectorTransition}
														/>
														{endpointValidationMessage ? (
															<p className="text-xs text-destructive">
																{endpointValidationMessage}
															</p>
														) : null}
														{remoteNodeId ? (
															<RemoteTargets
																driverDescriptors={
																	remoteStorageTargetDriverDescriptors
																}
																driverError={
																	remoteStorageTargetDriverDescriptorsError
																}
																driverLoading={
																	remoteStorageTargetDriverDescriptorsLoading
																}
																targets={remoteStorageTargets}
																targetsError={remoteStorageTargetsError}
																targetsLoading={remoteStorageTargetsLoading}
																onCreate={onCreateRemoteStorageTarget}
															/>
														) : null}
													</div>
													<ConnectorHelper
														descriptor={storageDriverDescriptor}
													/>
												</div>
											) : (
												<div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_300px]">
													<div className="space-y-4">
														<PolicyRules
															form={form}
															forceDefault={forceDefaultPolicy}
															nativeProcessingEnabled={nativeProcessingEnabled}
															onFieldChange={onFieldChange}
														/>
														<StorageConnectorActionsPanel
															actions={customActions}
															remoteNodes={remoteNodes}
															remoteStorageTargets={remoteStorageTargets}
															connectorId={
																storageDriverDescriptor?.connector_id
															}
															confirmActionId={connectorActionConfirmId}
															submittingActionId={connectorActionSubmittingId}
															t={t}
															values={connectorActionValues}
															onCancel={onCancelConnectorAction}
															onConfirm={onConfirmConnectorAction}
															onRequest={onRequestConnectorAction}
															onValueChange={onConnectorActionValueChange}
														/>
													</div>
													<PolicySummary
														descriptor={storageDriverDescriptor}
														name={form.name}
														items={summaryItems}
													/>
												</div>
											)}
										</div>
									</div>
								</div>
							</div>
						) : (
							<div data-testid="policy-edit-shell" className="space-y-4">
								<PolicyEditContextBar
									capacity={policyCapacity}
									descriptor={storageDriverDescriptor}
									form={form}
									loading={policyCapacityLoading}
								/>
								<StorageConnectorTransitionPanel
									confirmKey={connectorTransitionConfirmKey}
									loading={connectorTransitionsLoading}
									mode="edit"
									submittingKey={connectorTransitionSubmittingKey}
									t={t}
									transitions={connectorTransitions}
									unsavedChanges={hasUnsavedChanges}
									onCancel={onCancelConnectorTransition}
									onConfirm={onConfirmConnectorTransition}
									onRequest={onRequestConnectorTransition}
								/>
								<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
									<SectionTitle
										title={t("policy_editor_overview_title")}
										description={t("policy_editor_overview_desc")}
									/>
									<div className="mt-5 grid gap-5 md:grid-cols-2">
										<PolicyNameField
											form={form}
											showError={false}
											onChange={(value) => onFieldChange("name", value)}
										/>
										{basePathField ? (
											<StorageConnectorFieldsPanel
												descriptor={storageDriverDescriptor}
												fields={[basePathField]}
												form={form}
												mode="edit"
												remoteNodes={remoteNodes}
												remoteStorageTargets={remoteStorageTargets}
												showRequiredErrors={false}
												t={t}
												onFieldChange={onFieldChange}
											/>
										) : null}
									</div>
								</section>
								{connectionFields.length > 0 || remoteNodeId ? (
									<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
										<SectionTitle
											title={
												storageDriverDescriptor
													? connectorT(
															storageDriverDescriptor.ui.config_step_title_key,
														)
													: t("policy_editor_connection_title")
											}
											description={
												storageDriverDescriptor
													? connectorT(
															storageDriverDescriptor.ui
																.config_step_description_key,
														)
													: t("policy_editor_connection_desc")
											}
										/>
										<div className="mt-5 space-y-4">
											<StorageConnectorFieldsPanel
												descriptor={storageDriverDescriptor}
												fields={connectionFields}
												form={form}
												mode="edit"
												remoteNodes={remoteNodes}
												remoteStorageTargets={remoteStorageTargets}
												showRequiredErrors={false}
												t={t}
												onFieldChange={onFieldChange}
											/>
											{endpointValidationMessage ? (
												<p className="text-xs text-destructive">
													{endpointValidationMessage}
												</p>
											) : null}
											{remoteNodeId ? (
												<RemoteTargets
													driverDescriptors={
														remoteStorageTargetDriverDescriptors
													}
													driverError={
														remoteStorageTargetDriverDescriptorsError
													}
													driverLoading={
														remoteStorageTargetDriverDescriptorsLoading
													}
													targets={remoteStorageTargets}
													targetsError={remoteStorageTargetsError}
													targetsLoading={remoteStorageTargetsLoading}
													onCreate={onCreateRemoteStorageTarget}
												/>
											) : null}
											<ConnectorManagement
												management={
													storageDriverDescriptor?.credential_management ?? null
												}
												authorizationActionLabel={
													authorizationAction
														? connectorT(authorizationAction.label_key)
														: null
												}
												credentials={storageCredentials}
												credentialsLoading={storageCredentialsLoading}
												redirectUri={storageAuthorizationRedirectUri}
												validationActionLabel={
													validationAction
														? connectorT(validationAction.label_key)
														: null
												}
												authorizationSubmitting={storageAuthorizationSubmitting}
												validationSubmitting={
													storageCredentialValidationSubmitting
												}
												connectorT={connectorT}
												onAuthorize={onStartStorageAuthorization}
												onValidate={onValidateStorageCredential}
											/>
										</div>
									</section>
								) : null}
								<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
									<SectionTitle
										title={t("policy_editor_rules_title")}
										description={t("policy_editor_rules_desc")}
									/>
									<div className="mt-5">
										<PolicyRules
											form={form}
											forceDefault={forceDefaultPolicy}
											nativeProcessingEnabled={nativeProcessingEnabled}
											onFieldChange={onFieldChange}
										/>
									</div>
								</section>
								{customActions.length > 0 ? (
									<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
										<StorageConnectorActionsPanel
											actions={customActions}
											remoteNodes={remoteNodes}
											remoteStorageTargets={remoteStorageTargets}
											connectorId={storageDriverDescriptor?.connector_id}
											confirmActionId={connectorActionConfirmId}
											submittingActionId={connectorActionSubmittingId}
											t={t}
											values={connectorActionValues}
											onCancel={onCancelConnectorAction}
											onConfirm={onConfirmConnectorAction}
											onRequest={onRequestConnectorAction}
											onValueChange={onConnectorActionValueChange}
										/>
									</section>
								) : null}
							</div>
						)}
					</div>
					{saveAnywayConfirmOpen ? (
						<div className="shrink-0 px-6 pb-3">
							<InlineConfirm>
								<div className="flex items-center justify-between gap-3">
									<p className="text-sm text-muted-foreground">
										{t("connection_test_failed_save_prompt")}
									</p>
									<div className="flex gap-2">
										<Button
											type="button"
											variant="outline"
											onClick={onCancelSaveAnyway}
										>
											{t("core:cancel")}
										</Button>
										<Button type="button" onClick={onConfirmSaveAnyway}>
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
									disabled={submitting}
									onClick={onCreateBack}
								>
									{t("core:back")}
								</Button>
							) : null}
						</div>
						<div className="ml-auto flex shrink-0 flex-nowrap items-center justify-end gap-2">
							{isCreateMode ? (
								createStep === 0 ? null : createStep === createLastStep ? (
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
											disabled={submitting || !storageDriverDescriptor}
											onClick={onSubmit}
										>
											{submitting ? (
												<Icon
													name="Spinner"
													className="mr-1 size-4 animate-spin"
												/>
											) : null}
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
											disabled={submitting || !storageDriverDescriptor}
											onClick={onCreateNext}
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
										disabled={submitting || !storageDriverDescriptor}
										onClick={onSubmit}
									>
										{submitting ? (
											<Icon
												name="Spinner"
												className="mr-1 size-4 animate-spin"
											/>
										) : null}
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

function WizardProgress({
	createStep,
	steps,
	onStepChange,
}: {
	createStep: number;
	steps: Array<{ title: string; description: string }>;
	onStepChange: (step: number) => void;
}) {
	const { t } = useTranslation("admin");
	const currentStep = steps[Math.min(createStep, steps.length - 1)];
	return (
		<div className="space-y-3">
			<div className="rounded-2xl border border-border/70 bg-muted/20 p-3 sm:p-4">
				<div className="flex items-start justify-between gap-3">
					<div className="space-y-1">
						<p className="text-[11px] font-medium uppercase tracking-[0.2em] text-muted-foreground">
							{t("policy_wizard_progress", {
								current: createStep + 1,
								total: steps.length,
							})}
						</p>
						<h3 className="text-sm font-semibold sm:text-base">
							{currentStep.title}
						</h3>
						<p className="hidden text-sm text-muted-foreground sm:block">
							{currentStep.description}
						</p>
					</div>
					<div className="hidden text-3xl leading-none font-semibold text-foreground/15 md:block">
						{String(createStep + 1).padStart(2, "0")}
					</div>
				</div>
				<div className="mt-3 h-1.5 overflow-hidden rounded-full bg-muted">
					<div
						className="h-full rounded-full bg-primary transition-all"
						style={{ width: `${((createStep + 1) / steps.length) * 100}%` }}
					/>
				</div>
			</div>
			<div className="hidden gap-2 md:grid md:grid-cols-3">
				{steps.map((step, index) => (
					<button
						type="button"
						key={step.title}
						disabled={index > createStep}
						onClick={() => onStepChange(index)}
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

function ConnectorSelection({
	descriptors,
	error,
	loading,
	selectedId,
	setup,
	onSelect,
}: {
	descriptors: StorageConnectorDescriptor[];
	error: string | null;
	loading: boolean;
	selectedId: string;
	setup: boolean;
	onSelect: (connectorId: string) => void;
}) {
	const { t } = useTranslation("admin");
	if (loading && descriptors.length === 0) {
		return (
			<div className="flex min-h-32 items-center justify-center gap-2 rounded-lg border border-dashed border-border text-sm text-muted-foreground">
				<Icon name="Spinner" className="size-4 animate-spin" />
				<span>{t("core:loading")}</span>
			</div>
		);
	}
	if (error && descriptors.length === 0) {
		return (
			<div className="rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-sm text-destructive">
				{error}
			</div>
		);
	}
	return (
		<div
			data-testid="storage-driver-options"
			className="grid gap-3 md:grid-cols-2"
		>
			{descriptors.map((descriptor) => {
				const selected = descriptor.connector_id === selectedId;
				const disabled = setup && !descriptor.supports_initial_setup;
				return (
					<button
						type="button"
						key={descriptor.connector_id}
						aria-pressed={selected}
						disabled={disabled}
						onClick={() => onSelect(descriptor.connector_id)}
						className={cn(
							"rounded-2xl border border-border p-4 text-left transition hover:border-primary/40 hover:bg-muted/20 focus-visible:border-ring focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:border-border disabled:hover:bg-background",
							selected ? "bg-muted/15" : "bg-background",
						)}
					>
						<div className="flex items-start gap-4">
							<div className="flex size-14 shrink-0 items-center justify-center rounded-2xl bg-white shadow-sm ring-1 ring-black/5">
								<ConnectorVisual descriptor={descriptor} />
							</div>
							<div className="min-w-0 flex-1">
								<p className="text-base font-semibold">
									{translateStorageConnectorMessage(
										t,
										descriptor.connector_id,
										descriptor.ui.label_key,
									)}
								</p>
								<p className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">
									{translateStorageConnectorMessage(
										t,
										descriptor.connector_id,
										descriptor.ui.description_key,
									)}
								</p>
								{disabled ? (
									<p className="mt-2 text-xs font-medium leading-5 text-amber-700 dark:text-amber-300">
										{t("auth:storage_setup_connector_post_setup_only")}
									</p>
								) : null}
							</div>
						</div>
					</button>
				);
			})}
		</div>
	);
}

function ConnectorVisual({
	descriptor,
	className,
}: {
	descriptor: StorageConnectorDescriptor | null;
	className?: string;
}) {
	if (descriptor?.ui.icon_src) {
		return (
			<img
				src={descriptor.ui.icon_src}
				alt=""
				className={cn("max-h-9 w-auto object-contain", className)}
			/>
		);
	}
	const iconName = descriptor?.ui.icon_name;
	return (
		<Icon
			name={iconName && isIconName(iconName) ? iconName : "Globe"}
			className={cn("size-8 text-amber-600 dark:text-amber-300", className)}
		/>
	);
}

function PolicyNameField({
	form,
	showError,
	onChange,
}: {
	form: PolicyFormData;
	showError: boolean;
	onChange: (value: string) => void;
}) {
	const { t } = useTranslation("admin");
	const invalid = showError && !form.name.trim();
	return (
		<div className="space-y-2">
			<Label htmlFor="name">{t("core:name")}</Label>
			<Input
				id="name"
				value={form.name}
				required
				aria-invalid={invalid || undefined}
				className={ADMIN_CONTROL_HEIGHT_CLASS}
				onChange={(event) => onChange(event.target.value)}
			/>
			{invalid ? (
				<p className="text-xs text-destructive">
					{t("policy_wizard_name_required")}
				</p>
			) : null}
		</div>
	);
}

function ConnectorHelper({
	descriptor,
}: {
	descriptor: StorageConnectorDescriptor | null;
}) {
	const { t } = useTranslation("admin");
	const connectorT = (key: string) =>
		translateStorageConnectorMessage(t, descriptor?.connector_id, key);
	return (
		<div className="rounded-3xl border border-border/70 bg-muted/20 p-5">
			<div className="flex items-center gap-3">
				<div className="flex size-14 items-center justify-center rounded-2xl bg-white shadow-sm ring-1 ring-black/5">
					<ConnectorVisual descriptor={descriptor} />
				</div>
				<div>
					<p className="text-sm font-medium">
						{descriptor
							? connectorT(descriptor.ui.label_key)
							: t("driver_type")}
					</p>
					<p className="text-xs text-muted-foreground">
						{t("policy_wizard_driver_panel_title")}
					</p>
				</div>
			</div>
			<p className="mt-4 text-sm leading-6 text-muted-foreground">
				{descriptor
					? connectorT(descriptor.ui.description_key)
					: t("policy_wizard_step_storage_desc")}
			</p>
			<p className="mt-4 text-xs leading-5 text-muted-foreground">
				{descriptor
					? connectorT(descriptor.ui.helper_key)
					: t("policy_wizard_step_storage_desc")}
			</p>
		</div>
	);
}

function RemoteTargets({
	driverDescriptors,
	driverError,
	driverLoading,
	targets,
	targetsError,
	targetsLoading,
	onCreate,
}: {
	driverDescriptors: RemoteStorageTargetDriverDescriptor[];
	driverError: string | null;
	driverLoading: boolean;
	targets: RemoteStorageTargetInfo[];
	targetsError: string | null;
	targetsLoading: boolean;
	onCreate: (payload: RemoteCreateStorageTargetRequest) => Promise<void>;
}) {
	return (
		<RemoteNodeRemoteStorageTargetSection
			allowCreate
			createLabelKey="policy_remote_storage_targets_quick_create"
			descriptionKey="policy_remote_storage_targets_view_desc"
			driverDescriptors={driverDescriptors}
			errorMessage={targetsError ?? driverError}
			loading={targetsLoading || driverLoading}
			onCreateTarget={onCreate}
			readOnly
			surface="plain"
			targets={targets}
			titleKey="policy_remote_storage_targets_view_title"
		/>
	);
}

function PolicyRules({
	form,
	forceDefault,
	nativeProcessingEnabled,
	onFieldChange,
}: {
	form: PolicyFormData;
	forceDefault: boolean;
	nativeProcessingEnabled: boolean;
	onFieldChange: StoragePolicyDialogProps["onFieldChange"];
}) {
	const { t } = useTranslation("admin");
	return (
		<div className="space-y-4">
			<div className="grid gap-4 md:grid-cols-2">
				<NumberTextField
					id="max-file-size"
					label={t("max_file_size")}
					value={form.max_file_size}
					onChange={(value) => onFieldChange("max_file_size", value)}
				/>
				<NumberTextField
					id="chunk-size"
					label={t("chunk_size")}
					value={form.chunk_size}
					onChange={(value) => onFieldChange("chunk_size", value)}
				/>
			</div>
			{forceDefault ? null : (
				<div className="flex items-center justify-between rounded-lg border border-border/70 px-3 py-2">
					<Label htmlFor="default-policy">{t("set_as_default")}</Label>
					<Switch
						id="default-policy"
						checked={form.is_default}
						onCheckedChange={(checked) => onFieldChange("is_default", checked)}
					/>
				</div>
			)}
			{nativeProcessingEnabled ? (
				<div className="space-y-4 border-t border-border/70 pt-4">
					<SectionTitle
						title={t("policy_storage_native_section_title")}
						description={t("policy_storage_native_section_desc")}
					/>
					<ExtensionField
						id="thumbnail-extensions"
						label={t("storage_native_thumbnail_extensions")}
						values={form.thumbnail_extensions}
						onChange={(values) => onFieldChange("thumbnail_extensions", values)}
					/>
					<ExtensionField
						id="media-extensions"
						label={t("storage_native_media_metadata_extensions")}
						values={form.media_metadata_extensions}
						onChange={(values) =>
							onFieldChange("media_metadata_extensions", values)
						}
					/>
				</div>
			) : null}
		</div>
	);
}

function PolicySummary({
	descriptor,
	name,
	items,
}: {
	descriptor: StorageConnectorDescriptor | null;
	name: string;
	items: Array<{ label: string; value: string }>;
}) {
	const { t } = useTranslation("admin");
	return (
		<div
			data-testid="policy-summary-card"
			className="rounded-3xl border border-border/70 bg-muted/20 p-5 lg:sticky lg:top-0 lg:self-start"
		>
			<div className="flex items-center gap-3">
				<div className="flex size-14 items-center justify-center rounded-2xl bg-white shadow-sm ring-1 ring-black/5">
					<ConnectorVisual descriptor={descriptor} />
				</div>
				<div>
					<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
						{t("policy_wizard_summary_title")}
					</p>
					<h3 className="mt-1 text-base font-semibold">
						{name || t("new_policy")}
					</h3>
				</div>
			</div>
			<p className="mt-4 text-sm leading-6 text-muted-foreground">
				{t("policy_wizard_summary_desc")}
			</p>
			<div className="mt-4 overflow-hidden rounded-2xl border border-border/70 bg-background/85">
				<dl className="divide-y divide-border/70">
					{items.map((item) => (
						<div
							key={item.label}
							className="grid grid-cols-[96px_minmax(0,1fr)] items-start gap-3 px-4 py-3"
						>
							<dt className="pt-0.5 text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
								{item.label}
							</dt>
							<dd className="min-w-0 break-all text-sm font-medium leading-5 text-foreground">
								{item.value}
							</dd>
						</div>
					))}
				</dl>
			</div>
		</div>
	);
}

function PolicyEditContextBar({
	capacity,
	descriptor,
	form,
	loading,
}: {
	capacity: StoragePolicyCapacityInfo | null;
	descriptor: StorageConnectorDescriptor | null;
	form: PolicyFormData;
	loading: boolean;
}) {
	const { t } = useTranslation("admin");
	const connectorT = (key: string) =>
		translateStorageConnectorMessage(t, descriptor?.connector_id, key);
	const basePath = connectorStringValue(form, "base_path");
	const displayBasePath =
		basePath || t(descriptor?.ui.base_path_empty_display ?? "core:root");
	const badgePresentation = getStorageConnectorBadgePresentation(
		descriptor?.ui.badge_rgb,
	);
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
						{form.name.trim() || t("new_policy")}
					</h3>
					<div className="mt-2 flex flex-wrap items-center gap-2">
						<Badge
							variant="outline"
							data-testid="policy-edit-driver-badge"
							className={cn("shadow-sm", badgePresentation.className)}
							style={badgePresentation.style}
						>
							{descriptor
								? connectorT(descriptor.ui.label_key)
								: form.connector_id}
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
						{descriptor ? connectorT("base_path") : t("base_path")}:{" "}
						{displayBasePath}
					</p>
					<p className="mt-1 text-sm leading-6 text-muted-foreground">
						{descriptor
							? connectorT(descriptor.ui.edit_context_key)
							: t("policy_edit_context_local_desc")}
					</p>
				</div>
				<div
					data-testid="policy-edit-capacity-summary"
					className="min-w-0 border-border/70 md:border-l md:pl-4"
				>
					<PolicyCapacitySummary capacity={capacity} loading={loading} />
				</div>
			</div>
		</section>
	);
}

function capacityStatusTone(
	status: StoragePolicyCapacityInfo["capacity"]["status"],
) {
	if (status === "supported") {
		return "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-300";
	}
	if (status === "unsupported") {
		return "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-300";
	}
	return "border-border bg-background/80 text-muted-foreground";
}

function finiteNonNegative(value: number | null | undefined) {
	return typeof value === "number" && Number.isFinite(value)
		? Math.max(value, 0)
		: null;
}

function PolicyCapacitySummary({
	capacity,
	loading,
}: {
	capacity: StoragePolicyCapacityInfo | null;
	loading: boolean;
}) {
	const { t } = useTranslation("admin");
	const info = capacity?.capacity;
	const blobUsage =
		capacity == null
			? null
			: {
					bytes: Math.max(capacity.blob_total_bytes, 0),
					count: Math.max(capacity.blob_count, 0),
				};
	const rawTotal = finiteNonNegative(info?.total_bytes);
	const rawUsed = finiteNonNegative(info?.used_bytes);
	const rawAvailable = finiteNonNegative(info?.available_bytes);
	const hasSupportedTotals =
		info?.status === "supported" &&
		rawTotal != null &&
		rawUsed != null &&
		rawAvailable != null;
	const total = hasSupportedTotals ? rawTotal : null;
	const used =
		total != null && rawUsed != null ? Math.min(rawUsed, total) : null;
	const available =
		total != null && used != null && rawAvailable != null
			? Math.min(rawAvailable, total - used)
			: null;
	const blobInUsed =
		used != null && blobUsage != null ? Math.min(blobUsage.bytes, used) : 0;
	const occupiedPercent =
		total != null && total > 0 && used != null ? (used / total) * 100 : null;
	const blobPercent =
		occupiedPercent != null && total != null ? (blobInUsed / total) * 100 : 0;
	const otherPercent =
		occupiedPercent != null ? Math.max(0, occupiedPercent - blobPercent) : 0;
	const fallbackDescription =
		info?.status === "unsupported"
			? t("policy_capacity_unsupported_desc")
			: t("policy_capacity_unavailable_desc");
	const status = loading ? "unavailable" : (info?.status ?? "unavailable");

	return (
		<div>
			<div className="flex items-start justify-between gap-3">
				<p className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
					{t("policy_capacity_title")}
				</p>
				<span
					className={cn(
						"shrink-0 rounded-full border px-2 py-0.5 text-[11px] font-medium",
						capacityStatusTone(status),
					)}
				>
					{loading
						? t("policy_capacity_checking")
						: t(`policy_capacity_status_${status}`)}
				</span>
			</div>

			{loading ? (
				<p className="mt-3 text-sm leading-6 text-muted-foreground">
					{t("policy_capacity_loading")}
				</p>
			) : (
				<div className="mt-3 space-y-3">
					{blobUsage != null ? (
						<div className="flex items-baseline justify-between gap-3">
							<div>
								<p className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
									{t("policy_capacity_blob_usage")}
								</p>
								<p
									data-testid="policy-capacity-blob-used"
									className="mt-0.5 text-sm font-semibold tabular-nums text-foreground"
								>
									{formatBytes(blobUsage.bytes)}
								</p>
							</div>
							<p className="text-xs text-muted-foreground">
								{t("policy_capacity_blob_count", { count: blobUsage.count })}
							</p>
						</div>
					) : null}

					{total != null && used != null && available != null ? (
						<div className="rounded-xl border border-border/70 bg-background/70 p-3">
							<div className="grid grid-cols-2 gap-3">
								<div>
									<p className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
										{t("policy_capacity_system_used")}
									</p>
									<p
										data-testid="policy-capacity-system-used"
										className="mt-0.5 text-sm font-medium tabular-nums text-foreground"
									>
										{formatBytes(used)}
									</p>
								</div>
								<div>
									<p className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
										{t("policy_capacity_available")}
									</p>
									<p
										data-testid="policy-capacity-available"
										className="mt-0.5 text-sm font-medium tabular-nums text-foreground"
									>
										{formatBytes(available)}
									</p>
								</div>
							</div>
							<p
								data-testid="policy-capacity-total"
								className="mt-2 text-xs text-muted-foreground"
							>
								{t("policy_capacity_total", { total: formatBytes(total) })}
							</p>

							{occupiedPercent != null ? (
								<>
									<div
										role="progressbar"
										aria-label={t("policy_capacity_occupied_progress")}
										aria-valuemin={0}
										aria-valuemax={100}
										aria-valuenow={Number(occupiedPercent.toFixed(2))}
										aria-valuetext={t("policy_capacity_occupied_value", {
											percent: Math.round(occupiedPercent),
											total: formatBytes(total),
											used: formatBytes(used),
										})}
										className="mt-3 flex h-2 w-full overflow-hidden rounded-full bg-muted"
									>
										<span
											aria-hidden="true"
											data-testid="policy-capacity-other-segment"
											className="h-full bg-emerald-500"
											style={{ width: `${otherPercent}%` }}
										/>
										<span
											aria-hidden="true"
											data-testid="policy-capacity-blob-segment"
											className="h-full bg-blue-500"
											style={{ width: `${blobPercent}%` }}
										/>
									</div>
									<div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
										<span className="flex items-center gap-1.5">
											<span className="size-2 rounded-full bg-emerald-500" />
											{t("policy_capacity_other_system_used")}
										</span>
										<span className="flex items-center gap-1.5">
											<span className="size-2 rounded-full bg-blue-500" />
											{t("policy_capacity_blob_usage")}
										</span>
										<span className="flex items-center gap-1.5">
											<span className="size-2 rounded-full bg-muted ring-1 ring-border" />
											{t("policy_capacity_available")}
										</span>
									</div>
								</>
							) : (
								<p className="mt-2 text-xs leading-5 text-muted-foreground">
									{t("policy_capacity_zero_total_desc")}
								</p>
							)}
						</div>
					) : (
						<p className="text-sm leading-6 text-muted-foreground">
							{fallbackDescription}
						</p>
					)}
				</div>
			)}
		</div>
	);
}

function buildPolicySummaryItems(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor | null,
	remoteNodes: RemoteNodeInfo[],
	remoteStorageTargets: RemoteStorageTargetInfo[],
	t: (key: string, values?: Record<string, number | string>) => string,
) {
	const items = [
		{
			label: t("driver_type"),
			value: descriptor
				? translateStorageConnectorMessage(
						t,
						descriptor.connector_id,
						descriptor.ui.label_key,
					)
				: form.connector_id || "—",
		},
		{
			label: t("max_file_size"),
			value:
				!form.max_file_size || Number(form.max_file_size) === 0
					? t("core:unlimited")
					: `${form.max_file_size} bytes`,
		},
		{ label: t("chunk_size"), value: `${form.chunk_size || "0"} MB` },
		{
			label: t("set_as_default"),
			value: form.is_default
				? t("policy_wizard_enabled")
				: t("policy_wizard_disabled"),
		},
	];
	if (!descriptor) {
		return items;
	}
	for (const field of descriptor.fields) {
		if (
			field.scope === "action_input" ||
			field.secret ||
			field.kind === "secret"
		) {
			continue;
		}
		const value =
			field.scope === "connector_config"
				? connectorFormValue(form, field.name)
				: form.credential_values[field.name];
		items.push({
			label: translateStorageConnectorMessage(
				t,
				descriptor.connector_id,
				field.label_key,
			),
			value: connectorFieldDisplayValue(
				field,
				value,
				descriptor,
				remoteNodes,
				remoteStorageTargets,
				t,
			),
		});
	}
	return items;
}

function connectorFieldDisplayValue(
	field: StorageConnectorFieldDescriptor,
	value: StorageConnectorFieldValue | string | null | undefined,
	descriptor: StorageConnectorDescriptor,
	remoteNodes: RemoteNodeInfo[],
	remoteStorageTargets: RemoteStorageTargetInfo[],
	t: (key: string) => string,
) {
	const resolved = value ?? field.default_value;
	if (field.select?.data_source === "remote_nodes") {
		return (
			remoteNodes.find((node) => node.id === Number(resolved))?.name ??
			String(resolved ?? "—")
		);
	}
	if (field.select?.data_source === "remote_storage_targets") {
		const target = remoteStorageTargets.find(
			(candidate) => candidate.target_key === String(resolved),
		);
		return target?.name || target?.target_key || String(resolved ?? "—");
	}
	const option = field.select?.options?.find(
		(candidate) => String(candidate.value) === String(resolved),
	);
	if (option) {
		return translateStorageConnectorMessage(
			t,
			descriptor.connector_id,
			option.label_key,
		);
	}
	if (typeof resolved === "boolean") {
		return resolved ? t("policy_wizard_enabled") : t("policy_wizard_disabled");
	}
	if ((resolved === "" || resolved == null) && field.name === "base_path") {
		return t(descriptor.ui.base_path_empty_display);
	}
	return resolved == null || resolved === "" ? "—" : String(resolved);
}

function SectionTitle({
	title,
	description,
}: {
	title: string;
	description: string;
}) {
	return (
		<div>
			<h3 className="text-sm font-semibold">{title}</h3>
			<p className="mt-1 text-sm leading-6 text-muted-foreground">
				{description}
			</p>
		</div>
	);
}

function NumberTextField({
	id,
	label,
	value,
	onChange,
}: {
	id: string;
	label: string;
	value: string;
	onChange: (value: string) => void;
}) {
	return (
		<div className="space-y-2">
			<Label htmlFor={id}>{label}</Label>
			<Input
				id={id}
				type="number"
				min={0}
				value={value}
				onChange={(event) => onChange(event.target.value)}
			/>
		</div>
	);
}

function ExtensionField({
	id,
	label,
	values,
	onChange,
}: {
	id: string;
	label: string;
	values: string[];
	onChange: (values: string[]) => void;
}) {
	return (
		<div className="space-y-2">
			<Label htmlFor={id}>{label}</Label>
			<Input
				id={id}
				value={values.join(", ")}
				onChange={(event) =>
					onChange(
						event.target.value
							.split(",")
							.map((value) => value.trim())
							.filter(Boolean),
					)
				}
			/>
		</div>
	);
}

function ConnectorManagement({
	management,
	authorizationActionLabel,
	credentials,
	credentialsLoading,
	redirectUri,
	validationActionLabel,
	authorizationSubmitting,
	validationSubmitting,
	connectorT,
	onAuthorize,
	onValidate,
}: {
	management: StorageConnectorCredentialManagementDescriptor | null;
	authorizationActionLabel: string | null;
	credentials: StorageConnectorCredentialInfo[];
	credentialsLoading: boolean;
	redirectUri: string;
	validationActionLabel: string | null;
	authorizationSubmitting: boolean;
	validationSubmitting: boolean;
	connectorT: (key: string) => string;
	onAuthorize: () => void;
	onValidate: () => void;
}) {
	if (
		!management ||
		(!authorizationActionLabel &&
			!validationActionLabel &&
			credentials.length === 0)
	) {
		return null;
	}
	return (
		<section className="space-y-3 border-t pt-5">
			<div className="flex flex-wrap items-center justify-between gap-3">
				<div>
					<h3 className="text-sm font-semibold">
						{connectorT(management.title_key)}
					</h3>
					<p className="mt-1 text-xs text-muted-foreground">
						{credentialsLoading
							? connectorT(management.loading_key)
							: credentials[0]
								? `${
										management.status_keys[credentials[0].status]
											? connectorT(
													management.status_keys[credentials[0].status],
												)
											: credentials[0].status
									}${
										credentials[0].last_validated_at
											? ` · ${formatDateTime(credentials[0].last_validated_at)}`
											: ""
									}`
								: connectorT(management.status_keys.missing)}
					</p>
				</div>
				<div className="flex gap-2">
					{authorizationActionLabel ? (
						<Button
							type="button"
							variant="outline"
							disabled={authorizationSubmitting}
							onClick={onAuthorize}
						>
							{authorizationSubmitting ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : null}
							{authorizationActionLabel}
						</Button>
					) : null}
					{validationActionLabel ? (
						<Button
							type="button"
							variant="outline"
							disabled={validationSubmitting || credentials.length === 0}
							onClick={onValidate}
						>
							{validationSubmitting ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : null}
							{validationActionLabel}
						</Button>
					) : null}
				</div>
			</div>
			{authorizationActionLabel && management.redirect_uri_key ? (
				<div className="space-y-2">
					<Label htmlFor="storage-authorization-redirect-uri">
						{connectorT(management.redirect_uri_key)}
					</Label>
					<Input
						id="storage-authorization-redirect-uri"
						readOnly
						value={redirectUri}
						className="font-mono text-xs"
					/>
				</div>
			) : null}
		</section>
	);
}
