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
	type ExternalAuthProviderFieldChange,
	type ExternalAuthProviderFormData,
	kindDisplayName,
	requiredFieldsMissing,
	shouldShowIssuerUrl,
	shouldShowManualEndpoints,
} from "./shared";

interface ExternalAuthProviderPageProps {
	formTouched: boolean;
	form: ExternalAuthProviderFormData;
	mode: "create" | "edit";
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
	formTouched,
	form,
	mode,
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
		submitting ||
		(isCreate && providerKinds.length === 0) ||
		requiredFieldsMissing(form, selectedKind);

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
			formTouched={formTouched}
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
	const pageActions = (
		<Button
			type="submit"
			form="external-auth-provider-form"
			disabled={submitDisabled}
		>
			<Icon
				name={submitting ? "Spinner" : isCreate ? "Plus" : "FloppyDisk"}
				className={cn("mr-1 size-4", submitting && "animate-spin")}
			/>
			{isCreate ? t("external_auth_provider_create") : t("save_changes")}
		</Button>
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
				<div className="grid gap-8 lg:grid-cols-[300px_minmax(0,1fr)]">
					<aside className="min-w-0 space-y-5 rounded-xl bg-muted/30 p-4 lg:sticky lg:top-6 lg:self-start">
						{summaryPanel}
						{isCreate ? null : (
							<div className="border-t border-foreground/10 pt-4">
								{accessPolicyPanel}
							</div>
						)}
					</aside>
					<div className="min-w-0 space-y-8 animate-in fade-in slide-in-from-top-1 duration-200 fill-mode-backwards delay-75 motion-reduce:animate-none">
						{isCreate ? (
							<section className="rounded-xl bg-muted/30 p-5">
								<div className="space-y-1">
									<h3 className="text-sm font-semibold">
										{t("external_auth_provider_type")}
									</h3>
									<p className="text-sm text-muted-foreground">
										{t("external_auth_provider_type_desc")}
									</p>
								</div>
								<div className="mt-4">
									<ExternalAuthProviderKindPanel
										form={form}
										onProviderKindChange={onProviderKindChange}
										providerKinds={providerKinds}
									/>
								</div>
							</section>
						) : null}
						{identityPanel}
						{rulesPanel}
						{isCreate ? accessPolicyPanel : null}
					</div>
				</div>
			</div>
		</form>
	);

	return (
		<AdminDetailPageShell
			actions={pageActions}
			backLabel={pageBackLabel ?? t("external_auth")}
			description={t("external_auth_provider_page_desc")}
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
