import { useTranslation } from "react-i18next";
import { TestConnectionButton } from "@/components/admin/TestConnectionButton";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
	ExternalAuthProviderKind,
} from "@/types/api";
import { ExternalAuthClaimFields } from "./ExternalAuthClaimFields";
import { ExternalAuthConnectionFields } from "./ExternalAuthConnectionFields";
import {
	CallbackUrlField,
	defaultScopesForKind,
	type ExternalAuthProviderFieldChange,
	type ExternalAuthProviderFormData,
	ExternalAuthProviderIcon,
	formClaimSummary,
	formConnectionSummary,
	GITHUB_CLAIMS,
	GITHUB_FIXED_ENDPOINTS,
	isGitHubProviderKind,
	kindDescription,
	kindDisplayName,
	parseAllowedDomains,
	providerIconSummary,
} from "./shared";

interface ExternalAuthAccessPolicyPanelProps {
	form: ExternalAuthProviderFormData;
	onFieldChange: ExternalAuthProviderFieldChange;
}

export function ExternalAuthAccessPolicyPanel({
	form,
	onFieldChange,
}: ExternalAuthAccessPolicyPanelProps) {
	const { t } = useTranslation("admin");

	return (
		<section className="rounded-2xl border border-border/70 bg-muted/20 p-5">
			<h3 className="text-sm font-semibold">
				{t("external_auth_provider_access_title")}
			</h3>
			<div className="mt-4 space-y-4">
				<div className="space-y-2">
					<div className="flex items-center gap-2">
						<Switch
							id="external-auth-provider-enabled"
							checked={form.enabled}
							onCheckedChange={(value) => onFieldChange("enabled", value)}
						/>
						<Label htmlFor="external-auth-provider-enabled">
							{t("external_auth_provider_enabled")}
						</Label>
					</div>
					<p className="text-xs text-muted-foreground">
						{t("external_auth_provider_enabled_desc")}
					</p>
				</div>
				<div className="space-y-2">
					<div className="flex items-center gap-2">
						<Switch
							id="external-auth-provider-require-email-verified"
							checked={form.requireEmailVerified}
							onCheckedChange={(value) =>
								onFieldChange("requireEmailVerified", value)
							}
						/>
						<Label htmlFor="external-auth-provider-require-email-verified">
							{t("external_auth_provider_require_email_verified")}
						</Label>
					</div>
					<p className="text-xs text-muted-foreground">
						{t("external_auth_provider_require_email_verified_desc")}
					</p>
				</div>
				<div className="space-y-2">
					<div className="flex items-center gap-2">
						<Switch
							id="external-auth-provider-auto-link"
							checked={form.autoLinkVerifiedEmailEnabled}
							onCheckedChange={(value) =>
								onFieldChange("autoLinkVerifiedEmailEnabled", value)
							}
						/>
						<Label htmlFor="external-auth-provider-auto-link">
							{t("external_auth_provider_auto_link")}
						</Label>
					</div>
					<p className="text-xs text-muted-foreground">
						{t("external_auth_provider_auto_link_desc")}
					</p>
				</div>
				<div className="space-y-2">
					<div className="flex items-center gap-2">
						<Switch
							id="external-auth-provider-auto-provision"
							checked={form.autoProvisionEnabled}
							onCheckedChange={(value) =>
								onFieldChange("autoProvisionEnabled", value)
							}
						/>
						<Label htmlFor="external-auth-provider-auto-provision">
							{t("external_auth_provider_auto_provision")}
						</Label>
					</div>
					<p className="text-xs text-muted-foreground">
						{t("external_auth_provider_auto_provision_desc")}
					</p>
				</div>
			</div>
		</section>
	);
}

interface ExternalAuthConnectionTestPanelProps {
	disabled: boolean;
	onTestConnection: () => Promise<boolean>;
	testResult: string | null;
}

export function ExternalAuthConnectionTestPanel({
	disabled,
	onTestConnection,
	testResult,
}: ExternalAuthConnectionTestPanelProps) {
	const { t } = useTranslation("admin");

	return (
		<div className="min-w-0 space-y-2 md:col-span-2">
			<div className="flex min-w-0 flex-wrap items-center gap-3">
				<TestConnectionButton disabled={disabled} onTest={onTestConnection} />
				{testResult ? (
					<p className="min-w-0 flex-1 text-sm text-emerald-700 dark:text-emerald-300">
						{testResult}
					</p>
				) : null}
			</div>
			<p className="text-xs text-muted-foreground">
				{t("external_auth_provider_test_scope_hint")}
			</p>
		</div>
	);
}

interface ExternalAuthSummaryPanelProps {
	currentCallbackUrl: string;
	form: ExternalAuthProviderFormData;
	isCreate: boolean;
	providerKind: ExternalAuthProviderKind;
	providerKinds: AdminExternalAuthProviderKindInfo[];
	selectedKind: AdminExternalAuthProviderKindInfo | null;
}

export function ExternalAuthSummaryPanel({
	currentCallbackUrl,
	form,
	isCreate,
	providerKind,
	providerKinds,
	selectedKind,
}: ExternalAuthSummaryPanelProps) {
	const { t } = useTranslation("admin");
	const providerKindLabel = kindDisplayName(t, providerKind, providerKinds);
	const summaryConnection = formConnectionSummary(form, selectedKind);
	const summaryClaims = formClaimSummary(form, selectedKind);
	const isGitHub = isGitHubProviderKind(selectedKind ?? providerKind);

	return (
		<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
			<h3 className="text-sm font-semibold">
				{t("external_auth_provider_summary_title")}
			</h3>
			<dl className="mt-4 space-y-3 text-sm">
				<div>
					<dt className="text-xs text-muted-foreground">
						{t("external_auth_provider_type")}
					</dt>
					<dd className="mt-1 text-xs font-medium">{providerKindLabel}</dd>
				</div>
				<div>
					<dt className="text-xs text-muted-foreground">
						{t("external_auth_provider_icon_url")}
					</dt>
					<dd className="mt-1 break-words text-xs">
						{providerIconSummary(form)}
					</dd>
				</div>
				<div>
					<dt className="text-xs text-muted-foreground">
						{t("external_auth_provider_primary_endpoint")}
					</dt>
					<dd className="mt-1 break-words text-xs">{summaryConnection}</dd>
				</div>
				{isGitHub ? (
					<div className="rounded-lg border border-border/70 bg-muted/30 p-3">
						<dt className="text-xs font-medium">
							{t("external_auth_provider_github_email_title")}
						</dt>
						<dd className="mt-1 text-xs leading-5 text-muted-foreground">
							{t("external_auth_provider_github_email_desc")}
						</dd>
					</div>
				) : null}
				<div>
					<dt className="text-xs text-muted-foreground">
						{t("external_auth_provider_claims")}
					</dt>
					<dd className="mt-1 break-words text-xs">{summaryClaims}</dd>
				</div>
				<div>
					<dt className="text-xs text-muted-foreground">
						{t("external_auth_provider_scopes")}
					</dt>
					<dd className="mt-1 break-words text-xs">
						{form.scopes.trim() || defaultScopesForKind(selectedKind)}
					</dd>
				</div>
				<div>
					<dt className="text-xs text-muted-foreground">
						{t("external_auth_provider_allowed_domains")}
					</dt>
					<dd className="mt-1 text-xs">
						{parseAllowedDomains(form.allowedDomains).join(", ") ||
							t("external_auth_provider_allowed_domains_all")}
					</dd>
				</div>
				{isCreate ? null : (
					<div>
						<dt className="text-xs text-muted-foreground">
							{t("external_auth_provider_callback_url")}
						</dt>
						<dd className="mt-1 break-all font-mono text-xs">
							{currentCallbackUrl || "-"}
						</dd>
					</div>
				)}
			</dl>
		</section>
	);
}

interface ExternalAuthProviderKindPanelProps {
	form: ExternalAuthProviderFormData;
	onProviderKindChange: (kind: ExternalAuthProviderKind) => void;
	providerKinds: AdminExternalAuthProviderKindInfo[];
}

export function ExternalAuthProviderKindPanel({
	form,
	onProviderKindChange,
	providerKinds,
}: ExternalAuthProviderKindPanelProps) {
	const { t } = useTranslation("admin");

	return (
		<div className="space-y-4">
			<div className="max-w-2xl">
				<h3 className="text-base font-semibold">
					{t("external_auth_provider_wizard_choose_type_title")}
				</h3>
				<p className="mt-1 text-sm text-muted-foreground">
					{t("external_auth_provider_wizard_choose_type_desc")}
				</p>
			</div>
			<div className="grid gap-4 md:grid-cols-2">
				{providerKinds.map((kind) => (
					<button
						type="button"
						key={kind.kind}
						aria-pressed={form.providerKind === kind.kind}
						onClick={() => onProviderKindChange(kind.kind)}
						className={
							form.providerKind === kind.kind
								? "rounded-3xl border border-primary bg-primary/5 p-5 text-left shadow-sm transition"
								: "rounded-3xl border border-border bg-background p-5 text-left transition hover:border-primary/40 hover:bg-muted/20"
						}
					>
						<div className="flex items-start gap-4">
							<div className="flex size-16 shrink-0 items-center justify-center rounded-2xl bg-white shadow-sm ring-1 ring-black/5">
								<ExternalAuthProviderIcon
									kind={kind.kind}
									className="max-h-10 max-w-10"
								/>
							</div>
							<div className="min-w-0 flex-1">
								<div className="flex flex-wrap items-center gap-2">
									<p className="text-base font-semibold">
										{kindDisplayName(t, kind.kind, providerKinds)}
									</p>
								</div>
								<p className="mt-2 text-sm leading-6 text-muted-foreground">
									{kindDescription(t, kind)}
								</p>
							</div>
						</div>
					</button>
				))}
			</div>
		</div>
	);
}

function GitHubProviderInfoPanel() {
	const { t } = useTranslation("admin");

	return (
		<div className="md:col-span-2 rounded-xl border border-border/70 bg-muted/25 p-4">
			<div className="flex items-start gap-3">
				<div className="flex size-11 shrink-0 items-center justify-center rounded-lg bg-white shadow-sm ring-1 ring-black/5">
					<ExternalAuthProviderIcon kind="github" className="max-h-7 max-w-7" />
				</div>
				<div className="min-w-0 flex-1">
					<p className="text-sm font-medium">
						{t("external_auth_provider_github_fixed_title")}
					</p>
					<p className="mt-1 text-xs leading-5 text-muted-foreground">
						{t("external_auth_provider_github_fixed_desc")}
					</p>
				</div>
			</div>
			<dl className="mt-3 grid gap-2 text-xs sm:grid-cols-2">
				<div className="min-w-0">
					<dt className="text-muted-foreground">
						{t("external_auth_provider_authorization_url")}
					</dt>
					<dd
						className="truncate font-mono"
						title={GITHUB_FIXED_ENDPOINTS.authorizationUrl}
					>
						{GITHUB_FIXED_ENDPOINTS.authorizationUrl}
					</dd>
				</div>
				<div className="min-w-0">
					<dt className="text-muted-foreground">
						{t("external_auth_provider_token_url")}
					</dt>
					<dd
						className="truncate font-mono"
						title={GITHUB_FIXED_ENDPOINTS.tokenUrl}
					>
						{GITHUB_FIXED_ENDPOINTS.tokenUrl}
					</dd>
				</div>
				<div className="min-w-0">
					<dt className="text-muted-foreground">
						{t("external_auth_provider_userinfo_url")}
					</dt>
					<dd
						className="truncate font-mono"
						title={GITHUB_FIXED_ENDPOINTS.userinfoUrl}
					>
						{GITHUB_FIXED_ENDPOINTS.userinfoUrl}
					</dd>
				</div>
				<div className="min-w-0">
					<dt className="text-muted-foreground">
						{t("external_auth_provider_github_user_emails_url")}
					</dt>
					<dd
						className="truncate font-mono"
						title={GITHUB_FIXED_ENDPOINTS.userEmailsUrl}
					>
						{GITHUB_FIXED_ENDPOINTS.userEmailsUrl}
					</dd>
				</div>
			</dl>
		</div>
	);
}

interface ExternalAuthProviderIdentityPanelProps {
	connectionMissing: boolean;
	createStepTouched: boolean;
	currentCallbackUrl: string;
	form: ExternalAuthProviderFormData;
	identityMissing: boolean;
	isCreate: boolean;
	onCopyCallbackUrl: (value: string) => void;
	onFieldChange: ExternalAuthProviderFieldChange;
	onTestConnection: () => Promise<boolean>;
	provider: AdminExternalAuthProviderInfo | null;
	providerKindLabel: string;
	selectedKind: AdminExternalAuthProviderKindInfo | null;
	showIssuerUrl: boolean;
	showManualEndpoints: boolean;
	testDisabled: boolean;
	testResult: string | null;
}

export function ExternalAuthProviderIdentityPanel({
	connectionMissing,
	createStepTouched,
	currentCallbackUrl,
	form,
	identityMissing,
	isCreate,
	onCopyCallbackUrl,
	onFieldChange,
	onTestConnection,
	provider,
	providerKindLabel,
	selectedKind,
	showIssuerUrl,
	showManualEndpoints,
	testDisabled,
	testResult,
}: ExternalAuthProviderIdentityPanelProps) {
	const { t } = useTranslation("admin");
	const isGitHub = isGitHubProviderKind(selectedKind);

	return (
		<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
			<div className="space-y-1">
				<h3 className="text-sm font-semibold">
					{t("external_auth_provider_identity_title")}
				</h3>
				<p className="text-sm text-muted-foreground">
					{t("external_auth_provider_identity_desc")}
				</p>
			</div>
			<div className="mt-4 grid gap-4 md:grid-cols-2">
				{isCreate ? null : (
					<div className="space-y-2">
						<p className="text-sm font-medium">
							{t("external_auth_provider_type")}
						</p>
						<div className="flex h-9 items-center">
							<Badge variant="outline">{providerKindLabel}</Badge>
						</div>
					</div>
				)}
				<div className={isCreate ? "space-y-2 md:col-span-2" : "space-y-2"}>
					<Label htmlFor="external-auth-provider-display-name">
						{t("external_auth_provider_display_name")}
					</Label>
					<Input
						id="external-auth-provider-display-name"
						value={form.displayName}
						maxLength={128}
						placeholder="Authentik"
						aria-invalid={
							createStepTouched && !form.displayName.trim() ? true : undefined
						}
						onChange={(event) =>
							onFieldChange("displayName", event.target.value)
						}
					/>
				</div>
				<div className="space-y-2 md:col-span-2">
					<Label htmlFor="external-auth-provider-icon-url">
						{t("external_auth_provider_icon_url")}
					</Label>
					<Input
						id="external-auth-provider-icon-url"
						value={form.iconUrl}
						placeholder="/static/external-auth/acme.svg"
						maxLength={2048}
						onChange={(event) => onFieldChange("iconUrl", event.target.value)}
					/>
					<p className="text-xs text-muted-foreground">
						{t("external_auth_provider_icon_url_hint")}
					</p>
				</div>
				<ExternalAuthConnectionFields
					createStepTouched={createStepTouched}
					form={form}
					onFieldChange={onFieldChange}
					provider={provider}
					selectedKind={selectedKind}
					showIssuerUrl={showIssuerUrl}
					showManualEndpoints={showManualEndpoints}
				/>
				{isGitHub ? <GitHubProviderInfoPanel /> : null}
				<ExternalAuthConnectionTestPanel
					disabled={testDisabled}
					onTestConnection={onTestConnection}
					testResult={testResult}
				/>
				{isCreate &&
				createStepTouched &&
				(identityMissing || connectionMissing) ? (
					<p className="text-xs text-destructive md:col-span-2">
						{t("external_auth_provider_wizard_required")}
					</p>
				) : null}
				{isCreate ? null : (
					<div className="min-w-0 space-y-2 md:col-span-2">
						<Label>{t("external_auth_provider_callback_url")}</Label>
						<CallbackUrlField
							value={currentCallbackUrl}
							onCopy={onCopyCallbackUrl}
						/>
						<p className="text-xs text-muted-foreground">
							{t("external_auth_provider_callback_url_hint")}
						</p>
					</div>
				)}
			</div>
		</section>
	);
}

function GitHubClaimInfoPanel() {
	const { t } = useTranslation("admin");

	return (
		<div className="md:col-span-2 rounded-xl border border-border/70 bg-muted/25 p-4">
			<p className="text-sm font-medium">
				{t("external_auth_provider_github_claims_title")}
			</p>
			<p className="mt-1 text-xs leading-5 text-muted-foreground">
				{t("external_auth_provider_github_claims_desc")}
			</p>
			<dl className="mt-3 grid gap-2 text-xs sm:grid-cols-2">
				<div>
					<dt className="text-muted-foreground">
						{t("external_auth_provider_subject_claim")}
					</dt>
					<dd className="font-mono">{GITHUB_CLAIMS.subjectClaim}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">
						{t("external_auth_provider_username_claim")}
					</dt>
					<dd className="font-mono">{GITHUB_CLAIMS.usernameClaim}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">
						{t("external_auth_provider_display_name_claim")}
					</dt>
					<dd className="font-mono">{GITHUB_CLAIMS.displayNameClaim}</dd>
				</div>
				<div>
					<dt className="text-muted-foreground">
						{t("external_auth_provider_email_claim")}
					</dt>
					<dd className="font-mono">{GITHUB_CLAIMS.emailClaim}</dd>
				</div>
			</dl>
		</div>
	);
}

interface ExternalAuthProviderRulesPanelProps {
	form: ExternalAuthProviderFormData;
	onFieldChange: ExternalAuthProviderFieldChange;
	selectedKind: AdminExternalAuthProviderKindInfo | null;
}

export function ExternalAuthProviderRulesPanel({
	form,
	onFieldChange,
	selectedKind,
}: ExternalAuthProviderRulesPanelProps) {
	const { t } = useTranslation("admin");
	const isGitHub = isGitHubProviderKind(selectedKind);

	return (
		<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
			<div className="space-y-1">
				<h3 className="text-sm font-semibold">
					{t("external_auth_provider_rules_title")}
				</h3>
				<p className="text-sm text-muted-foreground">
					{t("external_auth_provider_rules_desc")}
				</p>
			</div>
			<div className="mt-4 grid gap-4 md:grid-cols-2">
				<div className="space-y-2 md:col-span-2">
					<Label htmlFor="external-auth-provider-scopes">
						{t("external_auth_provider_scopes")}
					</Label>
					<Input
						id="external-auth-provider-scopes"
						value={form.scopes}
						placeholder={defaultScopesForKind(selectedKind)}
						onChange={(event) => onFieldChange("scopes", event.target.value)}
					/>
					<p className="text-xs text-muted-foreground">
						{t("external_auth_provider_scopes_hint")}
					</p>
				</div>
				<div className="space-y-2 md:col-span-2">
					<Label htmlFor="external-auth-provider-allowed-domains">
						{t("external_auth_provider_allowed_domains")}
					</Label>
					<Input
						id="external-auth-provider-allowed-domains"
						value={form.allowedDomains}
						placeholder="example.com, example.org"
						onChange={(event) =>
							onFieldChange("allowedDomains", event.target.value)
						}
					/>
					<p className="text-xs text-muted-foreground">
						{t("external_auth_provider_allowed_domains_hint")}
					</p>
				</div>
				{isGitHub ? (
					<GitHubClaimInfoPanel />
				) : (
					<ExternalAuthClaimFields
						form={form}
						onFieldChange={onFieldChange}
						selectedKind={selectedKind}
					/>
				)}
			</div>
		</section>
	);
}
