import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
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
	RemoteCreateStorageTargetRequest,
	RemoteNodeInfo,
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
} from "@/types/api";
import {
	getRemoteNodeBaseUrlValidationMessage,
	type RemoteNodeFormData,
	type RemoteNodeTransportMode,
} from "../remoteNodeDialogShared";
import { RemoteNodeCreateWizard } from "./RemoteNodeCreateWizard";
import type { RemoteNodeDialogStep } from "./RemoteNodeDialogTypes";
import { RemoteNodeEditForm } from "./RemoteNodeEditForm";
import {
	getRemoteNodeEnrollmentStatusLabel,
	getRemoteNodeTransportLabel,
	getRemoteNodeTransportTone,
	hasCompletedRemoteNodeEnrollment,
	TestConnectionButton,
} from "./shared";

const EMPTY_REMOTE_STORAGE_TARGETS: RemoteStorageTargetInfo[] = [];
const EMPTY_REMOTE_STORAGE_TARGET_DRIVER_DESCRIPTORS: RemoteStorageTargetDriverDescriptor[] =
	[];

interface RemoteNodeDialogProps {
	createStep: number;
	createStepTouched: boolean;
	editingNode: RemoteNodeInfo | null;
	form: RemoteNodeFormData;
	remoteStorageTargetDriverDescriptors?: RemoteStorageTargetDriverDescriptor[];
	remoteStorageTargetDriverDescriptorsError?: string | null;
	remoteStorageTargetDriverDescriptorsLoading?: boolean;
	remoteStorageTargets?: RemoteStorageTargetInfo[];
	remoteStorageTargetsEnabled?: boolean;
	remoteStorageTargetsError?: string | null;
	remoteStorageTargetsLoading?: boolean;
	mode: "create" | "edit";
	onCreateRemoteStorageTarget?: (
		payload: RemoteCreateStorageTargetRequest,
	) => Promise<void>;
	onDeleteRemoteStorageTarget?: (
		profile: RemoteStorageTargetInfo,
	) => Promise<void>;
	onCreateBack: () => void;
	onCreateNext: () => void;
	onCreateStepChange: (step: number) => void;
	onFieldChange: <K extends keyof RemoteNodeFormData>(
		key: K,
		value: RemoteNodeFormData[K],
	) => void;
	onOpenChange: (open: boolean) => void;
	onRunConnectionTest: () => Promise<boolean>;
	onSubmit: () => void;
	onUpdateRemoteStorageTarget?: (
		target_key: string,
		payload: RemoteUpdateStorageTargetRequest,
	) => Promise<void>;
	open: boolean;
	submitting: boolean;
}

export function RemoteNodeDialog({
	createStep,
	createStepTouched,
	editingNode,
	form,
	remoteStorageTargetDriverDescriptors = EMPTY_REMOTE_STORAGE_TARGET_DRIVER_DESCRIPTORS,
	remoteStorageTargetDriverDescriptorsError = null,
	remoteStorageTargetDriverDescriptorsLoading = false,
	remoteStorageTargets = EMPTY_REMOTE_STORAGE_TARGETS,
	remoteStorageTargetsEnabled = false,
	remoteStorageTargetsError = null,
	remoteStorageTargetsLoading = false,
	mode,
	onCreateRemoteStorageTarget,
	onDeleteRemoteStorageTarget,
	onCreateBack,
	onCreateNext,
	onCreateStepChange,
	onFieldChange,
	onOpenChange,
	onRunConnectionTest,
	onSubmit,
	onUpdateRemoteStorageTarget,
	open,
	submitting,
}: RemoteNodeDialogProps) {
	const { t } = useTranslation("admin");
	const isCreateMode = mode === "create";
	const createSteps: RemoteNodeDialogStep[] = [
		{
			title: t("remote_node_wizard_step_identity_title"),
			description: t("remote_node_wizard_step_identity_desc"),
		},
		{
			title: t("remote_node_wizard_step_connection_title"),
			description: t("remote_node_wizard_step_connection_desc"),
		},
		{
			title: t("remote_node_wizard_step_review_title"),
			description: t("remote_node_wizard_step_review_desc"),
		},
	];
	const createLastStep = createSteps.length - 1;
	const baseUrlValidationMessage = getRemoteNodeBaseUrlValidationMessage(
		form.base_url,
		t,
	);
	const normalizedTransportMode =
		form.transport_mode === "direct" ||
		form.transport_mode === "reverse_tunnel" ||
		form.transport_mode === "auto"
			? form.transport_mode
			: "direct";
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
	const modeToneClass = getRemoteNodeTransportTone(normalizedTransportMode);
	const enabledToneClass = form.is_enabled
		? "border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300"
		: "border-slate-500/40 bg-slate-500/10 text-slate-600 dark:text-slate-300";
	const hasConnectionFieldChanges =
		editingNode == null
			? true
			: form.base_url !== editingNode.base_url ||
				normalizedTransportMode !== (editingNode.transport_mode ?? "direct");
	const canRunConnectionTest =
		editingNode !== null &&
		hasCompletedRemoteNodeEnrollment(editingNode) &&
		!hasConnectionFieldChanges &&
		(normalizedTransportMode !== "direct" || Boolean(form.base_url.trim())) &&
		!baseUrlValidationMessage;
	const isSubmitDisabled =
		submitting || !form.name.trim() || Boolean(baseUrlValidationMessage);
	const createNameError =
		isCreateMode && createStep === 0 && createStepTouched && !form.name.trim()
			? t("remote_node_wizard_name_required")
			: null;
	const createSummaryItems = [
		{
			label: t("remote_node_transport_mode"),
			value: getRemoteNodeTransportLabel(t, normalizedTransportMode),
		},
		{
			label: t("base_url"),
			value: form.base_url || t("remote_node_base_url_empty"),
		},
		...(editingNode
			? [
					{
						label: t("remote_node_enrollment_status"),
						value: getRemoteNodeEnrollmentStatusLabel(
							t,
							editingNode.enrollment_status,
						),
					},
				]
			: [
					{
						label: t("remote_node_wizard_followup_label"),
						value: t("remote_node_wizard_followup_value"),
					},
				]),
		{
			label: t("remote_node_status"),
			value: form.is_enabled
				? t("remote_node_status_enabled")
				: t("remote_node_status_disabled"),
		},
	];
	const transportOptions: {
		badge?: string;
		description: string;
		label: string;
		value: RemoteNodeTransportMode;
	}[] = [
		{
			value: "direct",
			label: t("remote_node_transport_direct"),
			description: t("remote_node_transport_direct_desc"),
		},
		{
			value: "reverse_tunnel",
			label: t("remote_node_transport_reverse_tunnel"),
			description: t("remote_node_transport_reverse_tunnel_desc"),
			badge: t("remote_node_transport_test_badge"),
		},
		{
			value: "auto",
			label: t("remote_node_transport_auto"),
			description: t("remote_node_transport_auto_desc"),
		},
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
						{isCreateMode ? t("create_remote_node") : t("edit_remote_node")}
					</DialogTitle>
					<DialogDescription>{t("remote_nodes_intro")}</DialogDescription>
				</DialogHeader>
				<form
					onSubmit={(event) => event.preventDefault()}
					autoComplete="off"
					className="flex min-h-0 flex-1 flex-col overflow-hidden"
				>
					<div className="min-h-0 flex-1 overflow-y-auto px-6 pt-6 pb-5">
						{isCreateMode ? (
							<RemoteNodeCreateWizard
								baseUrlValidationMessage={baseUrlValidationMessage}
								createNameError={createNameError}
								createStep={createStep}
								createStepDirection={createStepDirection}
								createSteps={createSteps}
								editingNode={editingNode}
								enabledToneClass={enabledToneClass}
								form={form}
								modeToneClass={modeToneClass}
								onCreateStepChange={onCreateStepChange}
								onFieldChange={onFieldChange}
								stepAnimationKey={stepAnimationKey}
								summaryItems={createSummaryItems}
								transportOptions={transportOptions}
							/>
						) : (
							<RemoteNodeEditForm
								baseUrlValidationMessage={baseUrlValidationMessage}
								editingNode={editingNode}
								enabledToneClass={enabledToneClass}
								form={form}
								remoteStorageTargets={remoteStorageTargets}
								remoteStorageTargetDriverDescriptors={
									remoteStorageTargetDriverDescriptors
								}
								remoteStorageTargetDriverDescriptorsError={
									remoteStorageTargetDriverDescriptorsError
								}
								remoteStorageTargetDriverDescriptorsLoading={
									remoteStorageTargetDriverDescriptorsLoading
								}
								remoteStorageTargetsEnabled={remoteStorageTargetsEnabled}
								remoteStorageTargetsError={remoteStorageTargetsError}
								remoteStorageTargetsLoading={remoteStorageTargetsLoading}
								modeToneClass={modeToneClass}
								onCreateRemoteStorageTarget={onCreateRemoteStorageTarget}
								onDeleteRemoteStorageTarget={onDeleteRemoteStorageTarget}
								onFieldChange={onFieldChange}
								onUpdateRemoteStorageTarget={onUpdateRemoteStorageTarget}
								summaryItems={createSummaryItems}
								transportOptions={transportOptions}
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
									<Button
										type="button"
										className={ADMIN_CONTROL_HEIGHT_CLASS}
										disabled={isSubmitDisabled}
										onClick={onSubmit}
									>
										{t("remote_node_save_and_generate_enrollment_command")}
									</Button>
								) : (
									<Button
										type="button"
										className={ADMIN_CONTROL_HEIGHT_CLASS}
										onClick={onCreateNext}
										disabled={
											submitting ||
											(createStep === 1 && Boolean(baseUrlValidationMessage))
										}
									>
										{createStep === createLastStep - 1
											? t("policy_wizard_review")
											: t("policy_wizard_next")}
									</Button>
								)
							) : (
								<>
									<TestConnectionButton
										onTest={onRunConnectionTest}
										disabled={!canRunConnectionTest || submitting}
									/>
									<Button
										type="button"
										className={ADMIN_CONTROL_HEIGHT_CLASS}
										disabled={isSubmitDisabled}
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
