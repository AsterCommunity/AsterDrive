import type { TFunction } from "i18next";
import {
	type MouseEvent,
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { TestConnectionButton } from "@/components/admin/TestConnectionButton";
import {
	ADMIN_INTERACTIVE_TABLE_ROW_CLASS,
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_MUTED_TEXT_CLASS,
	ADMIN_TABLE_STACKED_CELL_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminTable,
	AdminTableBody,
	AdminTableShell,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { AdminSurface } from "@/components/layout/AdminSurface";
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
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { writeTextToClipboard } from "@/lib/clipboard";
import {
	ADMIN_CONTROL_HEIGHT_CLASS,
	ADMIN_ICON_BUTTON_CLASS,
} from "@/lib/constants";
import {
	externalAuthKindIconPath,
	normalizeExternalAuthIconUrl,
} from "@/lib/externalAuthProviders";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import {
	buildOffsetPaginationSearchParams,
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
} from "@/lib/pagination";
import { absoluteAppUrl } from "@/lib/publicSiteUrl";
import { cn } from "@/lib/utils";
import { adminExternalAuthService } from "@/services/adminService";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
	CreateExternalAuthProviderInput,
	ExternalAuthProviderKind,
	ExternalAuthProviderTestParamsInput,
	ExternalAuthProviderTestResult,
	UpdateExternalAuthProviderInput,
} from "@/types/api";

const DEFAULT_SCOPES = "openid email profile";
const REDACTED_SECRET = "***REDACTED***";
const STANDARD_CLAIMS = {
	avatarUrlClaim: "picture",
	displayNameClaim: "name",
	emailClaim: "email",
	emailVerifiedClaim: "email_verified",
	groupsClaim: "groups",
	subjectClaim: "sub",
	usernameClaim: "preferred_username",
} as const;
const EXTERNAL_AUTH_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_EXTERNAL_AUTH_PAGE_SIZE = 20 as const;
const EXTERNAL_AUTH_MANAGED_QUERY_KEYS = ["offset", "pageSize"] as const;

interface ExternalAuthProviderFormData {
	allowedDomains: string;
	authorizationUrl: string;
	autoLinkVerifiedEmailEnabled: boolean;
	autoProvisionEnabled: boolean;
	avatarUrlClaim: string;
	clientId: string;
	clientSecret: string;
	displayName: string;
	displayNameClaim: string;
	emailClaim: string;
	emailVerifiedClaim: string;
	enabled: boolean;
	groupsClaim: string;
	iconUrl: string;
	issuerUrl: string;
	key: string;
	providerKind: ExternalAuthProviderKind;
	requireEmailVerified: boolean;
	scopes: string;
	subjectClaim: string;
	tokenUrl: string;
	userinfoUrl: string;
	usernameClaim: string;
}

interface ExternalAuthCreateStep {
	title: string;
	description: string;
}

const emptyForm: ExternalAuthProviderFormData = {
	allowedDomains: "",
	authorizationUrl: "",
	autoLinkVerifiedEmailEnabled: false,
	autoProvisionEnabled: false,
	avatarUrlClaim: "",
	clientId: "",
	clientSecret: "",
	displayName: "",
	displayNameClaim: "",
	emailClaim: "",
	emailVerifiedClaim: "",
	enabled: true,
	groupsClaim: "",
	iconUrl: "",
	issuerUrl: "",
	key: "",
	providerKind: "oidc",
	requireEmailVerified: true,
	scopes: DEFAULT_SCOPES,
	subjectClaim: "",
	tokenUrl: "",
	userinfoUrl: "",
	usernameClaim: "",
};

function formFromProvider(
	provider: AdminExternalAuthProviderInfo,
): ExternalAuthProviderFormData {
	return {
		allowedDomains: provider.allowed_domains.join(", "),
		authorizationUrl: provider.authorization_url ?? "",
		autoLinkVerifiedEmailEnabled: provider.auto_link_verified_email_enabled,
		autoProvisionEnabled: provider.auto_provision_enabled,
		avatarUrlClaim: provider.avatar_url_claim ?? "",
		clientId: provider.client_id,
		clientSecret: provider.client_secret ?? "",
		displayName: provider.display_name,
		displayNameClaim: provider.display_name_claim ?? "",
		emailClaim: provider.email_claim ?? "",
		emailVerifiedClaim: provider.email_verified_claim ?? "",
		enabled: provider.enabled,
		groupsClaim: provider.groups_claim ?? "",
		iconUrl: provider.icon_url ?? "",
		issuerUrl: provider.issuer_url ?? "",
		key: provider.key,
		providerKind: provider.provider_kind,
		requireEmailVerified: provider.require_email_verified,
		scopes: provider.scopes || DEFAULT_SCOPES,
		subjectClaim: provider.subject_claim ?? "",
		tokenUrl: provider.token_url ?? "",
		userinfoUrl: provider.userinfo_url ?? "",
		usernameClaim: provider.username_claim ?? "",
	};
}

function kindFallbackLabel(kind: ExternalAuthProviderKind) {
	switch (kind) {
		case "oidc":
			return "OpenID Connect";
	}
}

function localizedProviderKindText(
	t: TFunction,
	key: string,
	fallback: string,
) {
	const translated = t(key);
	return translated === key ? fallback : translated;
}

function ExternalAuthProviderIcon({
	className,
	iconUrl,
	kind,
}: {
	className?: string;
	iconUrl?: string | null;
	kind: ExternalAuthProviderKind;
}) {
	const configuredIcon = normalizeExternalAuthIconUrl(iconUrl);
	const kindIcon = externalAuthKindIconPath(kind);
	const effectiveIcon = configuredIcon || kindIcon;

	if (effectiveIcon) {
		return (
			<img
				src={effectiveIcon}
				alt=""
				aria-hidden="true"
				className={cn("object-contain", className)}
				onError={(event) => {
					if (
						configuredIcon &&
						kindIcon &&
						event.currentTarget.src !== kindIcon
					) {
						event.currentTarget.src = kindIcon;
						return;
					}
					event.currentTarget.hidden = true;
				}}
			/>
		);
	}

	return <Icon name="SignIn" className={cn("text-primary", className)} />;
}

function kindDisplayName(
	t: TFunction,
	kind: ExternalAuthProviderKind,
	providerKinds: AdminExternalAuthProviderKindInfo[],
) {
	const fallback =
		providerKinds.find((item) => item.kind === kind)?.display_name ??
		kindFallbackLabel(kind);
	return localizedProviderKindText(
		t,
		`external_auth_provider_kind_${kind}_name`,
		fallback,
	);
}

function kindDescription(
	t: TFunction,
	kind: AdminExternalAuthProviderKindInfo,
) {
	return localizedProviderKindText(
		t,
		`external_auth_provider_kind_${kind.kind}_description`,
		kind.description,
	);
}

function parseAllowedDomains(value: string) {
	return value
		.split(/[,\n]/)
		.map((domain) => domain.trim().replace(/^@+/, "").toLowerCase())
		.filter(
			(domain, index, domains) => domain && domains.indexOf(domain) === index,
		);
}

function nullableText(value: string) {
	const trimmed = value.trim();
	return trimmed ? trimmed : null;
}

function nullableSecretText(value: string) {
	const trimmed = value.trim();
	return trimmed && trimmed !== REDACTED_SECRET ? trimmed : null;
}

function effectiveClaim(value: string | null | undefined, fallback: string) {
	return value?.trim() || fallback;
}

function createPayload(
	form: ExternalAuthProviderFormData,
): CreateExternalAuthProviderInput {
	const allowedDomains = parseAllowedDomains(form.allowedDomains);
	return {
		allowed_domains: allowedDomains.length > 0 ? allowedDomains : null,
		authorization_url: nullableText(form.authorizationUrl),
		auto_link_verified_email_enabled: form.autoLinkVerifiedEmailEnabled,
		auto_provision_enabled: form.autoProvisionEnabled,
		avatar_url_claim: nullableText(form.avatarUrlClaim),
		client_id: form.clientId.trim(),
		client_secret: nullableText(form.clientSecret),
		display_name: form.displayName.trim(),
		display_name_claim: nullableText(form.displayNameClaim),
		email_claim: nullableText(form.emailClaim),
		email_verified_claim: nullableText(form.emailVerifiedClaim),
		enabled: form.enabled,
		groups_claim: nullableText(form.groupsClaim),
		icon_url: nullableText(form.iconUrl),
		issuer_url: nullableText(form.issuerUrl),
		provider_kind: form.providerKind,
		require_email_verified: form.requireEmailVerified,
		scopes: form.scopes.trim() || DEFAULT_SCOPES,
		subject_claim: nullableText(form.subjectClaim),
		token_url: nullableText(form.tokenUrl),
		userinfo_url: nullableText(form.userinfoUrl),
		username_claim: nullableText(form.usernameClaim),
	};
}

function updatePayload(
	form: ExternalAuthProviderFormData,
): UpdateExternalAuthProviderInput {
	const allowedDomains = parseAllowedDomains(form.allowedDomains);
	return {
		allowed_domains: allowedDomains.length > 0 ? allowedDomains : null,
		authorization_url: nullableText(form.authorizationUrl),
		auto_link_verified_email_enabled: form.autoLinkVerifiedEmailEnabled,
		auto_provision_enabled: form.autoProvisionEnabled,
		avatar_url_claim: nullableText(form.avatarUrlClaim),
		client_id: form.clientId.trim(),
		client_secret: nullableText(form.clientSecret),
		display_name: form.displayName.trim(),
		display_name_claim: nullableText(form.displayNameClaim),
		email_claim: nullableText(form.emailClaim),
		email_verified_claim: nullableText(form.emailVerifiedClaim),
		enabled: form.enabled,
		groups_claim: nullableText(form.groupsClaim),
		icon_url: nullableText(form.iconUrl),
		issuer_url: nullableText(form.issuerUrl),
		require_email_verified: form.requireEmailVerified,
		scopes: form.scopes.trim() || DEFAULT_SCOPES,
		subject_claim: nullableText(form.subjectClaim),
		token_url: nullableText(form.tokenUrl),
		userinfo_url: nullableText(form.userinfoUrl),
		username_claim: nullableText(form.usernameClaim),
	};
}

function testParamsPayload(
	form: ExternalAuthProviderFormData,
): ExternalAuthProviderTestParamsInput {
	return {
		authorization_url: nullableText(form.authorizationUrl),
		client_id: form.clientId.trim(),
		client_secret: nullableSecretText(form.clientSecret),
		issuer_url: nullableText(form.issuerUrl),
		provider_kind: form.providerKind,
		scopes: form.scopes.trim() || DEFAULT_SCOPES,
		token_url: nullableText(form.tokenUrl),
		userinfo_url: nullableText(form.userinfoUrl),
	};
}

function normalizeConnectionValue(value: string | null | undefined) {
	return value?.trim() ?? "";
}

function formClientSecretChanged(
	form: ExternalAuthProviderFormData,
	provider: AdminExternalAuthProviderInfo,
) {
	const value = normalizeConnectionValue(form.clientSecret);
	return provider.client_secret_configured
		? value !== REDACTED_SECRET
		: value !== "";
}

function formConnectionChanged(
	form: ExternalAuthProviderFormData,
	provider: AdminExternalAuthProviderInfo,
) {
	return (
		form.providerKind !== provider.provider_kind ||
		normalizeConnectionValue(form.issuerUrl) !==
			normalizeConnectionValue(provider.issuer_url) ||
		normalizeConnectionValue(form.authorizationUrl) !==
			normalizeConnectionValue(provider.authorization_url) ||
		normalizeConnectionValue(form.tokenUrl) !==
			normalizeConnectionValue(provider.token_url) ||
		normalizeConnectionValue(form.userinfoUrl) !==
			normalizeConnectionValue(provider.userinfo_url) ||
		normalizeConnectionValue(form.clientId) !==
			normalizeConnectionValue(provider.client_id) ||
		(form.scopes.trim() || DEFAULT_SCOPES) !==
			(provider.scopes.trim() || DEFAULT_SCOPES) ||
		formClientSecretChanged(form, provider)
	);
}

function formatTestResultSummary(
	t: TFunction,
	result: ExternalAuthProviderTestResult,
) {
	return result.checks.length > 0
		? result.checks
				.map((check) =>
					t(
						check.success
							? "external_auth_provider_test_check_ok"
							: "external_auth_provider_test_check_error",
						{
							name: check.name,
							message: check.message,
						},
					),
				)
				.join(" · ")
		: t("external_auth_provider_test_success_detail", {
				provider: result.provider,
			});
}

function providerStatusTone(provider: AdminExternalAuthProviderInfo) {
	return provider.enabled
		? "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/60 dark:text-emerald-300"
		: "border-slate-200 bg-slate-50 text-slate-700 dark:border-slate-800 dark:bg-slate-950/50 dark:text-slate-300";
}

function securityModeLabel(
	t: TFunction,
	provider: AdminExternalAuthProviderInfo,
) {
	if (
		provider.auto_provision_enabled &&
		provider.auto_link_verified_email_enabled
	) {
		return t("external_auth_provider_mode_link_and_provision");
	}
	if (provider.auto_provision_enabled) {
		return t("external_auth_provider_mode_provision");
	}
	if (provider.auto_link_verified_email_enabled) {
		return t("external_auth_provider_mode_link");
	}
	return t("external_auth_provider_mode_manual");
}

function callbackPath(
	providerKind: ExternalAuthProviderKind,
	providerKey: string,
) {
	const key = providerKey.trim();
	return key
		? `/api/v1/auth/external-auth/${encodeURIComponent(providerKind)}/${encodeURIComponent(key)}/callback`
		: null;
}

function callbackUrl(
	providerKind: ExternalAuthProviderKind,
	providerKey: string,
) {
	const path = callbackPath(providerKind, providerKey);
	return path ? absoluteAppUrl(path) : "";
}

function providerPrimaryEndpoint(provider: AdminExternalAuthProviderInfo) {
	if (provider.issuer_url) {
		return {
			labelKey: "external_auth_provider_issuer_url",
			value: provider.issuer_url,
		};
	}
	if (provider.authorization_url) {
		return {
			labelKey: "external_auth_provider_authorization_url",
			value: provider.authorization_url,
		};
	}
	if (provider.token_url) {
		return {
			labelKey: "external_auth_provider_token_url",
			value: provider.token_url,
		};
	}
	if (provider.userinfo_url) {
		return {
			labelKey: "external_auth_provider_userinfo_url",
			value: provider.userinfo_url,
		};
	}
	return null;
}

function providerAllowedDomainSummary(
	t: TFunction,
	provider: AdminExternalAuthProviderInfo,
) {
	return provider.allowed_domains.length > 0
		? provider.allowed_domains.join(", ")
		: t("external_auth_provider_allowed_domains_all");
}

function normalizeOffset(offset: number) {
	return Math.max(0, Math.floor(offset));
}

function buildManagedExternalAuthSearchParams({
	offset,
	pageSize,
}: {
	offset: number;
	pageSize: (typeof EXTERNAL_AUTH_PAGE_SIZE_OPTIONS)[number];
}) {
	return buildOffsetPaginationSearchParams({
		offset,
		pageSize,
		defaultPageSize: DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
	});
}

function getManagedExternalAuthSearchString(searchParams: URLSearchParams) {
	return buildManagedExternalAuthSearchParams({
		offset: normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
		pageSize: parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
			DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
		),
	}).toString();
}

function mergeManagedExternalAuthSearchParams(
	searchParams: URLSearchParams,
	managedSearchParams: URLSearchParams,
) {
	const merged = new URLSearchParams(searchParams);
	for (const key of EXTERNAL_AUTH_MANAGED_QUERY_KEYS) {
		merged.delete(key);
	}
	for (const [key, value] of managedSearchParams.entries()) {
		merged.set(key, value);
	}
	return merged;
}

function shouldShowIssuerUrl(kind: AdminExternalAuthProviderKindInfo | null) {
	return Boolean(kind?.supports_discovery || kind?.issuer_url_required);
}

function shouldShowManualEndpoints(
	kind: AdminExternalAuthProviderKindInfo | null,
) {
	return Boolean(kind?.manual_endpoint_configuration_supported);
}

function connectionRequirementsMissing(
	form: ExternalAuthProviderFormData,
	kind: AdminExternalAuthProviderKindInfo | null,
) {
	if (!form.clientId.trim()) {
		return true;
	}
	if ((kind?.issuer_url_required ?? true) && !form.issuerUrl.trim()) {
		return true;
	}
	if (kind?.authorization_url_required && !form.authorizationUrl.trim()) {
		return true;
	}
	if (kind?.token_url_required && !form.tokenUrl.trim()) {
		return true;
	}
	if (kind?.userinfo_url_required && !form.userinfoUrl.trim()) {
		return true;
	}
	return false;
}

function requiredFieldsMissing(
	form: ExternalAuthProviderFormData,
	kind: AdminExternalAuthProviderKindInfo | null,
) {
	return !form.displayName.trim() || connectionRequirementsMissing(form, kind);
}

function formConnectionSummary(
	form: ExternalAuthProviderFormData,
	selectedKind: AdminExternalAuthProviderKindInfo | null,
) {
	const items = [
		form.issuerUrl.trim() ? `issuer: ${form.issuerUrl.trim()}` : null,
		selectedKind?.manual_endpoint_configuration_supported &&
		form.authorizationUrl.trim()
			? `authorization: ${form.authorizationUrl.trim()}`
			: null,
		selectedKind?.manual_endpoint_configuration_supported &&
		form.tokenUrl.trim()
			? `token: ${form.tokenUrl.trim()}`
			: null,
		selectedKind?.manual_endpoint_configuration_supported &&
		form.userinfoUrl.trim()
			? `userinfo: ${form.userinfoUrl.trim()}`
			: null,
	]
		.filter((item): item is string => item !== null)
		.join(" · ");
	return items || "-";
}

function formClaimSummary(
	form: ExternalAuthProviderFormData,
	selectedKind: AdminExternalAuthProviderKindInfo | null,
) {
	const claims = [
		`subject=${effectiveClaim(form.subjectClaim, STANDARD_CLAIMS.subjectClaim)}`,
		`username=${effectiveClaim(form.usernameClaim, STANDARD_CLAIMS.usernameClaim)}`,
		`display=${effectiveClaim(form.displayNameClaim, STANDARD_CLAIMS.displayNameClaim)}`,
		`email=${effectiveClaim(form.emailClaim, STANDARD_CLAIMS.emailClaim)}`,
		selectedKind?.supports_email_verified_claim
			? `email_verified=${effectiveClaim(form.emailVerifiedClaim, STANDARD_CLAIMS.emailVerifiedClaim)}`
			: null,
		`groups=${effectiveClaim(form.groupsClaim, STANDARD_CLAIMS.groupsClaim)}`,
		`avatar=${effectiveClaim(form.avatarUrlClaim, STANDARD_CLAIMS.avatarUrlClaim)}`,
	]
		.filter((item): item is string => item !== null)
		.join(" · ");
	return claims || "-";
}

function providerIconSummary(form: ExternalAuthProviderFormData) {
	return form.iconUrl.trim() || "-";
}

interface CreateProgressProps {
	createStep: number;
	createSteps: ExternalAuthCreateStep[];
	onCreateStepChange: (step: number) => void;
}

function CreateProgress({
	createStep,
	createSteps,
	onCreateStepChange,
}: CreateProgressProps) {
	const { t } = useTranslation("admin");
	const currentStep = createSteps[Math.min(createStep, createSteps.length - 1)];

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
				<div className="mt-4 h-1.5 overflow-hidden rounded-full bg-background/80">
					<div
						className="h-full rounded-full bg-primary transition-[width] duration-300"
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
							"flex items-center gap-3 rounded-2xl border px-3 py-3 text-left transition",
							index === createStep
								? "border-primary bg-primary/5"
								: index < createStep
									? "border-border/80 bg-background hover:border-primary/40"
									: "border-border/60 bg-background/70 text-muted-foreground",
						)}
					>
						<span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-border/70 bg-background/80 text-[10px] font-semibold tracking-[0.16em] text-muted-foreground">
							{index + 1}
						</span>
						<span className="text-sm font-medium leading-5">{step.title}</span>
					</button>
				))}
			</div>
		</div>
	);
}

interface CallbackUrlFieldProps {
	className?: string;
	onCopy: (value: string) => void;
	value: string;
}

function CallbackUrlField({ className, onCopy, value }: CallbackUrlFieldProps) {
	const { t } = useTranslation("admin");
	const disabled = !value;

	return (
		<div
			className={cn(
				"flex min-w-0 w-full max-w-full items-center gap-2 overflow-hidden rounded-md border border-border/70 bg-muted/30 p-1",
				className,
			)}
		>
			<code
				className="block min-w-0 flex-1 select-all overflow-x-auto whitespace-nowrap px-2 py-1 font-mono text-xs text-foreground [scrollbar-width:thin]"
				title={value || "-"}
			>
				{value || "-"}
			</code>
			<Button
				type="button"
				variant="ghost"
				size="icon"
				className="h-7 w-7 shrink-0"
				disabled={disabled}
				aria-label={t("external_auth_provider_copy_callback_url")}
				title={t("external_auth_provider_copy_callback_url")}
				onClick={(event: MouseEvent<HTMLButtonElement>) => {
					event.stopPropagation();
					if (!disabled) {
						onCopy(value);
					}
				}}
			>
				<Icon name="Copy" className="h-3.5 w-3.5" />
			</Button>
		</div>
	);
}

interface ConnectionFieldsProps {
	createStepTouched: boolean;
	form: ExternalAuthProviderFormData;
	onFieldChange: <K extends keyof ExternalAuthProviderFormData>(
		key: K,
		value: ExternalAuthProviderFormData[K],
	) => void;
	provider: AdminExternalAuthProviderInfo | null;
	selectedKind: AdminExternalAuthProviderKindInfo | null;
	showIssuerUrl: boolean;
	showManualEndpoints: boolean;
	t: TFunction;
}

function ConnectionFields({
	createStepTouched,
	form,
	onFieldChange,
	provider,
	selectedKind,
	showIssuerUrl,
	showManualEndpoints,
	t,
}: ConnectionFieldsProps) {
	return (
		<>
			{showIssuerUrl ? (
				<div className="space-y-2 md:col-span-2">
					<Label htmlFor="external-auth-provider-issuer">
						{t("external_auth_provider_issuer_url")}
					</Label>
					<Input
						id="external-auth-provider-issuer"
						value={form.issuerUrl}
						placeholder="https://id.example.com/application/o/asterdrive/"
						aria-invalid={
							createStepTouched &&
							selectedKind?.issuer_url_required &&
							!form.issuerUrl.trim()
								? true
								: undefined
						}
						onChange={(event) => onFieldChange("issuerUrl", event.target.value)}
					/>
				</div>
			) : null}
			{showManualEndpoints ? (
				<>
					<div className="space-y-2 md:col-span-2">
						<Label htmlFor="external-auth-provider-authorization-url">
							{t("external_auth_provider_authorization_url")}
						</Label>
						<Input
							id="external-auth-provider-authorization-url"
							value={form.authorizationUrl}
							placeholder="https://id.example.com/oauth/authorize"
							aria-invalid={
								createStepTouched &&
								selectedKind?.authorization_url_required &&
								!form.authorizationUrl.trim()
									? true
									: undefined
							}
							onChange={(event) =>
								onFieldChange("authorizationUrl", event.target.value)
							}
						/>
					</div>
					<div className="space-y-2">
						<Label htmlFor="external-auth-provider-token-url">
							{t("external_auth_provider_token_url")}
						</Label>
						<Input
							id="external-auth-provider-token-url"
							value={form.tokenUrl}
							placeholder="https://id.example.com/oauth/token"
							aria-invalid={
								createStepTouched &&
								selectedKind?.token_url_required &&
								!form.tokenUrl.trim()
									? true
									: undefined
							}
							onChange={(event) =>
								onFieldChange("tokenUrl", event.target.value)
							}
						/>
					</div>
					<div className="space-y-2">
						<Label htmlFor="external-auth-provider-userinfo-url">
							{t("external_auth_provider_userinfo_url")}
						</Label>
						<Input
							id="external-auth-provider-userinfo-url"
							value={form.userinfoUrl}
							placeholder="https://id.example.com/oauth/userinfo"
							aria-invalid={
								createStepTouched &&
								selectedKind?.userinfo_url_required &&
								!form.userinfoUrl.trim()
									? true
									: undefined
							}
							onChange={(event) =>
								onFieldChange("userinfoUrl", event.target.value)
							}
						/>
					</div>
				</>
			) : null}
			<div className="space-y-2">
				<Label htmlFor="external-auth-provider-client-id">
					{t("external_auth_provider_client_id")}
				</Label>
				<Input
					id="external-auth-provider-client-id"
					value={form.clientId}
					aria-invalid={
						createStepTouched && !form.clientId.trim() ? true : undefined
					}
					onChange={(event) => onFieldChange("clientId", event.target.value)}
				/>
			</div>
			<div className="space-y-2">
				<Label htmlFor="external-auth-provider-client-secret">
					{t("external_auth_provider_client_secret")}
				</Label>
				<Input
					id="external-auth-provider-client-secret"
					type="password"
					value={form.clientSecret}
					placeholder={
						provider?.client_secret_configured
							? t("external_auth_provider_secret_keep_placeholder")
							: ""
					}
					onChange={(event) =>
						onFieldChange("clientSecret", event.target.value)
					}
				/>
				<p className="text-xs text-muted-foreground">
					{provider?.client_secret_configured
						? t("external_auth_provider_secret_keep_hint")
						: t("external_auth_provider_secret_hint")}
				</p>
			</div>
		</>
	);
}

interface ClaimFieldsProps {
	form: ExternalAuthProviderFormData;
	onFieldChange: <K extends keyof ExternalAuthProviderFormData>(
		key: K,
		value: ExternalAuthProviderFormData[K],
	) => void;
	selectedKind: AdminExternalAuthProviderKindInfo | null;
	t: TFunction;
}

function ClaimFields({
	form,
	onFieldChange,
	selectedKind,
	t,
}: ClaimFieldsProps) {
	return (
		<>
			<div className="space-y-2">
				<Label htmlFor="external-auth-provider-subject-claim">
					{t("external_auth_provider_subject_claim")}
				</Label>
				<Input
					id="external-auth-provider-subject-claim"
					value={form.subjectClaim}
					placeholder="sub"
					onChange={(event) =>
						onFieldChange("subjectClaim", event.target.value)
					}
				/>
				<p className="text-xs text-muted-foreground">
					{t("external_auth_provider_claim_default_hint", {
						claim: STANDARD_CLAIMS.subjectClaim,
					})}
				</p>
			</div>
			<div className="space-y-2">
				<Label htmlFor="external-auth-provider-username-claim">
					{t("external_auth_provider_username_claim")}
				</Label>
				<Input
					id="external-auth-provider-username-claim"
					value={form.usernameClaim}
					placeholder="preferred_username"
					onChange={(event) =>
						onFieldChange("usernameClaim", event.target.value)
					}
				/>
				<p className="text-xs text-muted-foreground">
					{t("external_auth_provider_claim_default_hint", {
						claim: STANDARD_CLAIMS.usernameClaim,
					})}
				</p>
			</div>
			<div className="space-y-2">
				<Label htmlFor="external-auth-provider-display-claim">
					{t("external_auth_provider_display_name_claim")}
				</Label>
				<Input
					id="external-auth-provider-display-claim"
					value={form.displayNameClaim}
					placeholder="name"
					onChange={(event) =>
						onFieldChange("displayNameClaim", event.target.value)
					}
				/>
				<p className="text-xs text-muted-foreground">
					{t("external_auth_provider_claim_default_hint", {
						claim: STANDARD_CLAIMS.displayNameClaim,
					})}
				</p>
			</div>
			<div className="space-y-2">
				<Label htmlFor="external-auth-provider-email-claim">
					{t("external_auth_provider_email_claim")}
				</Label>
				<Input
					id="external-auth-provider-email-claim"
					value={form.emailClaim}
					placeholder="email"
					onChange={(event) => onFieldChange("emailClaim", event.target.value)}
				/>
				<p className="text-xs text-muted-foreground">
					{t("external_auth_provider_claim_default_hint", {
						claim: STANDARD_CLAIMS.emailClaim,
					})}
				</p>
			</div>
			<div className="space-y-2">
				<Label htmlFor="external-auth-provider-groups-claim">
					{t("external_auth_provider_groups_claim")}
				</Label>
				<Input
					id="external-auth-provider-groups-claim"
					value={form.groupsClaim}
					placeholder="groups"
					onChange={(event) => onFieldChange("groupsClaim", event.target.value)}
				/>
				<p className="text-xs text-muted-foreground">
					{t("external_auth_provider_claim_default_hint", {
						claim: STANDARD_CLAIMS.groupsClaim,
					})}
				</p>
			</div>
			{selectedKind?.supports_email_verified_claim ? (
				<div className="space-y-2">
					<Label htmlFor="external-auth-provider-email-verified-claim">
						{t("external_auth_provider_email_verified_claim")}
					</Label>
					<Input
						id="external-auth-provider-email-verified-claim"
						value={form.emailVerifiedClaim}
						placeholder="email_verified"
						onChange={(event) =>
							onFieldChange("emailVerifiedClaim", event.target.value)
						}
					/>
					<p className="text-xs text-muted-foreground">
						{t("external_auth_provider_claim_default_hint", {
							claim: STANDARD_CLAIMS.emailVerifiedClaim,
						})}
					</p>
				</div>
			) : null}
			<div className="space-y-2">
				<Label htmlFor="external-auth-provider-avatar-claim">
					{t("external_auth_provider_avatar_url_claim")}
				</Label>
				<Input
					id="external-auth-provider-avatar-claim"
					value={form.avatarUrlClaim}
					placeholder="picture"
					onChange={(event) =>
						onFieldChange("avatarUrlClaim", event.target.value)
					}
				/>
				<p className="text-xs text-muted-foreground">
					{t("external_auth_provider_claim_default_hint", {
						claim: STANDARD_CLAIMS.avatarUrlClaim,
					})}
				</p>
			</div>
		</>
	);
}

interface ProviderDialogProps {
	createStep: number;
	createStepDirection: "idle" | "forward" | "backward";
	createStepTouched: boolean;
	createSteps: ExternalAuthCreateStep[];
	form: ExternalAuthProviderFormData;
	mode: "create" | "edit";
	onCreateBack: () => void;
	onCreateNext: () => void;
	onCreateStepChange: (step: number) => void;
	onFieldChange: <K extends keyof ExternalAuthProviderFormData>(
		key: K,
		value: ExternalAuthProviderFormData[K],
	) => void;
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

function ProviderDialog({
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
}: ProviderDialogProps) {
	const { t } = useTranslation("admin");
	const isCreate = mode === "create";
	const createLastStep = createSteps.length - 1;
	const providerKind = provider?.provider_kind ?? form.providerKind;
	const selectedKind =
		providerKinds.find((item) => item.kind === providerKind) ??
		providerKinds[0] ??
		null;
	const providerKindLabel = kindDisplayName(t, providerKind, providerKinds);
	const showIssuerUrl = Boolean(
		shouldShowIssuerUrl(selectedKind) || form.issuerUrl.trim(),
	);
	const showManualEndpoints = Boolean(
		shouldShowManualEndpoints(selectedKind) ||
			form.authorizationUrl.trim() ||
			form.tokenUrl.trim() ||
			form.userinfoUrl.trim(),
	);
	const stepAnimationKey = `${createStep}-${createStepDirection}`;
	const requiredMissing = requiredFieldsMissing(form, selectedKind);
	const currentCallbackUrl = callbackUrl(providerKind, form.key);
	const identityMissing = !form.displayName.trim();
	const connectionMissing = connectionRequirementsMissing(form, selectedKind);
	const testDisabled = submitting || connectionMissing;
	const submitDisabled = submitting || requiredMissing;
	const summaryConnection = formConnectionSummary(form, selectedKind);
	const summaryClaims = formClaimSummary(form, selectedKind);
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
	const accessPolicyPanel = (
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
	const connectionTestPanel = (
		<div className="min-w-0 space-y-2 md:col-span-2">
			<div className="flex min-w-0 flex-wrap items-center gap-3">
				<TestConnectionButton
					disabled={testDisabled}
					onTest={onTestConnection}
				/>
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
	const summaryPanel = (
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
						{form.scopes.trim() ||
							selectedKind?.default_scopes ||
							DEFAULT_SCOPES}
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
								<CreateProgress
									createStep={createStep}
									createSteps={createSteps}
									onCreateStepChange={onCreateStepChange}
								/>
								<div className="rounded-2xl border border-border/70 bg-background/70 p-5">
									<div className="relative overflow-hidden">
										<div
											key={stepAnimationKey}
											data-testid="external-auth-provider-step-panel"
											className={stepPanelClass}
										>
											{createStep === 0 ? (
												<div className="space-y-4">
													<div className="max-w-2xl">
														<h3 className="text-base font-semibold">
															{t(
																"external_auth_provider_wizard_choose_type_title",
															)}
														</h3>
														<p className="mt-1 text-sm text-muted-foreground">
															{t(
																"external_auth_provider_wizard_choose_type_desc",
															)}
														</p>
													</div>
													<div className="grid gap-4 md:grid-cols-2">
														{providerKinds.map((kind) => (
															<button
																type="button"
																key={kind.kind}
																aria-pressed={form.providerKind === kind.kind}
																onClick={() => onProviderKindChange(kind.kind)}
																className={cn(
																	"rounded-2xl border p-5 text-left transition",
																	form.providerKind === kind.kind
																		? "border-primary bg-primary/5 shadow-sm"
																		: "border-border bg-background hover:border-primary/40 hover:bg-muted/20",
																)}
															>
																<div className="flex items-start gap-4">
																	<div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-white shadow-xs ring-1 ring-black/5 dark:bg-slate-950 dark:ring-white/10">
																		<ExternalAuthProviderIcon
																			kind={kind.kind}
																			className="h-8 w-8"
																		/>
																	</div>
																	<div className="min-w-0 flex-1">
																		<div className="flex flex-wrap items-center gap-2">
																			<p className="text-base font-semibold">
																				{kindDisplayName(
																					t,
																					kind.kind,
																					providerKinds,
																				)}
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
											) : createStep === 1 ? (
												<div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_18rem]">
													<div className="min-w-0 space-y-4">
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
																<div className="space-y-2 md:col-span-2">
																	<Label htmlFor="external-auth-provider-display-name">
																		{t("external_auth_provider_display_name")}
																	</Label>
																	<Input
																		id="external-auth-provider-display-name"
																		value={form.displayName}
																		maxLength={128}
																		placeholder="Authentik"
																		aria-invalid={
																			createStepTouched &&
																			!form.displayName.trim()
																				? true
																				: undefined
																		}
																		onChange={(event) =>
																			onFieldChange(
																				"displayName",
																				event.target.value,
																			)
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
																		onChange={(event) =>
																			onFieldChange(
																				"iconUrl",
																				event.target.value,
																			)
																		}
																	/>
																	<p className="text-xs text-muted-foreground">
																		{t("external_auth_provider_icon_url_hint")}
																	</p>
																</div>
																<ConnectionFields
																	createStepTouched={createStepTouched}
																	form={form}
																	onFieldChange={onFieldChange}
																	provider={provider}
																	selectedKind={selectedKind}
																	showIssuerUrl={showIssuerUrl}
																	showManualEndpoints={showManualEndpoints}
																	t={t}
																/>
																{connectionTestPanel}
																{createStepTouched &&
																(identityMissing || connectionMissing) ? (
																	<p className="text-xs text-destructive md:col-span-2">
																		{t(
																			"external_auth_provider_wizard_required",
																		)}
																	</p>
																) : null}
															</div>
														</section>
													</div>
													<aside className="min-w-0 space-y-4 lg:sticky lg:top-0 lg:self-start">
														{summaryPanel}
													</aside>
												</div>
											) : (
												<div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_18rem]">
													<div className="min-w-0 space-y-4">
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
																		placeholder={
																			selectedKind?.default_scopes ??
																			DEFAULT_SCOPES
																		}
																		onChange={(event) =>
																			onFieldChange(
																				"scopes",
																				event.target.value,
																			)
																		}
																	/>
																	<p className="text-xs text-muted-foreground">
																		{t("external_auth_provider_scopes_hint")}
																	</p>
																</div>
																<div className="space-y-2 md:col-span-2">
																	<Label htmlFor="external-auth-provider-allowed-domains">
																		{t(
																			"external_auth_provider_allowed_domains",
																		)}
																	</Label>
																	<Input
																		id="external-auth-provider-allowed-domains"
																		value={form.allowedDomains}
																		placeholder="example.com, example.org"
																		onChange={(event) =>
																			onFieldChange(
																				"allowedDomains",
																				event.target.value,
																			)
																		}
																	/>
																	<p className="text-xs text-muted-foreground">
																		{t(
																			"external_auth_provider_allowed_domains_hint",
																		)}
																	</p>
																</div>
																<ClaimFields
																	form={form}
																	onFieldChange={onFieldChange}
																	selectedKind={selectedKind}
																	t={t}
																/>
															</div>
														</section>
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
											<div className="space-y-2">
												<p className="text-sm font-medium">
													{t("external_auth_provider_type")}
												</p>
												<div className="flex h-9 items-center">
													<Badge variant="outline">{providerKindLabel}</Badge>
												</div>
											</div>
											<div className="space-y-2">
												<Label htmlFor="external-auth-provider-display-name">
													{t("external_auth_provider_display_name")}
												</Label>
												<Input
													id="external-auth-provider-display-name"
													value={form.displayName}
													maxLength={128}
													placeholder="Authentik"
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
													onChange={(event) =>
														onFieldChange("iconUrl", event.target.value)
													}
												/>
												<p className="text-xs text-muted-foreground">
													{t("external_auth_provider_icon_url_hint")}
												</p>
											</div>
											<ConnectionFields
												createStepTouched={createStepTouched}
												form={form}
												onFieldChange={onFieldChange}
												provider={provider}
												selectedKind={selectedKind}
												showIssuerUrl={showIssuerUrl}
												showManualEndpoints={showManualEndpoints}
												t={t}
											/>
											{connectionTestPanel}
											<div className="min-w-0 space-y-2 md:col-span-2">
												<Label>
													{t("external_auth_provider_callback_url")}
												</Label>
												<CallbackUrlField
													value={currentCallbackUrl}
													onCopy={onCopyCallbackUrl}
												/>
												<p className="text-xs text-muted-foreground">
													{t("external_auth_provider_callback_url_hint")}
												</p>
											</div>
										</div>
									</section>
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
													placeholder={
														selectedKind?.default_scopes ?? DEFAULT_SCOPES
													}
													onChange={(event) =>
														onFieldChange("scopes", event.target.value)
													}
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
											<ClaimFields
												form={form}
												onFieldChange={onFieldChange}
												selectedKind={selectedKind}
												t={t}
											/>
										</div>
									</section>
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
										<Icon
											name="Spinner"
											className="mr-2 h-4 w-4 animate-spin"
										/>
									) : (
										<Icon name="FloppyDisk" className="mr-2 h-4 w-4" />
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

interface CreatedProviderCallbackDialogProps {
	onCopy: (value: string) => void;
	onOpenChange: (open: boolean) => void;
	provider: AdminExternalAuthProviderInfo | null;
}

function CreatedProviderCallbackDialog({
	onCopy,
	onOpenChange,
	provider,
}: CreatedProviderCallbackDialogProps) {
	const { t } = useTranslation("admin");
	const value = provider
		? callbackUrl(provider.provider_kind, provider.key)
		: "";

	return (
		<Dialog open={Boolean(provider)} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-[calc(100vw-2rem)] overflow-hidden sm:max-w-xl">
				<DialogHeader>
					<DialogTitle>
						{t("external_auth_provider_created_callback_title")}
					</DialogTitle>
					<DialogDescription>
						{t("external_auth_provider_created_callback_desc", {
							name: provider?.display_name ?? "",
						})}
					</DialogDescription>
				</DialogHeader>
				<div className="min-w-0 max-w-full space-y-2 overflow-hidden">
					<Label>{t("external_auth_provider_callback_url")}</Label>
					<CallbackUrlField value={value} onCopy={onCopy} />
					<p className="text-xs text-muted-foreground">
						{t("external_auth_provider_callback_url_hint")}
					</p>
				</div>
				<DialogFooter>
					<Button
						type="button"
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						onClick={() => onOpenChange(false)}
					>
						{t("core:close")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

export default function AdminExternalAuthPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("external_auth"));
	const [searchParams, setSearchParams] = useSearchParams();
	const [offset, setOffsetState] = useState(
		normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
	);
	const [pageSize, setPageSize] = useState<
		(typeof EXTERNAL_AUTH_PAGE_SIZE_OPTIONS)[number]
	>(
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
			DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
		),
	);
	const [providers, setProviders] = useState<AdminExternalAuthProviderInfo[]>(
		[],
	);
	const [providerKinds, setProviderKinds] = useState<
		AdminExternalAuthProviderKindInfo[]
	>([]);
	const [total, setTotal] = useState(0);
	const [loading, setLoading] = useState(true);
	const [dialogOpen, setDialogOpen] = useState(false);
	const [editingProvider, setEditingProvider] =
		useState<AdminExternalAuthProviderInfo | null>(null);
	const [form, setForm] = useState<ExternalAuthProviderFormData>(emptyForm);
	const [createStep, setCreateStep] = useState(0);
	const [createStepTouched, setCreateStepTouched] = useState(false);
	const [submitting, setSubmitting] = useState(false);
	const [testingId, setTestingId] = useState<number | null>(null);
	const [deletingId, setDeletingId] = useState<number | null>(null);
	const [testResult, setTestResult] = useState<string | null>(null);
	const [createdProviderCallback, setCreatedProviderCallback] =
		useState<AdminExternalAuthProviderInfo | null>(null);
	const lastWrittenSearchRef = useRef<string | null>(null);
	const setOffset = (value: number) => {
		setOffsetState(normalizeOffset(value));
	};
	const enabledCount = useMemo(
		() => providers.filter((provider) => provider.enabled).length,
		[providers],
	);
	const providerKindCount = providerKinds.length;
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;
	const pageSizeOptions = EXTERNAL_AUTH_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));
	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, EXTERNAL_AUTH_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
	};
	const createSteps: ExternalAuthCreateStep[] = useMemo(
		() => [
			{
				title: t("external_auth_provider_wizard_step_type_title"),
				description: t("external_auth_provider_wizard_step_type_desc"),
			},
			{
				title: t("external_auth_provider_wizard_step_connection_title"),
				description: t("external_auth_provider_wizard_step_connection_desc"),
			},
			{
				title: t("external_auth_provider_wizard_step_rules_title"),
				description: t("external_auth_provider_wizard_step_rules_desc"),
			},
		],
		[t],
	);
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

	useEffect(() => {
		const managedSearch = getManagedExternalAuthSearchString(searchParams);
		if (managedSearch === lastWrittenSearchRef.current) {
			return;
		}

		const nextOffset = normalizeOffset(
			parseOffsetSearchParam(searchParams.get("offset")),
		);
		const nextPageSize = parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
			DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
		);

		setOffsetState((prev) => (prev === nextOffset ? prev : nextOffset));
		setPageSize((prev) => (prev === nextPageSize ? prev : nextPageSize));
	}, [searchParams]);

	useEffect(() => {
		const nextManagedSearchParams = buildManagedExternalAuthSearchParams({
			offset,
			pageSize,
		});
		const nextSearch = nextManagedSearchParams.toString();
		const currentSearch = getManagedExternalAuthSearchString(searchParams);
		if (
			currentSearch !== lastWrittenSearchRef.current &&
			currentSearch !== nextSearch
		) {
			return;
		}

		lastWrittenSearchRef.current = nextSearch;
		if (nextSearch === currentSearch) {
			return;
		}

		setSearchParams(
			mergeManagedExternalAuthSearchParams(
				searchParams,
				nextManagedSearchParams,
			),
			{ replace: true },
		);
	}, [offset, pageSize, searchParams, setSearchParams]);

	const loadProviders = useCallback(async () => {
		try {
			setLoading(true);
			const [kinds, providerList] = await Promise.all([
				adminExternalAuthService.listKinds(),
				adminExternalAuthService.list({
					limit: pageSize,
					offset,
				}),
			]);
			setProviderKinds(kinds);
			setProviders(providerList.items);
			setTotal(providerList.total);
		} catch (error) {
			handleApiError(error);
		} finally {
			setLoading(false);
		}
	}, [offset, pageSize]);

	useEffect(() => {
		void loadProviders();
	}, [loadProviders]);

	useEffect(() => {
		if (!dialogOpen || editingProvider) {
			previousCreateStepRef.current = 0;
			stepAnimationRef.current = {
				direction: "idle",
				step: 0,
			};
			return;
		}

		previousCreateStepRef.current = createStep;
	}, [createStep, dialogOpen, editingProvider]);

	const setField = <K extends keyof ExternalAuthProviderFormData>(
		key: K,
		value: ExternalAuthProviderFormData[K],
	) => {
		setTestResult(null);
		setForm((prev) => ({ ...prev, [key]: value }));
	};

	const setProviderKind = (kind: ExternalAuthProviderKind) => {
		const descriptor = providerKinds.find((item) => item.kind === kind);
		setTestResult(null);
		setForm((prev) => ({
			...prev,
			providerKind: kind,
			scopes: descriptor?.default_scopes || prev.scopes || DEFAULT_SCOPES,
		}));
	};

	const copyCallbackUrl = async (value: string) => {
		try {
			await writeTextToClipboard(value);
			toast.success(t("core:copied_to_clipboard"));
		} catch {
			toast.error(t("errors:unexpected_error"));
		}
	};

	const openCreate = () => {
		setEditingProvider(null);
		const firstKind = providerKinds[0];
		setForm({
			...emptyForm,
			providerKind: firstKind?.kind ?? "oidc",
			scopes: firstKind?.default_scopes ?? DEFAULT_SCOPES,
		});
		setCreateStep(0);
		setCreateStepTouched(false);
		setTestResult(null);
		setDialogOpen(true);
		if (providerKinds.length === 0) {
			void adminExternalAuthService
				.listKinds()
				.then((kinds) => {
					setProviderKinds(kinds);
					const nextKind = kinds[0];
					if (nextKind) {
						setForm((prev) => ({
							...prev,
							providerKind: nextKind.kind,
							scopes: nextKind.default_scopes || DEFAULT_SCOPES,
						}));
					}
				})
				.catch(handleApiError);
		}
	};

	const openEdit = (provider: AdminExternalAuthProviderInfo) => {
		setEditingProvider(provider);
		setForm(formFromProvider(provider));
		setCreateStep(0);
		setCreateStepTouched(false);
		setTestResult(null);
		setDialogOpen(true);
	};

	const handleDialogOpenChange = (open: boolean) => {
		setDialogOpen(open);
		if (!open) {
			setEditingProvider(null);
			setForm(emptyForm);
			setCreateStep(0);
			setCreateStepTouched(false);
			setSubmitting(false);
		}
	};

	const canAdvanceCreateStep = () => {
		if (createStep === 0) {
			return providerKinds.length > 0;
		}
		if (createStep === 1) {
			const selectedKind =
				providerKinds.find((kind) => kind.kind === form.providerKind) ??
				providerKinds[0] ??
				null;
			return !requiredFieldsMissing(form, selectedKind);
		}
		return true;
	};

	const goCreateNext = () => {
		setCreateStepTouched(true);
		if (!canAdvanceCreateStep()) {
			return;
		}
		setCreateStep((step) => Math.min(step + 1, createSteps.length - 1));
		setCreateStepTouched(false);
	};

	const goCreateBack = () => {
		setCreateStep((step) => Math.max(step - 1, 0));
		setCreateStepTouched(false);
	};

	const goCreateStep = (step: number) => {
		setCreateStep(Math.max(0, Math.min(step, createSteps.length - 1)));
		setCreateStepTouched(false);
	};

	const submitProvider = async () => {
		if (submitting) return;

		setSubmitting(true);
		try {
			if (editingProvider) {
				const updated = await adminExternalAuthService.update(
					editingProvider.id,
					updatePayload(form),
				);
				setProviders((prev) =>
					prev.map((provider) =>
						provider.id === updated.id ? updated : provider,
					),
				);
				toast.success(t("external_auth_provider_updated"));
			} else {
				const created = await adminExternalAuthService.create(
					createPayload(form),
				);
				toast.success(t("external_auth_provider_created"));
				setCreatedProviderCallback(created);
			}
			await loadProviders();
			handleDialogOpenChange(false);
		} catch (error) {
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	const applyTestResult = (
		result: ExternalAuthProviderTestResult,
		options: { touchedProviderId?: number } = {},
	) => {
		setTestResult(formatTestResultSummary(t, result));
		toast.success(t("external_auth_provider_test_success"));
		if (options.touchedProviderId != null) {
			setProviders((prev) =>
				prev.map((item) =>
					item.id === options.touchedProviderId
						? { ...item, updated_at: new Date().toISOString() }
						: item,
				),
			);
		}
	};

	const testFormConnection = async () => {
		const selectedKind =
			providerKinds.find((kind) => kind.kind === form.providerKind) ??
			providerKinds[0] ??
			null;
		if (connectionRequirementsMissing(form, selectedKind)) {
			setCreateStepTouched(true);
			return false;
		}

		try {
			if (editingProvider && !formConnectionChanged(form, editingProvider)) {
				const result = await adminExternalAuthService.test(editingProvider.id);
				applyTestResult(result, { touchedProviderId: editingProvider.id });
				return true;
			}

			const result = await adminExternalAuthService.testParams(
				testParamsPayload(form),
			);
			applyTestResult(result);
			return true;
		} catch (error) {
			handleApiError(error);
			return false;
		}
	};

	const testProvider = async (provider: AdminExternalAuthProviderInfo) => {
		try {
			setTestingId(provider.id);
			const result = await adminExternalAuthService.test(provider.id);
			applyTestResult(result, { touchedProviderId: provider.id });
		} catch (error) {
			handleApiError(error);
		} finally {
			setTestingId(null);
		}
	};

	const deleteProvider = async (id: number) => {
		try {
			setDeletingId(id);
			await adminExternalAuthService.delete(id);
			const isLastItemOnPage = providers.length === 1;
			const nextOffset =
				isLastItemOnPage && offset > 0
					? Math.max(0, offset - pageSize)
					: offset;
			if (nextOffset !== offset) {
				setOffset(nextOffset);
			} else {
				await loadProviders();
			}
			toast.success(t("external_auth_provider_deleted"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setDeletingId(null);
		}
	};

	const {
		confirmId: deleteId,
		requestConfirm,
		dialogProps,
	} = useConfirmDialog<number>(deleteProvider);
	const deleteProviderName =
		deleteId == null
			? ""
			: (providers.find((provider) => provider.id === deleteId)?.display_name ??
				"");

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					title={t("external_auth")}
					description={t("external_auth_intro")}
					actions={
						<>
							<Button
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={openCreate}
							>
								<Icon name="Plus" className="mr-1 h-4 w-4" />
								{t("external_auth_provider_create")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void loadProviders()}
								disabled={loading}
							>
								<Icon
									name={loading ? "Spinner" : "ArrowsClockwise"}
									className={cn("mr-1 h-3.5 w-3.5", loading && "animate-spin")}
								/>
								{t("core:refresh")}
							</Button>
						</>
					}
				/>

				<div className="grid gap-4 md:grid-cols-3">
					<AdminSurface className="flex-none p-4">
						<p className="text-xs text-muted-foreground">
							{t("external_auth_providers_total")}
						</p>
						<p className="mt-1 text-2xl font-semibold">{total}</p>
					</AdminSurface>
					<AdminSurface className="flex-none p-4">
						<p className="text-xs text-muted-foreground">
							{t("external_auth_providers_enabled_page")}
						</p>
						<p className="mt-1 text-2xl font-semibold">{enabledCount}</p>
					</AdminSurface>
					<AdminSurface className="flex-none p-4">
						<p className="text-xs text-muted-foreground">
							{t("external_auth_provider_kinds_supported")}
						</p>
						<p className="mt-1 text-2xl font-semibold">{providerKindCount}</p>
					</AdminSurface>
				</div>

				{testResult ? (
					<div className="rounded-lg border border-emerald-200 bg-emerald-50 px-4 py-3 text-sm text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-200">
						{testResult}
					</div>
				) : null}

				{loading ? (
					<SkeletonTable columns={6} rows={6} />
				) : providers.length === 0 ? (
					<EmptyState
						icon={<Icon name="Globe" className="h-5 w-5" />}
						title={t("external_auth_providers_empty")}
						description={t("external_auth_providers_empty_desc")}
					/>
				) : (
					<AdminTableShell>
						<AdminTable>
							<TableHeader>
								<TableRow>
									<TableHead className="w-16">{t("id")}</TableHead>
									<TableHead className="min-w-[220px]">
										{t("external_auth_provider")}
									</TableHead>
									<TableHead className="min-w-[260px]">
										{t("external_auth_provider_primary_endpoint")}
									</TableHead>
									<TableHead className="w-[180px]">
										{t("external_auth_provider_allowed_domains")}
									</TableHead>
									<TableHead className="w-[220px]">
										{t("core:status")}
									</TableHead>
									<TableHead className="w-32">{t("core:actions")}</TableHead>
								</TableRow>
							</TableHeader>
							<AdminTableBody>
								{providers.map((provider) => {
									const deleting = deletingId === provider.id;
									const testing = testingId === provider.id;
									const providerCallbackUrl = callbackUrl(
										provider.provider_kind,
										provider.key,
									);
									const primaryEndpoint = providerPrimaryEndpoint(provider);
									const allowedDomainSummary = providerAllowedDomainSummary(
										t,
										provider,
									);
									return (
										<TableRow
											key={provider.id}
											className={ADMIN_INTERACTIVE_TABLE_ROW_CLASS}
											onClick={() => {
												if (!deleting) openEdit(provider);
											}}
											onKeyDown={(event) => {
												if (event.key === "Enter" || event.key === " ") {
													event.preventDefault();
													if (!deleting) openEdit(provider);
												}
											}}
											tabIndex={0}
										>
											<TableCell>
												<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
													<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>
														{provider.id}
													</span>
												</div>
											</TableCell>
											<TableCell>
												<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
													<div className="mr-3 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-white shadow-xs ring-1 ring-black/5 dark:bg-slate-950 dark:ring-white/10">
														<ExternalAuthProviderIcon
															kind={provider.provider_kind}
															iconUrl={provider.icon_url}
															className="h-5 w-5"
														/>
													</div>
													<div className={ADMIN_TABLE_STACKED_CELL_CLASS}>
														<div className="flex min-w-0 items-center gap-2">
															<span className="truncate font-medium text-foreground">
																{provider.display_name}
															</span>
														</div>
														<div className="flex min-w-0 flex-wrap items-center gap-2">
															<Badge variant="outline">
																{kindDisplayName(
																	t,
																	provider.provider_kind,
																	providerKinds,
																)}
															</Badge>
														</div>
													</div>
												</div>
											</TableCell>
											<TableCell>
												<div className={ADMIN_TABLE_STACKED_CELL_CLASS}>
													<span
														className="truncate font-mono text-xs text-foreground"
														title={primaryEndpoint?.value ?? "-"}
													>
														{primaryEndpoint?.value ?? "-"}
													</span>
													<span
														className={ADMIN_TABLE_MUTED_TEXT_CLASS}
														title={formatDateAbsoluteWithOffset(
															provider.updated_at,
														)}
													>
														{primaryEndpoint
															? t(primaryEndpoint.labelKey)
															: t("external_auth_provider_primary_endpoint")}
														{" · "}
														{t("core:updated_at")}:{" "}
														{formatDateAbsolute(provider.updated_at)}
													</span>
												</div>
											</TableCell>
											<TableCell>
												<div className={ADMIN_TABLE_STACKED_CELL_CLASS}>
													<span
														className="truncate text-xs text-muted-foreground"
														title={allowedDomainSummary}
													>
														{allowedDomainSummary}
													</span>
													<span
														className="truncate font-mono text-xs text-muted-foreground"
														title={provider.scopes}
													>
														{provider.scopes}
													</span>
												</div>
											</TableCell>
											<TableCell>
												<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
													<Badge
														variant="outline"
														className={providerStatusTone(provider)}
													>
														{provider.enabled
															? t("external_auth_provider_enabled_badge")
															: t("external_auth_provider_disabled_badge")}
													</Badge>
													<Badge variant="outline">
														{securityModeLabel(t, provider)}
													</Badge>
													{provider.require_email_verified ? (
														<Badge variant="outline">
															{t(
																"external_auth_provider_require_email_verified",
															)}
														</Badge>
													) : null}
												</div>
											</TableCell>
											<TableCell
												onClick={(event) => event.stopPropagation()}
												onKeyDown={(event) => event.stopPropagation()}
											>
												<div className="flex justify-end gap-1">
													<Button
														variant="ghost"
														size="icon"
														className={ADMIN_ICON_BUTTON_CLASS}
														onClick={() =>
															void copyCallbackUrl(providerCallbackUrl)
														}
														disabled={!providerCallbackUrl || deleting}
														aria-label={t(
															"external_auth_provider_copy_callback_url",
														)}
														title={t(
															"external_auth_provider_copy_callback_url",
														)}
													>
														<Icon name="Copy" className="h-3.5 w-3.5" />
													</Button>
													<Button
														variant="ghost"
														size="icon"
														className={ADMIN_ICON_BUTTON_CLASS}
														onClick={() => void testProvider(provider)}
														disabled={testing || deleting}
														aria-label={t("external_auth_provider_test")}
														title={t("external_auth_provider_test")}
													>
														<Icon
															name={testing ? "Spinner" : "WifiHigh"}
															className={cn(
																"h-3.5 w-3.5",
																testing && "animate-spin",
															)}
														/>
													</Button>
													<Button
														variant="ghost"
														size="icon"
														className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
														onClick={() => requestConfirm(provider.id)}
														disabled={deleting || testing}
														aria-label={t("external_auth_provider_delete")}
														title={t("external_auth_provider_delete")}
													>
														<Icon
															name={deleting ? "Spinner" : "Trash"}
															className={cn(
																"h-3.5 w-3.5",
																deleting && "animate-spin",
															)}
														/>
													</Button>
												</div>
											</TableCell>
										</TableRow>
									);
								})}
							</AdminTableBody>
						</AdminTable>
					</AdminTableShell>
				)}

				<AdminOffsetPagination
					total={total}
					currentPage={currentPage}
					totalPages={totalPages}
					pageSize={String(pageSize)}
					pageSizeOptions={pageSizeOptions}
					onPageSizeChange={handlePageSizeChange}
					prevDisabled={prevPageDisabled}
					nextDisabled={nextPageDisabled}
					onPrevious={() => setOffset(Math.max(0, offset - pageSize))}
					onNext={() => setOffset(offset + pageSize)}
				/>

				<ProviderDialog
					createStep={createStep}
					createStepDirection={createStepDirection}
					createStepTouched={createStepTouched}
					createSteps={createSteps}
					form={form}
					mode={editingProvider ? "edit" : "create"}
					onCreateBack={goCreateBack}
					onCreateNext={goCreateNext}
					onCreateStepChange={goCreateStep}
					open={dialogOpen}
					provider={editingProvider}
					providerKinds={providerKinds}
					submitting={submitting}
					onCopyCallbackUrl={(value) => void copyCallbackUrl(value)}
					onFieldChange={setField}
					onOpenChange={handleDialogOpenChange}
					onProviderKindChange={setProviderKind}
					onSubmit={() => void submitProvider()}
					onTestConnection={testFormConnection}
					testResult={testResult}
				/>

				<CreatedProviderCallbackDialog
					provider={createdProviderCallback}
					onCopy={(value) => void copyCallbackUrl(value)}
					onOpenChange={(open) => {
						if (!open) {
							setCreatedProviderCallback(null);
						}
					}}
				/>

				<ConfirmDialog
					{...dialogProps}
					title={t("external_auth_provider_delete_title", {
						name: deleteProviderName,
					})}
					description={t("external_auth_provider_delete_desc")}
					confirmLabel={t("core:delete")}
					variant="destructive"
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}
