import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
	buildManagedExternalAuthSearchParams,
	CallbackUrlField,
	callbackUrl,
	connectionRequirementsMissing,
	createPayload,
	defaultScopesForKind,
	emptyForm,
	formatTestResultSummary,
	formClaimSummary,
	formConnectionChanged,
	formConnectionSummary,
	formFromProvider,
	getManagedExternalAuthSearchString,
	kindDescription,
	kindDisplayName,
	mergeManagedExternalAuthSearchParams,
	microsoftIssuerUrlForTenant,
	microsoftTenantFromIssuerUrl,
	normalizeOffset,
	parseAllowedDomains,
	providerAllowedDomainSummary,
	providerIconSummary,
	providerPrimaryEndpoint,
	requiredFieldsMissing,
	securityModeLabel,
	shouldShowIssuerUrl,
	testParamsPayload,
	updatePayload,
} from "@/components/admin/admin-external-auth-page/shared";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
	ExternalAuthProviderTestResult,
} from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		"aria-label": ariaLabel,
		children,
		disabled,
		onClick,
		title,
		type,
	}: {
		"aria-label"?: string;
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: React.MouseEventHandler<HTMLButtonElement>;
		title?: string;
		type?: "button" | "submit";
	}) => (
		<button
			type={type ?? "button"}
			aria-label={ariaLabel}
			disabled={disabled}
			onClick={onClick}
			title={title}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/lib/publicSiteUrl", () => ({
	absoluteAppUrl: (path: string) => `https://app.example.com${path}`,
}));

function t(key: string, options?: Record<string, unknown>) {
	if (key === "external_auth_provider_test_check_ok") {
		return `ok:${options?.name}:${options?.message}`;
	}
	if (key === "external_auth_provider_test_check_error") {
		return `error:${options?.name}:${options?.message}`;
	}
	if (key === "external_auth_provider_test_success_detail") {
		return `success:${options?.provider}`;
	}
	return key;
}

function provider(
	overrides: Partial<AdminExternalAuthProviderInfo> = {},
): AdminExternalAuthProviderInfo {
	return {
		allowed_domains: ["example.com", "example.org"],
		authorization_url: null,
		auto_link_verified_email_enabled: false,
		auto_provision_enabled: false,
		avatar_url_claim: null,
		client_id: "client-123",
		client_secret: null,
		client_secret_configured: true,
		created_at: "2026-05-17T10:00:00Z",
		display_name: "Example IDP",
		display_name_claim: null,
		email_claim: null,
		email_verified_claim: null,
		enabled: true,
		groups_claim: null,
		icon_url: " /static/idp.svg ",
		id: 1,
		issuer_url: "https://idp.example.com",
		key: "example",
		protocol: "oidc",
		provider_kind: "oidc",
		require_email_verified: true,
		scopes: "openid email profile",
		subject_claim: null,
		token_url: null,
		updated_at: "2026-05-17T10:00:00Z",
		userinfo_url: null,
		username_claim: null,
		...overrides,
	};
}

function kind(
	overrides: Partial<AdminExternalAuthProviderKindInfo> = {},
): AdminExternalAuthProviderKindInfo {
	return {
		authorization_url_required: false,
		default_scopes: "openid email profile",
		description: "OIDC sign-in.",
		display_name: "OpenID Connect",
		issuer_url_required: true,
		kind: "oidc",
		manual_endpoint_configuration_supported: false,
		protocol: "oidc",
		supports_discovery: true,
		supports_email_verified_claim: true,
		supports_pkce: true,
		token_url_required: false,
		userinfo_url_required: false,
		...overrides,
	};
}

function oauth2Kind(
	overrides: Partial<AdminExternalAuthProviderKindInfo> = {},
): AdminExternalAuthProviderKindInfo {
	return kind({
		authorization_url_required: true,
		default_scopes: "openid email profile",
		description: "OAuth2 sign-in.",
		display_name: "Generic OAuth2",
		issuer_url_required: false,
		kind: "generic_oauth2",
		manual_endpoint_configuration_supported: true,
		protocol: "oauth2",
		supports_discovery: false,
		token_url_required: true,
		userinfo_url_required: true,
		...overrides,
	});
}

function githubKind(
	overrides: Partial<AdminExternalAuthProviderKindInfo> = {},
): AdminExternalAuthProviderKindInfo {
	return kind({
		authorization_url_required: false,
		default_scopes: "read:user user:email",
		description: "GitHub sign-in.",
		display_name: "GitHub",
		issuer_url_required: false,
		kind: "github",
		manual_endpoint_configuration_supported: false,
		protocol: "oauth2",
		supports_discovery: false,
		supports_email_verified_claim: false,
		token_url_required: false,
		userinfo_url_required: false,
		...overrides,
	});
}

function googleKind(
	overrides: Partial<AdminExternalAuthProviderKindInfo> = {},
): AdminExternalAuthProviderKindInfo {
	return kind({
		authorization_url_required: false,
		default_scopes: "openid profile email",
		description: "Google sign-in.",
		display_name: "Google",
		issuer_url_required: false,
		kind: "google",
		manual_endpoint_configuration_supported: false,
		protocol: "oidc",
		supports_discovery: true,
		supports_email_verified_claim: true,
		token_url_required: false,
		userinfo_url_required: false,
		...overrides,
	});
}

function microsoftKind(
	overrides: Partial<AdminExternalAuthProviderKindInfo> = {},
): AdminExternalAuthProviderKindInfo {
	return kind({
		authorization_url_required: false,
		default_scopes: "openid profile email",
		description: "Microsoft OpenID Connect sign-in.",
		display_name: "Microsoft",
		issuer_url_required: false,
		kind: "microsoft",
		manual_endpoint_configuration_supported: false,
		protocol: "oidc",
		supports_discovery: true,
		supports_email_verified_claim: false,
		token_url_required: false,
		userinfo_url_required: false,
		...overrides,
	});
}

function qqKind(
	overrides: Partial<AdminExternalAuthProviderKindInfo> = {},
): AdminExternalAuthProviderKindInfo {
	return kind({
		authorization_url_required: false,
		default_scopes: "get_user_info",
		description: "QQ sign-in.",
		display_name: "QQ",
		issuer_url_required: false,
		kind: "qq",
		manual_endpoint_configuration_supported: false,
		protocol: "oauth2",
		supports_discovery: false,
		supports_email_verified_claim: false,
		token_url_required: false,
		userinfo_url_required: false,
		...overrides,
	});
}

describe("admin external auth shared helpers", () => {
	it("normalizes domains and payload text fields", () => {
		expect(parseAllowedDomains(" @Example.COM, example.com\nTeam.io ")).toEqual(
			["example.com", "team.io"],
		);

		const form = {
			...emptyForm,
			allowedDomains: " @Example.COM, example.com\nTeam.io ",
			authorizationUrl: " ",
			autoLinkVerifiedEmailEnabled: true,
			autoProvisionEnabled: true,
			clientId: " client-123 ",
			clientSecret: " secret ",
			displayName: " Example IDP ",
			iconUrl: " /static/idp.svg ",
			issuerUrl: " https://idp.example.com ",
			scopes: " ",
			subjectClaim: " sub_override ",
		};

		expect(createPayload(form)).toMatchObject({
			allowed_domains: ["example.com", "team.io"],
			authorization_url: null,
			auto_link_verified_email_enabled: true,
			auto_provision_enabled: true,
			client_id: "client-123",
			client_secret: "secret",
			display_name: "Example IDP",
			icon_url: "/static/idp.svg",
			issuer_url: "https://idp.example.com",
			scopes: "openid email profile",
			subject_claim: "sub_override",
		});

		const update = updatePayload({ ...form, clientSecret: " ***REDACTED*** " });
		expect(update).not.toHaveProperty("client_secret");
		expect(update).not.toHaveProperty("provider_kind");

		expect(
			testParamsPayload({ ...form, clientSecret: "***REDACTED***" }),
		).toMatchObject({
			client_secret: null,
			client_id: "client-123",
			provider_kind: "oidc",
		});
	});

	it("uses provider descriptor defaults for generic OAuth2 scopes", () => {
		const descriptor = oauth2Kind();
		const form = {
			...emptyForm,
			authorizationUrl: "https://idp.example.com/authorize",
			clientId: "client-123",
			displayName: "Generic OAuth2",
			providerKind: "generic_oauth2" as const,
			scopes: " ",
			tokenUrl: "https://idp.example.com/token",
			userinfoUrl: "https://idp.example.com/userinfo",
		};

		expect(defaultScopesForKind(descriptor)).toBe("openid email profile");
		expect(createPayload(form, descriptor)).toMatchObject({
			provider_kind: "generic_oauth2",
			scopes: "openid email profile",
		});
		expect(updatePayload(form, descriptor)).toMatchObject({
			scopes: "openid email profile",
		});
		expect(testParamsPayload(form, descriptor)).toMatchObject({
			provider_kind: "generic_oauth2",
			scopes: "openid email profile",
		});
	});

	it("uses GitHub descriptor defaults and fixed summaries", () => {
		const descriptor = githubKind();
		const form = {
			...emptyForm,
			clientId: "client-123",
			displayName: "GitHub",
			providerKind: "github",
			scopes: " ",
		};

		expect(defaultScopesForKind(descriptor)).toBe("read:user user:email");
		expect(createPayload(form, descriptor)).toMatchObject({
			authorization_url: null,
			provider_kind: "github",
			scopes: "read:user user:email",
			token_url: null,
			userinfo_url: null,
		});
		expect(connectionRequirementsMissing(form, descriptor)).toBe(false);
		expect(formConnectionSummary(form, descriptor)).toContain(
			"https://api.github.com/user/emails",
		);
		expect(formClaimSummary(form, descriptor)).toBe(
			"subject=id · username=login · display=name · email=primary && verified from /user/emails",
		);
		expect(callbackUrl("github", "github")).toBe(
			"https://app.example.com/api/v1/auth/external-auth/github/github/callback",
		);
	});

	it("uses Google descriptor defaults and fixed OIDC summaries", () => {
		const descriptor = googleKind();
		const form = {
			...emptyForm,
			clientId: "client-123",
			displayName: "Google",
			providerKind: "google",
			scopes: " ",
		};

		expect(defaultScopesForKind(descriptor)).toBe("openid profile email");
		expect(createPayload(form, descriptor)).toMatchObject({
			authorization_url: null,
			issuer_url: null,
			provider_kind: "google",
			scopes: "openid profile email",
			token_url: null,
			userinfo_url: null,
		});
		expect(connectionRequirementsMissing(form, descriptor)).toBe(false);
		expect(shouldShowIssuerUrl(descriptor)).toBe(false);
		expect(formConnectionSummary(form, descriptor)).toBe(
			"issuer: https://accounts.google.com · discovery: https://accounts.google.com/.well-known/openid-configuration",
		);
		expect(formClaimSummary(form, descriptor)).toBe(
			"subject=sub · display=name · email=email · email_verified=email_verified · avatar=picture",
		);
		expect(callbackUrl("google", "google")).toBe(
			"https://app.example.com/api/v1/auth/external-auth/google/google/callback",
		);
	});

	it("uses Microsoft descriptor defaults and tenant-derived OIDC summaries", () => {
		const descriptor = microsoftKind();
		const form = {
			...emptyForm,
			clientId: "client-123",
			displayName: "Microsoft",
			microsoftTenant: "organizations",
			microsoftTenantMode: "organizations" as const,
			providerKind: "microsoft" as const,
			scopes: " ",
		};

		expect(defaultScopesForKind(descriptor)).toBe("openid profile email");
		expect(microsoftIssuerUrlForTenant("organizations")).toBe(
			"https://login.microsoftonline.com/organizations/v2.0",
		);
		expect(microsoftIssuerUrlForTenant(" ")).toBe(
			"https://login.microsoftonline.com/common/v2.0",
		);
		expect(
			microsoftIssuerUrlForTenant(
				"https://login.microsoftonline.com/contoso/v2.0/",
			),
		).toBe("https://login.microsoftonline.com/contoso/v2.0");
		expect(
			microsoftTenantFromIssuerUrl(
				"https://login.microsoftonline.com/consumers/v2.0",
			),
		).toBe("consumers");
		expect(
			microsoftTenantFromIssuerUrl("https://example.com/common/v2.0"),
		).toBe("");
		expect(createPayload(form, descriptor)).toMatchObject({
			authorization_url: null,
			issuer_url: "https://login.microsoftonline.com/organizations/v2.0",
			provider_kind: "microsoft",
			require_email_verified: true,
			scopes: "openid profile email",
			token_url: null,
			userinfo_url: null,
		});
		expect(connectionRequirementsMissing(form, descriptor)).toBe(false);
		expect(
			connectionRequirementsMissing(
				{
					...form,
					microsoftTenant: "",
					microsoftTenantMode: "custom",
				},
				descriptor,
			),
		).toBe(true);
		expect(
			createPayload(
				{
					...form,
					microsoftTenant: "11111111-2222-3333-4444-555555555555",
					microsoftTenantMode: "custom",
				},
				descriptor,
			),
		).toMatchObject({
			issuer_url:
				"https://login.microsoftonline.com/11111111-2222-3333-4444-555555555555/v2.0",
		});
		expect(shouldShowIssuerUrl(descriptor)).toBe(false);
		expect(formConnectionSummary(form, descriptor)).toBe(
			"tenant: organizations · OIDC discovery",
		);
		expect(formClaimSummary(form, descriptor)).toBe(
			"subject=sub · display=name · email=email",
		);
		expect(
			formFromProvider(
				provider({
					issuer_url: "https://login.microsoftonline.com/common/v2.0",
					provider_kind: "microsoft",
				}),
			),
		).toMatchObject({
			microsoftTenantMode: "common",
			microsoftTenant: "common",
			providerKind: "microsoft",
		});
		expect(
			formFromProvider(
				provider({
					issuer_url:
						"https://login.microsoftonline.com/11111111-2222-3333-4444-555555555555/v2.0",
					provider_kind: "microsoft",
				}),
			),
		).toMatchObject({
			microsoftTenantMode: "custom",
			microsoftTenant: "11111111-2222-3333-4444-555555555555",
		});
		expect(callbackUrl("microsoft", "microsoft")).toBe(
			"https://app.example.com/api/v1/auth/external-auth/microsoft/microsoft/callback",
		);
	});

	it("uses QQ descriptor defaults and fixed OAuth2 summaries", () => {
		const descriptor = qqKind();
		const form = {
			...emptyForm,
			clientId: "100000001",
			displayName: "QQ",
			providerKind: "qq" as const,
			requireEmailVerified: false,
			scopes: " ",
		};

		expect(defaultScopesForKind(descriptor)).toBe("get_user_info");
		expect(createPayload(form, descriptor)).toMatchObject({
			authorization_url: null,
			issuer_url: null,
			provider_kind: "qq",
			require_email_verified: false,
			scopes: "get_user_info",
			token_url: null,
			userinfo_url: null,
		});
		expect(connectionRequirementsMissing(form, descriptor)).toBe(false);
		expect(shouldShowIssuerUrl(descriptor)).toBe(false);
		expect(formConnectionSummary(form, descriptor)).toContain(
			"https://graph.qq.com/oauth2.0/me",
		);
		expect(formClaimSummary(form, descriptor)).toBe(
			"subject=openid · display=nickname · email=not returned",
		);
		expect(callbackUrl("qq", "qq")).toBe(
			"https://app.example.com/api/v1/auth/external-auth/qq/qq/callback",
		);
	});

	it("maps saved providers into editable forms and detects connection changes", () => {
		const saved = provider();
		const form = formFromProvider(saved);

		expect(form).toMatchObject({
			allowedDomains: "example.com, example.org",
			clientId: "client-123",
			clientSecret: "",
			iconUrl: " /static/idp.svg ",
			issuerUrl: "https://idp.example.com",
		});
		expect(
			formConnectionChanged({ ...form, clientSecret: "***REDACTED***" }, saved),
		).toBe(false);
		expect(
			formConnectionChanged({ ...form, clientSecret: "new-secret" }, saved),
		).toBe(true);
		expect(formConnectionChanged({ ...form, scopes: "openid" }, saved)).toBe(
			true,
		);
		expect(
			formConnectionChanged(
				{ ...form, clientSecret: "" },
				provider({ client_secret_configured: false }),
			),
		).toBe(false);
		expect(
			formConnectionChanged(
				{
					...form,
					clientSecret: "***REDACTED***",
					providerKind: "generic_oauth2",
					scopes: "",
				},
				provider({
					client_secret_configured: true,
					provider_kind: "generic_oauth2",
					protocol: "oauth2",
					scopes: "openid email profile",
				}),
				oauth2Kind(),
			),
		).toBe(false);
	});

	it("checks connection requirements and summarizes connection and claims", () => {
		const manualKind = kind({
			authorization_url_required: true,
			issuer_url_required: false,
			manual_endpoint_configuration_supported: true,
			token_url_required: true,
			userinfo_url_required: true,
		});
		const form = {
			...emptyForm,
			authorizationUrl: "https://idp.example.com/authorize",
			clientId: "client-123",
			displayName: "Example IDP",
			emailClaim: "mail",
			issuerUrl: "",
			tokenUrl: "https://idp.example.com/token",
			userinfoUrl: "https://idp.example.com/userinfo",
		};

		expect(connectionRequirementsMissing(emptyForm, manualKind)).toBe(true);
		expect(connectionRequirementsMissing(form, manualKind)).toBe(false);
		expect(
			connectionRequirementsMissing({ ...form, issuerUrl: "" }, kind()),
		).toBe(true);
		expect(
			connectionRequirementsMissing(
				{ ...form, authorizationUrl: "" },
				manualKind,
			),
		).toBe(true);
		expect(
			connectionRequirementsMissing({ ...form, tokenUrl: "" }, manualKind),
		).toBe(true);
		expect(
			connectionRequirementsMissing({ ...form, userinfoUrl: "" }, manualKind),
		).toBe(true);
		expect(
			requiredFieldsMissing({ ...form, displayName: "" }, manualKind),
		).toBe(true);
		expect(formConnectionSummary(form, manualKind)).toBe(
			"authorization: https://idp.example.com/authorize · token: https://idp.example.com/token · userinfo: https://idp.example.com/userinfo",
		);
		expect(formConnectionSummary(emptyForm, manualKind)).toBe("-");
		expect(formClaimSummary(form, manualKind)).toContain("email=mail");
		expect(
			formClaimSummary(form, kind({ supports_email_verified_claim: false })),
		).not.toContain("email_verified=");
		expect(providerIconSummary(form)).toBe("-");
		expect(providerIconSummary({ ...form, iconUrl: " /idp.svg " })).toBe(
			"/idp.svg",
		);
	});

	it("formats labels, statuses, callback URLs, endpoints, and pagination params", () => {
		const translate = (key: string) => key;

		expect(kindDisplayName(translate as never, "oidc", [])).toBe(
			"OpenID Connect",
		);
		expect(kindDisplayName(translate as never, "generic_oauth2", [])).toBe(
			"Generic OAuth2",
		);
		expect(kindDisplayName(translate as never, "github", [])).toBe("GitHub");
		expect(kindDisplayName(translate as never, "google", [])).toBe("Google");
		expect(kindDisplayName(translate as never, "microsoft", [])).toBe(
			"Microsoft",
		);
		expect(kindDescription(translate as never, kind())).toBe("OIDC sign-in.");
		expect(securityModeLabel(translate as never, provider())).toBe(
			"external_auth_provider_mode_manual",
		);
		expect(
			securityModeLabel(
				translate as never,
				provider({ auto_link_verified_email_enabled: true }),
			),
		).toBe("external_auth_provider_mode_link");
		expect(
			securityModeLabel(
				translate as never,
				provider({ auto_provision_enabled: true }),
			),
		).toBe("external_auth_provider_mode_provision");
		expect(
			securityModeLabel(
				translate as never,
				provider({
					auto_link_verified_email_enabled: true,
					auto_provision_enabled: true,
				}),
			),
		).toBe("external_auth_provider_mode_link_and_provision");
		expect(callbackUrl("oidc", "example idp")).toBe(
			"https://app.example.com/api/v1/auth/external-auth/oidc/example%20idp/callback",
		);
		expect(callbackUrl("oidc", " ")).toBe("");
		expect(providerPrimaryEndpoint(provider())?.labelKey).toBe(
			"external_auth_provider_issuer_url",
		);
		expect(
			providerPrimaryEndpoint(
				provider({ authorization_url: "https://authorize", issuer_url: null }),
			)?.labelKey,
		).toBe("external_auth_provider_authorization_url");
		expect(
			providerPrimaryEndpoint(
				provider({ issuer_url: null, token_url: "https://token" }),
			)?.labelKey,
		).toBe("external_auth_provider_token_url");
		expect(
			providerPrimaryEndpoint(
				provider({
					issuer_url: null,
					token_url: null,
					userinfo_url: "https://user",
				}),
			)?.labelKey,
		).toBe("external_auth_provider_userinfo_url");
		expect(
			providerPrimaryEndpoint(
				provider({ issuer_url: null, token_url: null, userinfo_url: null }),
			),
		).toBeNull();
		expect(providerAllowedDomainSummary(translate as never, provider())).toBe(
			"example.com, example.org",
		);
		expect(
			providerAllowedDomainSummary(
				translate as never,
				provider({ allowed_domains: [] }),
			),
		).toBe("external_auth_provider_allowed_domains_all");
		expect(normalizeOffset(-3.8)).toBe(0);
		expect(normalizeOffset(3.8)).toBe(3);
		expect(
			buildManagedExternalAuthSearchParams({
				offset: 20,
				pageSize: 10,
			}).toString(),
		).toBe("offset=20&pageSize=10");
		expect(
			getManagedExternalAuthSearchString(
				new URLSearchParams("offset=-1&pageSize=999&keep=1"),
			),
		).toBe("");
		expect(
			mergeManagedExternalAuthSearchParams(
				new URLSearchParams("offset=20&pageSize=10&keep=1"),
				new URLSearchParams("pageSize=50"),
			).toString(),
		).toBe("keep=1&pageSize=50");
	});

	it("formats test results and handles callback copy button state", () => {
		const result: ExternalAuthProviderTestResult = {
			authorization_endpoint: "https://authorize",
			checks: [
				{ message: "ready", name: "issuer", success: true },
				{ message: "failed", name: "jwks", success: false },
			],
			issuer: "https://issuer",
			jwks_key_count: 1,
			provider: "OpenID Connect",
			token_endpoint: "https://token",
			userinfo_endpoint: null,
		};
		expect(formatTestResultSummary(t as never, result)).toBe(
			"ok:issuer:ready · error:jwks:failed",
		);
		expect(formatTestResultSummary(t as never, { ...result, checks: [] })).toBe(
			"success:OpenID Connect",
		);

		const onCopy = vi.fn();
		const { rerender } = render(<CallbackUrlField value="" onCopy={onCopy} />);
		expect(
			screen.getByRole("button", {
				name: "external_auth_provider_copy_callback_url",
			}),
		).toBeDisabled();
		expect(screen.getByText("-")).toBeInTheDocument();

		rerender(<CallbackUrlField value="https://callback" onCopy={onCopy} />);
		fireEvent.click(
			screen.getByRole("button", {
				name: "external_auth_provider_copy_callback_url",
			}),
		);

		expect(onCopy).toHaveBeenCalledWith("https://callback");
	});
});
