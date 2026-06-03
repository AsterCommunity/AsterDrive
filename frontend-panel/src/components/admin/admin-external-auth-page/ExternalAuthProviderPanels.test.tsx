import { fireEvent, render, screen } from "@testing-library/react";
import * as React from "react";
import { describe, expect, it, vi } from "vitest";
import {
	ExternalAuthAccessPolicyPanel,
	ExternalAuthProviderIdentityPanel,
	ExternalAuthProviderKindPanel,
	ExternalAuthProviderRulesPanel,
	ExternalAuthSummaryPanel,
} from "@/components/admin/admin-external-auth-page/ExternalAuthProviderPanels";
import {
	type ExternalAuthProviderFormData,
	emptyForm,
} from "@/components/admin/admin-external-auth-page/shared";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
} from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/admin/TestConnectionButton", () => ({
	TestConnectionButton: ({
		disabled,
		onTest,
	}: {
		disabled?: boolean;
		onTest: () => Promise<boolean>;
	}) => (
		<button type="button" disabled={disabled} onClick={() => void onTest()}>
			test-connection
		</button>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
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

vi.mock("@/components/ui/input", () => ({
	Input: ({ ...props }: React.InputHTMLAttributes<HTMLInputElement>) => (
		<input {...props} />
	),
}));

vi.mock("@/components/ui/label", () => ({
	Label: ({
		children,
		htmlFor,
	}: {
		children: React.ReactNode;
		htmlFor?: string;
	}) => <label htmlFor={htmlFor}>{children}</label>,
}));

vi.mock("@/components/ui/select", () => {
	const SelectContext = React.createContext<{
		items?: Array<{ label: string; value: string }>;
		onValueChange?: (value: string) => void;
		value?: string;
	}>({});
	return {
		Select: ({
			children,
			items,
			onValueChange,
			value,
		}: {
			children: React.ReactNode;
			items?: Array<{ label: string; value: string }>;
			onValueChange?: (value: string) => void;
			value?: string;
		}) => (
			<SelectContext.Provider value={{ items, onValueChange, value }}>
				{children}
			</SelectContext.Provider>
		),
		SelectContent: () => null,
		SelectItem: ({
			children,
			value,
		}: {
			children: React.ReactNode;
			value: string;
		}) => <option value={value}>{children}</option>,
		SelectTrigger: ({
			children,
			id,
		}: {
			children: React.ReactNode;
			id?: string;
		}) => {
			const { items, onValueChange, value } = React.use(SelectContext);
			return (
				<select
					id={id}
					value={value}
					onChange={(event) => onValueChange?.(event.target.value)}
				>
					{children}
					{items?.map((item) => (
						<option key={item.value} value={item.value}>
							{item.label}
						</option>
					))}
				</select>
			);
		},
		SelectValue: () => null,
	};
});

vi.mock("@/components/ui/switch", () => ({
	Switch: ({
		checked,
		id,
		onCheckedChange,
	}: {
		checked?: boolean;
		id?: string;
		onCheckedChange?: (checked: boolean) => void;
	}) => (
		<button
			type="button"
			id={id}
			role="switch"
			aria-checked={checked ? "true" : "false"}
			onClick={() => onCheckedChange?.(!checked)}
		/>
	),
}));

function form(
	overrides: Partial<ExternalAuthProviderFormData> = {},
): ExternalAuthProviderFormData {
	return {
		...emptyForm,
		allowedDomains: "example.com, example.org",
		avatarUrlClaim: "picture",
		clientId: "client-123",
		clientSecret: "",
		displayName: "Example IDP",
		displayNameClaim: "name",
		emailClaim: "email",
		emailVerifiedClaim: "email_verified",
		groupsClaim: "groups",
		iconUrl: "/static/idp.svg",
		issuerUrl: "https://idp.example.com",
		key: "example",
		scopes: "openid email profile",
		subjectClaim: "sub",
		usernameClaim: "preferred_username",
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
		description: "Microsoft sign-in.",
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

function provider(
	overrides: Partial<AdminExternalAuthProviderInfo> = {},
): AdminExternalAuthProviderInfo {
	return {
		allowed_domains: ["example.com"],
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
		icon_url: null,
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

describe("ExternalAuthProviderPanels", () => {
	it("wires access policy switches to form fields", () => {
		const onFieldChange = vi.fn();

		render(
			<ExternalAuthAccessPolicyPanel
				form={form({
					autoLinkVerifiedEmailEnabled: true,
					autoProvisionEnabled: false,
					enabled: true,
					requireEmailVerified: false,
				})}
				onFieldChange={onFieldChange}
			/>,
		);

		fireEvent.click(
			screen.getByRole("switch", {
				name: "external_auth_provider_enabled",
			}),
		);
		fireEvent.click(
			screen.getByRole("switch", {
				name: "external_auth_provider_require_email_verified",
			}),
		);
		fireEvent.click(
			screen.getByRole("switch", {
				name: "external_auth_provider_auto_link",
			}),
		);
		fireEvent.click(
			screen.getByRole("switch", {
				name: "external_auth_provider_auto_provision",
			}),
		);

		expect(onFieldChange).toHaveBeenNthCalledWith(1, "enabled", false);
		expect(onFieldChange).toHaveBeenNthCalledWith(
			2,
			"requireEmailVerified",
			true,
		);
		expect(onFieldChange).toHaveBeenNthCalledWith(
			3,
			"autoLinkVerifiedEmailEnabled",
			false,
		);
		expect(onFieldChange).toHaveBeenNthCalledWith(
			4,
			"autoProvisionEnabled",
			true,
		);
	});

	it("renders connection fields, required markers, and create warnings", () => {
		const onFieldChange = vi.fn();
		const onTestConnection = vi.fn().mockResolvedValue(true);

		render(
			<ExternalAuthProviderIdentityPanel
				connectionMissing
				createStepTouched
				currentCallbackUrl=""
				form={form({
					authorizationUrl: "",
					clientId: "",
					clientSecret: "",
					displayName: "",
					iconUrl: "",
					issuerUrl: "",
					tokenUrl: "",
					userinfoUrl: "",
				})}
				identityMissing
				isCreate
				onCopyCallbackUrl={vi.fn()}
				onFieldChange={onFieldChange}
				onTestConnection={onTestConnection}
				provider={null}
				providerKindLabel="OpenID Connect"
				selectedKind={kind({
					authorization_url_required: true,
					token_url_required: true,
					userinfo_url_required: true,
				})}
				showIssuerUrl
				showManualEndpoints
				testDisabled
				testResult="connection ok"
			/>,
		);

		expect(
			screen.getByLabelText("external_auth_provider_display_name"),
		).toHaveAttribute("aria-invalid", "true");
		expect(
			screen.getByLabelText("external_auth_provider_issuer_url"),
		).toHaveAttribute("aria-invalid", "true");
		expect(
			screen.getByLabelText("external_auth_provider_authorization_url"),
		).toHaveAttribute("aria-invalid", "true");
		expect(
			screen.getByLabelText("external_auth_provider_token_url"),
		).toHaveAttribute("aria-invalid", "true");
		expect(
			screen.getByLabelText("external_auth_provider_userinfo_url"),
		).toHaveAttribute("aria-invalid", "true");
		expect(
			screen.getByLabelText("external_auth_provider_client_id"),
		).toHaveAttribute("aria-invalid", "true");
		expect(
			screen.getByText("external_auth_provider_wizard_required"),
		).toBeInTheDocument();
		expect(screen.getByText("connection ok")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "test-connection" }),
		).toBeDisabled();
		expect(
			screen.getByText("external_auth_provider_secret_hint"),
		).toBeInTheDocument();

		fireEvent.change(
			screen.getByLabelText("external_auth_provider_display_name"),
			{ target: { value: "Example IDP" } },
		);
		fireEvent.change(screen.getByLabelText("external_auth_provider_icon_url"), {
			target: { value: "/static/idp.svg" },
		});
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_issuer_url"),
			{
				target: { value: "https://idp.example.com" },
			},
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_authorization_url"),
			{ target: { value: "https://idp.example.com/authorize" } },
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_token_url"),
			{
				target: { value: "https://idp.example.com/token" },
			},
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_userinfo_url"),
			{ target: { value: "https://idp.example.com/userinfo" } },
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_id"),
			{
				target: { value: "client-123" },
			},
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_secret"),
			{ target: { value: "secret" } },
		);

		expect(onFieldChange).toHaveBeenCalledWith("displayName", "Example IDP");
		expect(onFieldChange).toHaveBeenCalledWith("iconUrl", "/static/idp.svg");
		expect(onFieldChange).toHaveBeenCalledWith(
			"issuerUrl",
			"https://idp.example.com",
		);
		expect(onFieldChange).toHaveBeenCalledWith(
			"authorizationUrl",
			"https://idp.example.com/authorize",
		);
		expect(onFieldChange).toHaveBeenCalledWith(
			"tokenUrl",
			"https://idp.example.com/token",
		);
		expect(onFieldChange).toHaveBeenCalledWith(
			"userinfoUrl",
			"https://idp.example.com/userinfo",
		);
		expect(onFieldChange).toHaveBeenCalledWith("clientId", "client-123");
		expect(onFieldChange).toHaveBeenCalledWith("clientSecret", "secret");
	});

	it("renders edit-only identity details and copies callback URLs", () => {
		const onCopyCallbackUrl = vi.fn();
		const onTestConnection = vi.fn().mockResolvedValue(true);

		render(
			<ExternalAuthProviderIdentityPanel
				connectionMissing={false}
				createStepTouched={false}
				currentCallbackUrl="https://app.example.com/api/callback"
				form={form()}
				identityMissing={false}
				isCreate={false}
				onCopyCallbackUrl={onCopyCallbackUrl}
				onFieldChange={vi.fn()}
				onTestConnection={onTestConnection}
				provider={provider()}
				providerKindLabel="OpenID Connect"
				selectedKind={kind()}
				showIssuerUrl={false}
				showManualEndpoints={false}
				testDisabled={false}
				testResult={null}
			/>,
		);

		expect(screen.getByText("OpenID Connect")).toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_issuer_url"),
		).not.toBeInTheDocument();
		expect(
			screen.getByLabelText("external_auth_provider_client_secret"),
		).toHaveAttribute(
			"placeholder",
			"external_auth_provider_secret_keep_placeholder",
		);
		expect(
			screen.getByText("external_auth_provider_secret_keep_hint"),
		).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "test-connection" }));
		expect(onTestConnection).toHaveBeenCalledTimes(1);
		fireEvent.click(
			screen.getByRole("button", {
				name: "external_auth_provider_copy_callback_url",
			}),
		);
		expect(onCopyCallbackUrl).toHaveBeenCalledWith(
			"https://app.example.com/api/callback",
		);
	});

	it("hides manual endpoint fields for GitHub in the identity panel", () => {
		render(
			<ExternalAuthProviderIdentityPanel
				connectionMissing={false}
				createStepTouched={false}
				currentCallbackUrl=""
				form={form({
					providerKind: "github",
					scopes: "read:user user:email",
				})}
				identityMissing={false}
				isCreate
				onCopyCallbackUrl={vi.fn()}
				onFieldChange={vi.fn()}
				onTestConnection={vi.fn().mockResolvedValue(true)}
				provider={null}
				providerKindLabel="GitHub"
				selectedKind={githubKind()}
				showIssuerUrl={false}
				showManualEndpoints={false}
				testDisabled={false}
				testResult={null}
			/>,
		);

		expect(
			screen.queryByText("external_auth_provider_github_fixed_title"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_authorization_url"),
		).not.toBeInTheDocument();
	});

	it("hides issuer and manual endpoint fields for Google in the identity panel", () => {
		render(
			<ExternalAuthProviderIdentityPanel
				connectionMissing={false}
				createStepTouched={false}
				currentCallbackUrl=""
				form={form({
					providerKind: "google",
					scopes: "openid profile email",
				})}
				identityMissing={false}
				isCreate
				onCopyCallbackUrl={vi.fn()}
				onFieldChange={vi.fn()}
				onTestConnection={vi.fn().mockResolvedValue(true)}
				provider={null}
				providerKindLabel="Google"
				selectedKind={googleKind()}
				showIssuerUrl={false}
				showManualEndpoints={false}
				testDisabled={false}
				testResult={null}
			/>,
		);

		expect(
			screen.queryByText("external_auth_provider_google_fixed_title"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_issuer_url"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_authorization_url"),
		).not.toBeInTheDocument();
	});

	it("hides manual endpoint fields for QQ in the identity panel", () => {
		render(
			<ExternalAuthProviderIdentityPanel
				connectionMissing={false}
				createStepTouched={false}
				currentCallbackUrl=""
				form={form({
					providerKind: "qq",
					scopes: "get_user_info",
				})}
				identityMissing={false}
				isCreate
				onCopyCallbackUrl={vi.fn()}
				onFieldChange={vi.fn()}
				onTestConnection={vi.fn().mockResolvedValue(true)}
				provider={null}
				providerKindLabel="QQ"
				selectedKind={qqKind()}
				showIssuerUrl={false}
				showManualEndpoints={false}
				testDisabled={false}
				testResult={null}
			/>,
		);

		expect(
			screen.queryByText("external_auth_provider_qq_fixed_title"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_authorization_url"),
		).not.toBeInTheDocument();
	});

	it("renders Microsoft tenant OIDC guidance in the identity panel", () => {
		const onFieldChange = vi.fn();

		render(
			<ExternalAuthProviderIdentityPanel
				connectionMissing={false}
				createStepTouched={false}
				currentCallbackUrl=""
				form={form({
					microsoftTenant: "organizations",
					microsoftTenantMode: "organizations",
					providerKind: "microsoft",
					scopes: "openid profile email",
				})}
				identityMissing={false}
				isCreate
				onCopyCallbackUrl={vi.fn()}
				onFieldChange={onFieldChange}
				onTestConnection={vi.fn().mockResolvedValue(true)}
				provider={null}
				providerKindLabel="Microsoft"
				selectedKind={microsoftKind()}
				showIssuerUrl={false}
				showManualEndpoints={false}
				testDisabled={false}
				testResult={null}
			/>,
		);

		const tenantSelect = screen.getByLabelText(
			"external_auth_provider_microsoft_tenant",
		);
		expect(tenantSelect).toHaveValue("organizations");
		expect(
			screen.queryByText("external_auth_provider_microsoft_fixed_title"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByText(
				"https://login.microsoftonline.com/organizations/v2.0",
			),
		).not.toBeInTheDocument();
		expect(
			screen.queryByText(
				"https://login.microsoftonline.com/organizations/v2.0/.well-known/openid-configuration",
			),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_issuer_url"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_authorization_url"),
		).not.toBeInTheDocument();

		fireEvent.change(tenantSelect, { target: { value: "consumers" } });
		expect(onFieldChange).toHaveBeenCalledWith(
			"microsoftTenantMode",
			"consumers",
		);
		expect(onFieldChange).toHaveBeenCalledWith("microsoftTenant", "consumers");

		fireEvent.change(tenantSelect, { target: { value: "custom" } });
		expect(onFieldChange).toHaveBeenCalledWith("microsoftTenantMode", "custom");
		expect(onFieldChange).toHaveBeenCalledWith("microsoftTenant", "");
	});

	it("renders Microsoft custom tenant input when custom mode is selected", () => {
		const onFieldChange = vi.fn();

		render(
			<ExternalAuthProviderIdentityPanel
				connectionMissing={false}
				createStepTouched
				currentCallbackUrl=""
				form={form({
					microsoftTenant: "",
					microsoftTenantMode: "custom",
					providerKind: "microsoft",
					scopes: "openid profile email",
				})}
				identityMissing={false}
				isCreate
				onCopyCallbackUrl={vi.fn()}
				onFieldChange={onFieldChange}
				onTestConnection={vi.fn().mockResolvedValue(true)}
				provider={null}
				providerKindLabel="Microsoft"
				selectedKind={microsoftKind()}
				showIssuerUrl={false}
				showManualEndpoints={false}
				testDisabled={false}
				testResult={null}
			/>,
		);

		const customInput = screen.getByLabelText(
			"external_auth_provider_microsoft_tenant_custom_label",
		);
		expect(customInput).toHaveAttribute("aria-invalid", "true");

		fireEvent.change(customInput, {
			target: { value: "11111111-2222-3333-4444-555555555555" },
		});
		expect(onFieldChange).toHaveBeenCalledWith(
			"microsoftTenant",
			"11111111-2222-3333-4444-555555555555",
		);
	});

	it("does not mark a filled Microsoft custom tenant as invalid", () => {
		render(
			<ExternalAuthProviderIdentityPanel
				connectionMissing={false}
				createStepTouched
				currentCallbackUrl=""
				form={form({
					microsoftTenant: "11111111-2222-3333-4444-555555555555",
					microsoftTenantMode: "custom",
					providerKind: "microsoft",
					scopes: "openid profile email",
				})}
				identityMissing={false}
				isCreate
				onCopyCallbackUrl={vi.fn()}
				onFieldChange={vi.fn()}
				onTestConnection={vi.fn().mockResolvedValue(true)}
				provider={null}
				providerKindLabel="Microsoft"
				selectedKind={microsoftKind()}
				showIssuerUrl={false}
				showManualEndpoints={false}
				testDisabled={false}
				testResult={null}
			/>,
		);

		expect(
			screen.getByLabelText(
				"external_auth_provider_microsoft_tenant_custom_label",
			),
		).not.toHaveAttribute("aria-invalid");
	});

	it("wires rules and claim fields, including email-verified support", () => {
		const onFieldChange = vi.fn();
		const view = render(
			<ExternalAuthProviderRulesPanel
				form={form()}
				onFieldChange={onFieldChange}
				selectedKind={kind()}
			/>,
		);

		const fieldChanges = [
			["external_auth_provider_scopes", "scopes", "openid"],
			[
				"external_auth_provider_allowed_domains",
				"allowedDomains",
				"example.com",
			],
			["external_auth_provider_subject_claim", "subjectClaim", "sub_id"],
			["external_auth_provider_username_claim", "usernameClaim", "username"],
			[
				"external_auth_provider_display_name_claim",
				"displayNameClaim",
				"full_name",
			],
			["external_auth_provider_email_claim", "emailClaim", "mail"],
			["external_auth_provider_groups_claim", "groupsClaim", "memberOf"],
			[
				"external_auth_provider_email_verified_claim",
				"emailVerifiedClaim",
				"verified",
			],
			["external_auth_provider_avatar_url_claim", "avatarUrlClaim", "avatar"],
		] as const;

		for (const [label, field, value] of fieldChanges) {
			fireEvent.change(screen.getByLabelText(label), {
				target: { value },
			});
			expect(onFieldChange).toHaveBeenCalledWith(field, value);
		}

		view.rerender(
			<ExternalAuthProviderRulesPanel
				form={form()}
				onFieldChange={onFieldChange}
				selectedKind={kind({ supports_email_verified_claim: false })}
			/>,
		);
		expect(
			screen.queryByLabelText("external_auth_provider_email_verified_claim"),
		).not.toBeInTheDocument();
	});

	it("renders GitHub fixed claims instead of editable claim mapping", () => {
		render(
			<ExternalAuthProviderRulesPanel
				form={form({ providerKind: "github" })}
				onFieldChange={vi.fn()}
				selectedKind={githubKind()}
			/>,
		);

		expect(
			screen.getByText("external_auth_provider_github_claims_title"),
		).toBeInTheDocument();
		expect(
			screen.getByText("primary && verified from /user/emails"),
		).toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_subject_claim"),
		).not.toBeInTheDocument();
	});

	it("renders Google fixed claims instead of editable claim mapping", () => {
		render(
			<ExternalAuthProviderRulesPanel
				form={form({ providerKind: "google" })}
				onFieldChange={vi.fn()}
				selectedKind={googleKind()}
			/>,
		);

		expect(
			screen.getByText("external_auth_provider_google_claims_title"),
		).toBeInTheDocument();
		expect(screen.getByText("email_verified")).toBeInTheDocument();
		expect(screen.getByText("picture")).toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_subject_claim"),
		).not.toBeInTheDocument();
	});

	it("renders Microsoft fixed claims instead of editable claim mapping", () => {
		render(
			<ExternalAuthProviderRulesPanel
				form={form({ providerKind: "microsoft" })}
				onFieldChange={vi.fn()}
				selectedKind={microsoftKind()}
			/>,
		);

		expect(
			screen.getByText("external_auth_provider_microsoft_claims_title"),
		).toBeInTheDocument();
		expect(screen.getByText("sub")).toBeInTheDocument();
		expect(screen.getByText("name")).toBeInTheDocument();
		expect(screen.getByText("email")).toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_subject_claim"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_email_verified_claim"),
		).not.toBeInTheDocument();
	});

	it("renders QQ fixed claims instead of editable claim mapping", () => {
		render(
			<ExternalAuthProviderRulesPanel
				form={form({ providerKind: "qq", scopes: "get_user_info" })}
				onFieldChange={vi.fn()}
				selectedKind={qqKind()}
			/>,
		);

		expect(
			screen.getByText("external_auth_provider_qq_claims_title"),
		).toBeInTheDocument();
		expect(screen.getByText("openid")).toBeInTheDocument();
		expect(screen.getByText("nickname")).toBeInTheDocument();
		expect(screen.getByText("not returned")).toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_subject_claim"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_email_verified_claim"),
		).not.toBeInTheDocument();
	});

	it("renders provider kind choices and summary fallbacks", () => {
		const onProviderKindChange = vi.fn();
		const providerKinds = [kind()];
		const view = render(
			<ExternalAuthProviderKindPanel
				form={form({ providerKind: "oidc" })}
				onProviderKindChange={onProviderKindChange}
				providerKinds={providerKinds}
			/>,
		);

		const kindButton = screen.getByRole("button", { name: /OpenID Connect/ });
		expect(kindButton).toHaveAttribute("aria-pressed", "true");
		fireEvent.click(kindButton);
		expect(onProviderKindChange).toHaveBeenCalledWith("oidc");

		view.rerender(
			<ExternalAuthSummaryPanel
				currentCallbackUrl=""
				form={form({
					allowedDomains: "",
					iconUrl: "",
					issuerUrl: "",
					scopes: "",
				})}
				isCreate
				providerKind="generic_oauth2"
				providerKinds={[
					kind({
						default_scopes: "openid email profile",
						display_name: "Generic OAuth2",
						kind: "generic_oauth2",
						protocol: "oauth2",
					}),
				]}
				selectedKind={kind({
					default_scopes: "openid email profile",
					kind: "generic_oauth2",
					protocol: "oauth2",
				})}
			/>,
		);
		expect(screen.getByText("openid email profile")).toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_allowed_domains_all"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("https://app.example.com/callback"),
		).not.toBeInTheDocument();

		view.rerender(
			<ExternalAuthSummaryPanel
				currentCallbackUrl="https://app.example.com/callback"
				form={form()}
				isCreate={false}
				providerKind="oidc"
				providerKinds={providerKinds}
				selectedKind={kind()}
			/>,
		);
		expect(
			screen.getByText("https://app.example.com/callback"),
		).toBeInTheDocument();
		expect(screen.getByText("example.com, example.org")).toBeInTheDocument();
	});
});
