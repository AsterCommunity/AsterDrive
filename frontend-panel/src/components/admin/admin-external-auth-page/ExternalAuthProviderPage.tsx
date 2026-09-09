import { useTranslation } from "react-i18next";
import { AdminDetailPageShell } from "@/components/layout/AdminDetailPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
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

interface ExternalAuthProviderPageProps {
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
	provider: AdminExternalAuthProviderInfo | null;
	providerKinds: AdminExternalAuthProviderKindInfo[];
	submitting: boolean;
	testResult: string | null;
	pageBackLabel?: string;
}

export function ExternalAuthProviderPage({
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
	provider,
	providerKinds,
	submitting,
	testResult,
	pageBackLabel,
}: ExternalAuthProviderPageProps) {
	const { t } = useTranslation("admin");
	const isCreate = mode === "create";
	const createLastStep = createSteps.length - 1;
	const providerKind = provider?.provider_kind ?? form.providerKind;
	const selectedKind =
		providerKinds.find((item) => item.kind === providerKind) ??
		providerKinds[0] ??
		null;
	const providerKindLabel = kindDisplayName(t, providerKind, providerKinds);
	const showIssuerUrl = shouldShowIssuerUrl(selectedKind);
	const showManualEndpoints = shouldShowManualEndpoints(selectedKind);
	const currentCallbackUrl = callbackUrl(providerKind, form.key);
	const identityMissing = !form.displayName.trim();
	const connectionMissing = connectionRequirementsMissing(form, selectedKind);
	const testDisabled = submitting || connectionMissing;
	const submitDisabled =
		submitting || requiredFieldsMissing(form, selectedKind);
	const showCreateNextButton = isCreate && createStep < createLastStep;
	const stepPanelClass = cn(
		createStep === 0 && "contents",
		createStep === 0
			? undefined
			: createStepDirection === "idle"
				? undefined
				: "animate-in fade-in duration-[360ms] motion-reduce:animate-none",
		createStep > 0 && createStepDirection === "forward"
			? "slide-in-from-right-6"
			: createStep > 0 && createStepDirection === "backward"
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
	const pageActions = isCreate ? (
		<>
			{createStep > 0 ? (
				<Button
					type="button"
					variant="outline"
					onClick={onCreateBack}
					disabled={submitting}
				>
					{t("core:back")}
				</Button>
			) : null}
			{createStep === 1 ? (
				<Button
					type="button"
					variant="outline"
					onClick={() => void onTestConnection()}
					disabled={testDisabled}
				>
					{t("external_auth_provider_test")}
				</Button>
			) : null}
			{showCreateNextButton ? (
				<Button
					type="button"
					onClick={onCreateNext}
					disabled={
						submitting || (createStep === 0 && providerKinds.length === 0)
					}
				>
					{createStep === createLastStep - 1
						? t("policy_wizard_review")
						: t("policy_wizard_next")}
				</Button>
			) : createStep === createLastStep ? (
				<Button
					type="submit"
					form="external-auth-provider-form"
					disabled={submitDisabled}
				>
					<Icon
						name={submitting ? "Spinner" : "Plus"}
						className={cn("mr-1 size-4", submitting && "animate-spin")}
					/>
					{t("external_auth_provider_create")}
				</Button>
			) : null}
		</>
	) : (
		<>
			<Button
				type="button"
				variant="outline"
				onClick={() => void onTestConnection()}
				disabled={testDisabled}
			>
				{t("external_auth_provider_test")}
			</Button>
			<Button
				type="submit"
				form="external-auth-provider-form"
				disabled={submitDisabled}
			>
				<Icon
					name={submitting ? "Spinner" : "FloppyDisk"}
					className={cn("mr-1 size-4", submitting && "animate-spin")}
				/>
				{t("save_changes")}
			</Button>
		</>
	);
	const content = (
		<form
			id="external-auth-provider-form"
			onSubmit={(event) => {
				event.preventDefault();
				onSubmit();
			}}
			autoComplete="off"
			className="flex min-h-0 flex-1 flex-col overflow-visible"
		>
			<div className="min-h-0 flex-1 overflow-visible px-0 pt-0 pb-6">
				{isCreate ? (
					<div className="space-y-6">
						<ExternalAuthCreateProgress
							createStep={createStep}
							createSteps={createSteps}
							onCreateStepChange={onCreateStepChange}
						/>
						<div className="py-1">
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
											<div className="min-w-0 space-y-4">{identityPanel}</div>
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
					<div className="grid gap-8 lg:grid-cols-[300px_minmax(0,1fr)]">
						<aside className="animate-in fade-in slide-in-from-top-1 duration-200 fill-mode-backwards motion-reduce:animate-none space-y-5 rounded-xl bg-muted/30 p-4 lg:sticky lg:top-6 lg:self-start">
							{summaryPanel}
							<div className="border-t border-foreground/10 pt-4">
								{accessPolicyPanel}
							</div>
						</aside>
						<div className="min-w-0 space-y-8 animate-in fade-in slide-in-from-top-1 duration-200 fill-mode-backwards delay-75 motion-reduce:animate-none">
							{identityPanel}
							{rulesPanel}
						</div>
					</div>
				)}
			</div>
		</form>
	);

	return (
		<AdminDetailPageShell
			actions={pageActions}
			backLabel={pageBackLabel ?? t("external_auth")}
			description={t("external_auth_provider_dialog_desc")}
			onBack={() => onOpenChange(false)}
			title={
				isCreate
					? t("external_auth_provider_create")
					: (provider?.display_name ?? t("external_auth_provider_edit"))
			}
		>
			{content}
		</AdminDetailPageShell>
	);
}
