import type { TFunction } from "i18next";
import type { MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import {
	externalAuthKindIconPath,
	normalizeExternalAuthIconUrl,
} from "@/lib/externalAuthProviders";
import {
	buildOffsetPaginationSearchParams,
	parseOffsetSearchParam,
	parsePageSizeSearchParam,
} from "@/lib/pagination";
import { absoluteAppUrl } from "@/lib/publicSiteUrl";
import { cn } from "@/lib/utils";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
	CreateExternalAuthProviderInput,
	ExternalAuthProviderKind,
	ExternalAuthProviderTestParamsInput,
	ExternalAuthProviderTestResult,
	UpdateExternalAuthProviderInput,
} from "@/types/api";

export const DEFAULT_SCOPES = "openid email profile";
const PROVIDER_KIND_ORDER: Partial<Record<ExternalAuthProviderKind, number>> = {
	oidc: 0,
	generic_oauth2: 1,
	github: 2,
	google: 3,
	microsoft: 4,
	qq: 5,
};
const REDACTED_SECRET = "***REDACTED***";
export const STANDARD_CLAIMS = {
	avatarUrlClaim: "picture",
	displayNameClaim: "name",
	emailClaim: "email",
	emailVerifiedClaim: "email_verified",
	groupsClaim: "groups",
	subjectClaim: "sub",
	usernameClaim: "preferred_username",
} as const;
export const GITHUB_FIXED_ENDPOINTS = {
	authorizationUrl: "https://github.com/login/oauth/authorize",
	tokenUrl: "https://github.com/login/oauth/access_token",
	userinfoUrl: "https://api.github.com/user",
	userEmailsUrl: "https://api.github.com/user/emails",
} as const;
export const GITHUB_CLAIMS = {
	displayNameClaim: "name",
	emailClaim: "primary && verified from /user/emails",
	subjectClaim: "id",
	usernameClaim: "login",
} as const;
export const QQ_FIXED_ENDPOINTS = {
	authorizationUrl: "https://graph.qq.com/oauth2.0/authorize",
	openidUrl: "https://graph.qq.com/oauth2.0/me",
	tokenUrl: "https://graph.qq.com/oauth2.0/token",
	userinfoUrl: "https://graph.qq.com/user/get_user_info",
} as const;
export const QQ_CLAIMS = {
	displayNameClaim: "nickname",
	emailClaim: "not returned",
	subjectClaim: "openid",
} as const;
export const GOOGLE_ISSUER_URL = "https://accounts.google.com";
export const GOOGLE_DISCOVERY_URL =
	"https://accounts.google.com/.well-known/openid-configuration";
export const GOOGLE_CLAIMS = {
	avatarUrlClaim: "picture",
	displayNameClaim: "name",
	emailClaim: "email",
	emailVerifiedClaim: "email_verified",
	subjectClaim: "sub",
} as const;
export const MICROSOFT_DEFAULT_TENANT = "common";
export const MICROSOFT_ISSUER_BASE = "https://login.microsoftonline.com";
export const MICROSOFT_CUSTOM_TENANT_MODE = "custom";
export const MICROSOFT_TENANT_PRESETS = [
	"consumers",
	"organizations",
	MICROSOFT_DEFAULT_TENANT,
] as const;
export type MicrosoftTenantMode =
	| (typeof MICROSOFT_TENANT_PRESETS)[number]
	| typeof MICROSOFT_CUSTOM_TENANT_MODE;
export const MICROSOFT_CLAIMS = {
	displayNameClaim: "name",
	emailClaim: "email",
	subjectClaim: "sub",
} as const;
export const EXTERNAL_AUTH_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
export const DEFAULT_EXTERNAL_AUTH_PAGE_SIZE = 20 as const;
const EXTERNAL_AUTH_MANAGED_QUERY_KEYS = ["offset", "pageSize"] as const;

export interface ExternalAuthProviderFormData {
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
	microsoftTenantMode: MicrosoftTenantMode;
	microsoftTenant: string;
	providerKind: ExternalAuthProviderKind;
	requireEmailVerified: boolean;
	scopes: string;
	subjectClaim: string;
	tokenUrl: string;
	userinfoUrl: string;
	usernameClaim: string;
}

export type ExternalAuthProviderFieldChange = <
	K extends keyof ExternalAuthProviderFormData,
>(
	key: K,
	value: ExternalAuthProviderFormData[K],
) => void;

export interface ExternalAuthCreateStep {
	title: string;
	description: string;
}

export const emptyForm: ExternalAuthProviderFormData = {
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
	microsoftTenantMode: MICROSOFT_DEFAULT_TENANT,
	microsoftTenant: MICROSOFT_DEFAULT_TENANT,
	providerKind: "oidc",
	requireEmailVerified: true,
	scopes: DEFAULT_SCOPES,
	subjectClaim: "",
	tokenUrl: "",
	userinfoUrl: "",
	usernameClaim: "",
};

export function formFromProviderKind(
	kind: AdminExternalAuthProviderKindInfo,
): ExternalAuthProviderFormData {
	const microsoftTenant =
		kind.create_defaults.options.microsoft?.tenant.trim() ||
		MICROSOFT_DEFAULT_TENANT;
	return {
		...emptyForm,
		autoLinkVerifiedEmailEnabled:
			kind.create_defaults.auto_link_verified_email_enabled,
		autoProvisionEnabled: kind.create_defaults.auto_provision_enabled,
		displayName: kind.create_defaults.display_name,
		enabled: kind.create_defaults.enabled,
		microsoftTenantMode: microsoftTenantModeForValue(microsoftTenant),
		microsoftTenant,
		providerKind: kind.kind,
		requireEmailVerified: kind.create_defaults.require_email_verified,
		scopes: kind.create_defaults.scopes,
	};
}

export function formFromProvider(
	provider: AdminExternalAuthProviderInfo,
): ExternalAuthProviderFormData {
	const microsoftTenant = isMicrosoftProviderKind(provider.provider_kind)
		? normalizeMicrosoftTenantValue(
				provider.options?.microsoft?.tenant ||
					microsoftTenantFromIssuerUrl(provider.issuer_url) ||
					MICROSOFT_DEFAULT_TENANT,
			) || MICROSOFT_DEFAULT_TENANT
		: MICROSOFT_DEFAULT_TENANT;
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
		microsoftTenantMode: microsoftTenantModeForValue(microsoftTenant),
		microsoftTenant,
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
		case "generic_oauth2":
			return "Generic OAuth2";
		case "github":
			return "GitHub";
		case "google":
			return "Google";
		case "microsoft":
			return "Microsoft";
		case "qq":
			return "QQ";
		case "oidc":
			return "OpenID Connect";
	}
}

export function isGitHubProviderKind(
	kind:
		| AdminExternalAuthProviderKindInfo
		| ExternalAuthProviderKind
		| null
		| undefined,
) {
	return (typeof kind === "string" ? kind : kind?.kind) === "github";
}

export function isGoogleProviderKind(
	kind:
		| AdminExternalAuthProviderKindInfo
		| ExternalAuthProviderKind
		| null
		| undefined,
) {
	return (typeof kind === "string" ? kind : kind?.kind) === "google";
}

export function isMicrosoftProviderKind(
	kind:
		| AdminExternalAuthProviderKindInfo
		| ExternalAuthProviderKind
		| null
		| undefined,
) {
	return (typeof kind === "string" ? kind : kind?.kind) === "microsoft";
}

export function isQqProviderKind(
	kind:
		| AdminExternalAuthProviderKindInfo
		| ExternalAuthProviderKind
		| null
		| undefined,
) {
	return (typeof kind === "string" ? kind : kind?.kind) === "qq";
}

function isSpecializedProviderKind(
	kind:
		| AdminExternalAuthProviderKindInfo
		| ExternalAuthProviderKind
		| null
		| undefined,
) {
	return (
		isGitHubProviderKind(kind) ||
		isGoogleProviderKind(kind) ||
		isMicrosoftProviderKind(kind) ||
		isQqProviderKind(kind)
	);
}

function providerKindValue(
	kind:
		| AdminExternalAuthProviderKindInfo
		| ExternalAuthProviderKind
		| null
		| undefined,
) {
	return typeof kind === "string" ? kind : kind?.kind;
}

function providerUsesFixedConnection(
	kind:
		| AdminExternalAuthProviderKindInfo
		| ExternalAuthProviderKind
		| null
		| undefined,
	descriptor?: AdminExternalAuthProviderKindInfo | null,
) {
	if (descriptor && descriptor.kind === providerKindValue(kind)) {
		return (
			!descriptor.issuer_url_supported &&
			!descriptor.manual_endpoint_configuration_supported
		);
	}
	return isSpecializedProviderKind(kind);
}

function localizedProviderKindText(
	t: TFunction,
	key: string,
	fallback: string,
) {
	const translated = t(key);
	return translated === key ? fallback : translated;
}

export function ExternalAuthProviderIcon({
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
					fallbackExternalAuthIcon(
						event.currentTarget,
						configuredIcon,
						kindIcon,
					);
				}}
			/>
		);
	}

	return <Icon name="SignIn" className={cn("text-primary", className)} />;
}

export function kindDisplayName(
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

export function kindDescription(
	t: TFunction,
	kind: AdminExternalAuthProviderKindInfo,
) {
	return localizedProviderKindText(
		t,
		`external_auth_provider_kind_${kind.kind}_description`,
		kind.description,
	);
}

export function sortExternalAuthProviderKinds(
	providerKinds: AdminExternalAuthProviderKindInfo[],
) {
	return [...providerKinds].sort((left, right) => {
		const leftRank = PROVIDER_KIND_ORDER[left.kind] ?? Number.MAX_SAFE_INTEGER;
		const rightRank =
			PROVIDER_KIND_ORDER[right.kind] ?? Number.MAX_SAFE_INTEGER;
		return (
			leftRank - rightRank ||
			left.display_name.localeCompare(right.display_name) ||
			left.kind.localeCompare(right.kind)
		);
	});
}

export function parseAllowedDomains(value: string) {
	const domains: string[] = [];
	const seen = new Set<string>();
	for (const item of value.split(/[,\n]/)) {
		const domain = item.trim().replace(/^@+/, "").toLowerCase();
		if (!domain || seen.has(domain)) {
			continue;
		}
		seen.add(domain);
		domains.push(domain);
	}
	return domains;
}

function nullableText(value: string) {
	const trimmed = value.trim();
	return trimmed ? trimmed : null;
}

function nullableSecretText(value: string) {
	const trimmed = value.trim();
	return trimmed && trimmed !== REDACTED_SECRET ? trimmed : null;
}

export function microsoftIssuerUrlForTenant(value: string) {
	const trimmed = value.trim();
	if (/^https?:\/\//i.test(trimmed)) {
		const tenant = trimmed;
		return tenant.replace(/\/+$/, "");
	}
	const tenant =
		normalizeMicrosoftTenantValue(trimmed) || MICROSOFT_DEFAULT_TENANT;
	return `${MICROSOFT_ISSUER_BASE}/${tenant}/v2.0`;
}

export function microsoftTenantFromIssuerUrl(value: string | null | undefined) {
	const trimmed = value?.trim();
	if (!trimmed) return "";
	try {
		const parsed = new URL(trimmed);
		if (parsed.hostname !== "login.microsoftonline.com") {
			return "";
		}
		if (parsed.search || parsed.hash) {
			return "";
		}
		const segments = parsed.pathname.split("/").filter(Boolean);
		return segments.length === 2 && segments[1]?.toLowerCase() === "v2.0"
			? normalizeMicrosoftTenantValue(segments[0])
			: "";
	} catch {
		return "";
	}
}

export function microsoftTenantModeForValue(
	value: string,
): MicrosoftTenantMode {
	const trimmed = normalizeMicrosoftTenantValue(value);
	return MICROSOFT_TENANT_PRESETS.includes(
		trimmed as (typeof MICROSOFT_TENANT_PRESETS)[number],
	)
		? (trimmed as (typeof MICROSOFT_TENANT_PRESETS)[number])
		: MICROSOFT_CUSTOM_TENANT_MODE;
}

function normalizeMicrosoftTenantValue(value: string) {
	return value.trim().toLowerCase();
}

function formMicrosoftTenantValue(form: ExternalAuthProviderFormData) {
	return form.microsoftTenantMode === MICROSOFT_CUSTOM_TENANT_MODE
		? normalizeMicrosoftTenantValue(form.microsoftTenant)
		: normalizeMicrosoftTenantValue(form.microsoftTenantMode);
}

function providerMicrosoftTenantValue(provider: AdminExternalAuthProviderInfo) {
	return normalizeMicrosoftTenantValue(
		provider.options?.microsoft?.tenant ||
			microsoftTenantFromIssuerUrl(provider.issuer_url) ||
			MICROSOFT_DEFAULT_TENANT,
	);
}

function formIssuerUrlForPayload(
	form: ExternalAuthProviderFormData,
	selectedKind?: AdminExternalAuthProviderKindInfo | null,
) {
	if (
		selectedKind?.kind === form.providerKind &&
		!selectedKind.issuer_url_supported
	) {
		return null;
	}
	if (providerUsesFixedConnection(form.providerKind, selectedKind)) {
		return null;
	}
	return nullableText(form.issuerUrl);
}

function formManualEndpointForPayload(
	form: ExternalAuthProviderFormData,
	value: string,
	selectedKind?: AdminExternalAuthProviderKindInfo | null,
) {
	if (
		selectedKind?.kind === form.providerKind &&
		!selectedKind.manual_endpoint_configuration_supported
	) {
		return null;
	}
	if (providerUsesFixedConnection(form.providerKind, selectedKind)) {
		return null;
	}
	return nullableText(value);
}

function formOptionsForPayload(
	form: ExternalAuthProviderFormData,
	selectedKind?: AdminExternalAuthProviderKindInfo | null,
) {
	if (!selectedKind?.create_defaults.options.microsoft) {
		return {};
	}
	return {
		microsoft: {
			tenant: formMicrosoftTenantValue(form) || MICROSOFT_DEFAULT_TENANT,
		},
	};
}

function isRedactedSecret(value: string) {
	return value.trim() === REDACTED_SECRET;
}

function normalizedUrl(value: string) {
	try {
		return new URL(value, document.baseURI).href;
	} catch {
		return value;
	}
}

function fallbackExternalAuthIcon(
	target: HTMLImageElement,
	configuredIcon: string,
	kindIcon: string,
) {
	if (
		configuredIcon &&
		kindIcon &&
		target.dataset.fallbackTried !== "1" &&
		normalizedUrl(target.src) !== normalizedUrl(kindIcon)
	) {
		target.dataset.fallbackTried = "1";
		target.src = kindIcon;
		return;
	}
	target.hidden = true;
}

function effectiveClaim(value: string | null | undefined, fallback: string) {
	return value?.trim() || fallback;
}

export function defaultScopesForKind(
	kind: AdminExternalAuthProviderKindInfo | null | undefined,
) {
	return kind?.default_scopes?.trim() || DEFAULT_SCOPES;
}

export function createPayload(
	form: ExternalAuthProviderFormData,
	selectedKind?: AdminExternalAuthProviderKindInfo | null,
): CreateExternalAuthProviderInput {
	const allowedDomains = parseAllowedDomains(form.allowedDomains);
	return {
		allowed_domains: allowedDomains.length > 0 ? allowedDomains : null,
		authorization_url: formManualEndpointForPayload(
			form,
			form.authorizationUrl,
			selectedKind,
		),
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
		issuer_url: formIssuerUrlForPayload(form, selectedKind),
		options: formOptionsForPayload(form, selectedKind),
		provider_kind: form.providerKind,
		require_email_verified: form.requireEmailVerified,
		scopes: form.scopes.trim() || defaultScopesForKind(selectedKind),
		subject_claim: nullableText(form.subjectClaim),
		token_url: formManualEndpointForPayload(form, form.tokenUrl, selectedKind),
		userinfo_url: formManualEndpointForPayload(
			form,
			form.userinfoUrl,
			selectedKind,
		),
		username_claim: nullableText(form.usernameClaim),
	};
}

export function updatePayload(
	form: ExternalAuthProviderFormData,
	selectedKind?: AdminExternalAuthProviderKindInfo | null,
): UpdateExternalAuthProviderInput {
	const allowedDomains = parseAllowedDomains(form.allowedDomains);
	return {
		allowed_domains: allowedDomains.length > 0 ? allowedDomains : null,
		authorization_url: formManualEndpointForPayload(
			form,
			form.authorizationUrl,
			selectedKind,
		),
		auto_link_verified_email_enabled: form.autoLinkVerifiedEmailEnabled,
		auto_provision_enabled: form.autoProvisionEnabled,
		avatar_url_claim: nullableText(form.avatarUrlClaim),
		client_id: form.clientId.trim(),
		...(isRedactedSecret(form.clientSecret)
			? {}
			: { client_secret: nullableText(form.clientSecret) }),
		display_name: form.displayName.trim(),
		display_name_claim: nullableText(form.displayNameClaim),
		email_claim: nullableText(form.emailClaim),
		email_verified_claim: nullableText(form.emailVerifiedClaim),
		enabled: form.enabled,
		groups_claim: nullableText(form.groupsClaim),
		icon_url: nullableText(form.iconUrl),
		issuer_url: formIssuerUrlForPayload(form, selectedKind),
		options: formOptionsForPayload(form, selectedKind),
		require_email_verified: form.requireEmailVerified,
		scopes: form.scopes.trim() || defaultScopesForKind(selectedKind),
		subject_claim: nullableText(form.subjectClaim),
		token_url: formManualEndpointForPayload(form, form.tokenUrl, selectedKind),
		userinfo_url: formManualEndpointForPayload(
			form,
			form.userinfoUrl,
			selectedKind,
		),
		username_claim: nullableText(form.usernameClaim),
	};
}

export function testParamsPayload(
	form: ExternalAuthProviderFormData,
	selectedKind?: AdminExternalAuthProviderKindInfo | null,
): ExternalAuthProviderTestParamsInput {
	return {
		authorization_url: formManualEndpointForPayload(
			form,
			form.authorizationUrl,
			selectedKind,
		),
		client_id: form.clientId.trim(),
		client_secret: nullableSecretText(form.clientSecret),
		issuer_url: formIssuerUrlForPayload(form, selectedKind),
		options: formOptionsForPayload(form, selectedKind),
		provider_kind: form.providerKind,
		scopes: form.scopes.trim() || defaultScopesForKind(selectedKind),
		token_url: formManualEndpointForPayload(form, form.tokenUrl, selectedKind),
		userinfo_url: formManualEndpointForPayload(
			form,
			form.userinfoUrl,
			selectedKind,
		),
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

export function formConnectionChanged(
	form: ExternalAuthProviderFormData,
	provider: AdminExternalAuthProviderInfo,
	selectedKind?: AdminExternalAuthProviderKindInfo | null,
) {
	const defaultScopes = defaultScopesForKind(selectedKind);
	const formIssuerUrl = formIssuerUrlForPayload(form, selectedKind) ?? "";
	const formAuthorizationUrl =
		formManualEndpointForPayload(form, form.authorizationUrl, selectedKind) ??
		"";
	const formTokenUrl =
		formManualEndpointForPayload(form, form.tokenUrl, selectedKind) ?? "";
	const formUserinfoUrl =
		formManualEndpointForPayload(form, form.userinfoUrl, selectedKind) ?? "";
	const providerHasFixedConnection = providerUsesFixedConnection(
		provider.provider_kind,
		selectedKind,
	);
	const providerIssuerUrl = providerHasFixedConnection
		? ""
		: (provider.issuer_url ?? "");
	const providerAuthorizationUrl = providerHasFixedConnection
		? null
		: provider.authorization_url;
	const providerTokenUrl = providerHasFixedConnection
		? null
		: provider.token_url;
	const providerUserinfoUrl = providerHasFixedConnection
		? null
		: provider.userinfo_url;
	return (
		form.providerKind !== provider.provider_kind ||
		normalizeConnectionValue(formIssuerUrl) !==
			normalizeConnectionValue(providerIssuerUrl) ||
		(Boolean(selectedKind?.create_defaults.options.microsoft) &&
			formMicrosoftTenantValue(form) !==
				providerMicrosoftTenantValue(provider)) ||
		normalizeConnectionValue(formAuthorizationUrl) !==
			normalizeConnectionValue(providerAuthorizationUrl) ||
		normalizeConnectionValue(formTokenUrl) !==
			normalizeConnectionValue(providerTokenUrl) ||
		normalizeConnectionValue(formUserinfoUrl) !==
			normalizeConnectionValue(providerUserinfoUrl) ||
		normalizeConnectionValue(form.clientId) !==
			normalizeConnectionValue(provider.client_id) ||
		(form.scopes.trim() || defaultScopes) !==
			(provider.scopes.trim() || defaultScopes) ||
		formClientSecretChanged(form, provider)
	);
}

export function formatTestResultSummary(
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

export function providerStatusTone(provider: AdminExternalAuthProviderInfo) {
	return provider.enabled
		? "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/60 dark:text-emerald-300"
		: "border-slate-200 bg-slate-50 text-slate-700 dark:border-slate-800 dark:bg-slate-950/50 dark:text-slate-300";
}

export function securityModeLabel(
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

export function callbackUrl(
	providerKind: ExternalAuthProviderKind,
	providerKey: string,
) {
	const path = callbackPath(providerKind, providerKey);
	return path ? absoluteAppUrl(path) : "";
}

export function providerPrimaryEndpoint(
	provider: AdminExternalAuthProviderInfo,
) {
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

export function providerAllowedDomainSummary(
	t: TFunction,
	provider: AdminExternalAuthProviderInfo,
) {
	return provider.allowed_domains.length > 0
		? provider.allowed_domains.join(", ")
		: t("external_auth_provider_allowed_domains_all");
}

export function normalizeOffset(offset: number) {
	return Math.max(0, Math.floor(offset));
}

export function buildManagedExternalAuthSearchParams({
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

export function getManagedExternalAuthSearchString(
	searchParams: URLSearchParams,
) {
	return buildManagedExternalAuthSearchParams({
		offset: normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
		pageSize: parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
			DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
		),
	}).toString();
}

export function mergeManagedExternalAuthSearchParams(
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

export function shouldShowIssuerUrl(
	kind: AdminExternalAuthProviderKindInfo | null,
) {
	return Boolean(kind?.issuer_url_supported);
}

export function shouldShowManualEndpoints(
	kind: AdminExternalAuthProviderKindInfo | null,
) {
	return Boolean(kind?.manual_endpoint_configuration_supported);
}

export function connectionRequirementsMissing(
	form: ExternalAuthProviderFormData,
	kind: AdminExternalAuthProviderKindInfo | null,
) {
	if (!form.clientId.trim()) {
		return true;
	}
	if ((kind?.issuer_url_required ?? true) && !form.issuerUrl.trim()) {
		return true;
	}
	if (
		kind?.create_defaults.options.microsoft &&
		form.microsoftTenantMode === MICROSOFT_CUSTOM_TENANT_MODE &&
		!form.microsoftTenant.trim()
	) {
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

export function requiredFieldsMissing(
	form: ExternalAuthProviderFormData,
	kind: AdminExternalAuthProviderKindInfo | null,
) {
	return !form.displayName.trim() || connectionRequirementsMissing(form, kind);
}

export function formConnectionSummary(
	form: ExternalAuthProviderFormData,
	selectedKind: AdminExternalAuthProviderKindInfo | null,
) {
	if (isGitHubProviderKind(selectedKind ?? form.providerKind)) {
		return `authorization: ${GITHUB_FIXED_ENDPOINTS.authorizationUrl} · token: ${GITHUB_FIXED_ENDPOINTS.tokenUrl} · userinfo: ${GITHUB_FIXED_ENDPOINTS.userinfoUrl} · emails: ${GITHUB_FIXED_ENDPOINTS.userEmailsUrl}`;
	}
	if (isGoogleProviderKind(selectedKind ?? form.providerKind)) {
		return `issuer: ${GOOGLE_ISSUER_URL} · discovery: ${GOOGLE_DISCOVERY_URL}`;
	}
	if (isMicrosoftProviderKind(selectedKind ?? form.providerKind)) {
		const tenant = formMicrosoftTenantValue(form) || MICROSOFT_DEFAULT_TENANT;
		return `tenant: ${tenant} · OIDC discovery`;
	}
	if (isQqProviderKind(selectedKind ?? form.providerKind)) {
		return `authorization: ${QQ_FIXED_ENDPOINTS.authorizationUrl} · token: ${QQ_FIXED_ENDPOINTS.tokenUrl} · openid: ${QQ_FIXED_ENDPOINTS.openidUrl} · userinfo: ${QQ_FIXED_ENDPOINTS.userinfoUrl}`;
	}
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

export function formClaimSummary(
	form: ExternalAuthProviderFormData,
	selectedKind: AdminExternalAuthProviderKindInfo | null,
) {
	if (isGitHubProviderKind(selectedKind ?? form.providerKind)) {
		return `subject=${GITHUB_CLAIMS.subjectClaim} · username=${GITHUB_CLAIMS.usernameClaim} · display=${GITHUB_CLAIMS.displayNameClaim} · email=${GITHUB_CLAIMS.emailClaim}`;
	}
	if (isGoogleProviderKind(selectedKind ?? form.providerKind)) {
		return `subject=${GOOGLE_CLAIMS.subjectClaim} · display=${GOOGLE_CLAIMS.displayNameClaim} · email=${GOOGLE_CLAIMS.emailClaim} · email_verified=${GOOGLE_CLAIMS.emailVerifiedClaim} · avatar=${GOOGLE_CLAIMS.avatarUrlClaim}`;
	}
	if (isMicrosoftProviderKind(selectedKind ?? form.providerKind)) {
		return `subject=${MICROSOFT_CLAIMS.subjectClaim} · display=${MICROSOFT_CLAIMS.displayNameClaim} · email=${MICROSOFT_CLAIMS.emailClaim}`;
	}
	if (isQqProviderKind(selectedKind ?? form.providerKind)) {
		return `subject=${QQ_CLAIMS.subjectClaim} · display=${QQ_CLAIMS.displayNameClaim} · email=${QQ_CLAIMS.emailClaim}`;
	}
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

export function providerIconSummary(form: ExternalAuthProviderFormData) {
	return form.iconUrl.trim() || "-";
}

interface CallbackUrlFieldProps {
	className?: string;
	onCopy: (value: string) => void;
	value: string;
}

export function CallbackUrlField({
	className,
	onCopy,
	value,
}: CallbackUrlFieldProps) {
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
				className="size-7 shrink-0"
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
				<Icon name="Copy" className="size-3.5" />
			</Button>
		</div>
	);
}
