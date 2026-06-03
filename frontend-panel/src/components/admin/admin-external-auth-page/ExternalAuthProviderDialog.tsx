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
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
	ExternalAuthProviderKind,
} from "@/types/api";
import { ExternalAuthCreateProgress } from "./ExternalAuthCreateProgress";
import {
	ExternalAuthAccessPolicyPanel,
	ExternalAuthProviderIdentityPanel,
	ExternalAuthProviderKindPanel,
	ExternalAuthProviderRulesPanel,
	ExternalAuthSummaryPanel,
} from "./ExternalAuthProviderPanels";
import {
	callbackUrl,
	connectionRequirementsMissing,
	type ExternalAuthCreateStep,
	type ExternalAuthProviderFieldChange,
	type ExternalAuthProviderFormData,
	kindDisplayName,
	requiredFieldsMissing,
	shouldShowIssuerUrl,
	shouldShowManualEndpoints,
} from "./shared";

interface ExternalAuthProviderDialogProps {
	createStep: number;
	createStepDirection: "idle" | "forward" | "backward";
	createStepTouched: boolean;
	createSteps: ExternalAuthCreateStep[];
	form: ExternalAuthProviderFormData;
	mode: "create" | "edit";
	onCreateBack: () => void;
	onCreateNext: () => void;
	onCreateStepChange: (step: number) => void;
	onFieldChange: ExternalAuthProviderFieldChange;
	onProviderKindChange: (kind: ExternalAuthProviderKind) => void;
	onCopyCallbackUrl: (value: string) => void;
	onOpenChange: (open: boolean) => void;
	onSubmit: () => void;
	onTestConnection: () => Promise<boolean>;
	open: boolean;
	provider: AdminExternalAuthProviderInfo | null;
	providerKinds: AdminExternalAuthProviderKindInfo[];
	submitting: boolean;
	testResult: string | null;
}

export function ExternalAuthProviderDialog({
	createStep,
	createStepDirection,
	createStepTouched,
	createSteps,
	form,
	mode,
	onCreateBack,
	onCreateNext,
	onCreateStepChange,
	onCopyCallbackUrl,
	onFieldChange,
	onProviderKindChange,
	onOpenChange,
	onSubmit,
	onTestConnection,
	open,
	provider,
	providerKinds,
	submitting,
	testResult,
}: ExternalAuthProviderDialogProps) {
	const { t } = useTranslation("admin");
	const isCreate = mode === "create";
	const createLastStep = createSteps.length - 1;
	const providerKind = provider?.provider_kind ?? form.providerKind;
	const selectedKind =
		providerKinds.find((item) => item.kind === providerKind) ??
		providerKinds[0] ??
		null;
	const providerKindLabel = kindDisplayName(t, providerKind, providerKinds);
	const showOptionalGenericIssuer =
		selectedKind?.kind === "generic_oauth2" && Boolean(form.issuerUrl.trim());
	const showIssuerUrl = Boolean(
		shouldShowIssuerUrl(selectedKind) || showOptionalGenericIssuer,
	);
	const showManualEndpoints = Boolean(
		shouldShowManualEndpoints(selectedKind) ||
			form.authorizationUrl.trim() ||
			form.tokenUrl.trim() ||
			form.userinfoUrl.trim(),
	);
	const currentCallbackUrl = callbackUrl(providerKind, form.key);
	const identityMissing = !form.displayName.trim();
	const connectionMissing = connectionRequirementsMissing(form, selectedKind);
	const testDisabled = submitting || connectionMissing;
	const submitDisabled =
		submitting || requiredFieldsMissing(form, selectedKind);
	const stepPanelClass = cn(
		createStepDirection === "idle"
			? undefined
			: "animate-in fade-in duration-[360ms] motion-reduce:animate-none",
		createStepDirection === "forward"
			? "slide-in-from-right-6"
			: createStepDirection === "backward"
				? "slide-in-from-left-6"
				: undefined,
	);

	const summaryPanel = (
		<ExternalAuthSummaryPanel
			currentCallbackUrl={currentCallbackUrl}
			form={form}
			isCreate={isCreate}
			providerKind={providerKind}
			providerKinds={providerKinds}
			selectedKind={selectedKind}
		/>
	);
	const identityPanel = (
		<ExternalAuthProviderIdentityPanel
			connectionMissing={connectionMissing}
			createStepTouched={createStepTouched}
			currentCallbackUrl={currentCallbackUrl}
			form={form}
			identityMissing={identityMissing}
			isCreate={isCreate}
			onCopyCallbackUrl={onCopyCallbackUrl}
			onFieldChange={onFieldChange}
			onTestConnection={onTestConnection}
			provider={provider}
			providerKindLabel={providerKindLabel}
			selectedKind={selectedKind}
			showIssuerUrl={showIssuerUrl}
			showManualEndpoints={showManualEndpoints}
			testDisabled={testDisabled}
			testResult={testResult}
		/>
	);
	const rulesPanel = (
		<ExternalAuthProviderRulesPanel
			form={form}
			onFieldChange={onFieldChange}
			selectedKind={selectedKind}
		/>
	);
	const accessPolicyPanel = (
		<ExternalAuthAccessPolicyPanel form={form} onFieldChange={onFieldChange} />
	);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="flex max-h-[min(90vh,calc(100vh-2rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-[calc(100%-2rem)] lg:max-w-4xl">
				<DialogHeader className="shrink-0 px-6 pt-5 pb-0 pr-14">
					<DialogTitle>
						{isCreate
							? t("external_auth_provider_create")
							: t("external_auth_provider_edit")}
					</DialogTitle>
					<DialogDescription>
						{t("external_auth_provider_dialog_desc")}
					</DialogDescription>
				</DialogHeader>
				<form
					onSubmit={(event) => {
						event.preventDefault();
						onSubmit();
					}}
					autoComplete="off"
					className="flex min-h-0 flex-1 flex-col overflow-hidden"
				>
					<div className="min-h-0 flex-1 overflow-y-auto px-6 pt-6 pb-5">
						{isCreate ? (
							<div className="space-y-6">
								<ExternalAuthCreateProgress
									createStep={createStep}
									createSteps={createSteps}
									onCreateStepChange={onCreateStepChange}
								/>
								<div className="rounded-2xl border border-border/70 bg-background/70 p-5">
									<div className="relative overflow-hidden">
										<div
											key={`${createStep}-${createStepDirection}`}
											data-testid="external-auth-provider-step-panel"
											className={stepPanelClass}
										>
											{createStep === 0 ? (
												<ExternalAuthProviderKindPanel
													form={form}
													onProviderKindChange={onProviderKindChange}
													providerKinds={providerKinds}
												/>
											) : createStep === 1 ? (
												<div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_18rem]">
													<div className="min-w-0 space-y-4">
														{identityPanel}
													</div>
													<aside className="min-w-0 space-y-4 lg:sticky lg:top-0 lg:self-start">
														{summaryPanel}
													</aside>
												</div>
											) : (
												<div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_18rem]">
													<div className="min-w-0 space-y-4">
														{rulesPanel}
														{accessPolicyPanel}
													</div>
													<aside className="min-w-0 space-y-4 lg:sticky lg:top-0 lg:self-start">
														{summaryPanel}
													</aside>
												</div>
											)}
										</div>
									</div>
								</div>
							</div>
						) : (
							<div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_18rem]">
								<div className="min-w-0 space-y-4">
									{identityPanel}
									{rulesPanel}
								</div>
								<aside className="min-w-0 space-y-4 lg:sticky lg:top-0 lg:self-start">
									{accessPolicyPanel}
									{summaryPanel}
								</aside>
							</div>
						)}
					</div>
					<DialogFooter className="mx-0 mb-0 w-full shrink-0 flex-row items-center gap-2 rounded-b-xl px-6 py-3">
						<div className="mr-auto flex shrink-0 gap-2">
							{isCreate && createStep > 0 ? (
								<Button
									type="button"
									variant="outline"
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									disabled={submitting}
									onClick={onCreateBack}
								>
									{t("core:back")}
								</Button>
							) : (
								<Button
									type="button"
									variant="outline"
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									disabled={submitting}
									onClick={() => onOpenChange(false)}
								>
									{t("core:cancel")}
								</Button>
							)}
						</div>
						<div className="ml-auto flex shrink-0 flex-nowrap items-center justify-end gap-2">
							{isCreate && createStep < createLastStep ? (
								<Button
									type="button"
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									disabled={submitting}
									onClick={onCreateNext}
								>
									{createStep === createLastStep - 1
										? t("policy_wizard_review")
										: t("policy_wizard_next")}
								</Button>
							) : (
								<Button
									type="submit"
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									disabled={submitDisabled}
								>
									{submitting ? (
										<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
									) : (
										<Icon name="FloppyDisk" className="mr-2 size-4" />
									)}
									{isCreate
										? t("external_auth_provider_create")
										: t("save_changes")}
								</Button>
							)}
						</div>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
