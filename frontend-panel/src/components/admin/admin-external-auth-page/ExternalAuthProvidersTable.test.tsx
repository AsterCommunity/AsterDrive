import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ExternalAuthProvidersTable } from "@/components/admin/admin-external-auth-page/ExternalAuthProvidersTable";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
} from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/layout/AdminSurface", () => ({
	AdminSurface: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
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
		onClick?: () => void;
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

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/table", () => ({
	Table: ({ children }: { children: React.ReactNode }) => (
		<table>{children}</table>
	),
	TableBody: ({ children }: { children: React.ReactNode }) => (
		<tbody>{children}</tbody>
	),
	TableCell: ({
		children,
		onClick,
		onKeyDown,
	}: {
		children: React.ReactNode;
		onClick?: React.MouseEventHandler<HTMLTableCellElement>;
		onKeyDown?: React.KeyboardEventHandler<HTMLTableCellElement>;
	}) => (
		<td onClick={onClick} onKeyDown={onKeyDown}>
			{children}
		</td>
	),
	TableHead: ({ children }: { children: React.ReactNode }) => (
		<th>{children}</th>
	),
	TableHeader: ({ children }: { children: React.ReactNode }) => (
		<thead>{children}</thead>
	),
	TableRow: ({
		children,
		onClick,
		onKeyDown,
		tabIndex,
	}: {
		children: React.ReactNode;
		onClick?: React.MouseEventHandler<HTMLTableRowElement>;
		onKeyDown?: React.KeyboardEventHandler<HTMLTableRowElement>;
		tabIndex?: number;
	}) => (
		<tr onClick={onClick} onKeyDown={onKeyDown} tabIndex={tabIndex}>
			{children}
		</tr>
	),
}));

vi.mock("@/lib/format", () => ({
	formatDateAbsolute: (value: string) => `date:${value}`,
	formatDateAbsoluteWithOffset: (value: string) => `offset:${value}`,
}));

vi.mock("@/lib/publicSiteUrl", () => ({
	absoluteAppUrl: (path: string) => `https://app.example.com${path}`,
}));

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
		client_secret_configured: false,
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
		options: {},
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

const providerKinds: AdminExternalAuthProviderKindInfo[] = [
	{
		authorization_url_required: false,
		create_defaults: {
			auto_link_verified_email_enabled: false,
			auto_provision_enabled: false,
			display_name: "",
			enabled: true,
			options: {},
			require_email_verified: true,
			scopes: "openid email profile",
		},
		default_scopes: "openid email profile",
		description: "OIDC sign-in.",
		display_name: "OpenID Connect",
		issuer_url_supported: true,
		issuer_url_required: true,
		kind: "oidc",
		manual_endpoint_configuration_supported: false,
		protocol: "oidc",
		supports_discovery: true,
		supports_email_verified_claim: true,
		supports_pkce: true,
		token_url_required: false,
		userinfo_url_required: false,
	},
];

describe("ExternalAuthProvidersTable", () => {
	it("renders provider summaries and wires row/button actions without bubbling", () => {
		const onCopyCallbackUrl = vi.fn();
		const onEdit = vi.fn();
		const onRequestDelete = vi.fn();
		const onTestProvider = vi.fn();
		const item = provider();

		render(
			<ExternalAuthProvidersTable
				deletingId={null}
				items={[item]}
				onCopyCallbackUrl={onCopyCallbackUrl}
				onEdit={onEdit}
				onRequestDelete={onRequestDelete}
				onTestProvider={onTestProvider}
				providerKinds={providerKinds}
				testingId={null}
			/>,
		);

		expect(screen.getByText("Example IDP")).toBeInTheDocument();
		expect(screen.getByText("OpenID Connect")).toBeInTheDocument();
		expect(screen.getByText("https://idp.example.com")).toBeInTheDocument();
		expect(screen.getByText("example.com")).toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_enabled_badge"),
		).toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_mode_manual"),
		).toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_require_email_verified"),
		).toBeInTheDocument();

		const row = screen.getByText("Example IDP").closest("tr");
		if (!row) throw new Error("provider row not found");
		fireEvent.click(row);
		expect(onEdit).toHaveBeenCalledWith(item);

		fireEvent.keyDown(row, { key: " " });
		fireEvent.keyDown(row, { key: "Enter" });
		expect(onEdit).toHaveBeenCalledTimes(3);

		fireEvent.click(
			screen.getByRole("button", {
				name: "external_auth_provider_copy_callback_url",
			}),
		);
		expect(onCopyCallbackUrl).toHaveBeenCalledWith(
			"https://app.example.com/api/v1/auth/external-auth/oidc/example/callback",
		);
		expect(onEdit).toHaveBeenCalledTimes(3);

		fireEvent.click(
			screen.getByRole("button", { name: "external_auth_provider_test" }),
		);
		expect(onTestProvider).toHaveBeenCalledWith(item);
		expect(onEdit).toHaveBeenCalledTimes(3);

		fireEvent.click(
			screen.getByRole("button", { name: "external_auth_provider_delete" }),
		);
		expect(onRequestDelete).toHaveBeenCalledWith(1);
		expect(onEdit).toHaveBeenCalledTimes(3);
	});

	it("shows fallback summaries and disables row actions while deleting", () => {
		const onCopyCallbackUrl = vi.fn();
		const onEdit = vi.fn();
		const disabledProvider = provider({
			allowed_domains: [],
			auto_link_verified_email_enabled: true,
			display_name: "Manual IDP",
			enabled: false,
			id: 2,
			issuer_url: null,
			key: "",
			require_email_verified: false,
			scopes: "",
			token_url: "https://idp.example.com/token",
		});

		render(
			<ExternalAuthProvidersTable
				deletingId={2}
				items={[disabledProvider]}
				onCopyCallbackUrl={onCopyCallbackUrl}
				onEdit={onEdit}
				onRequestDelete={vi.fn()}
				onTestProvider={vi.fn()}
				providerKinds={providerKinds}
				testingId={null}
			/>,
		);

		expect(
			screen.getByText("https://idp.example.com/token"),
		).toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_allowed_domains_all"),
		).toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_disabled_badge"),
		).toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_mode_link"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("external_auth_provider_require_email_verified"),
		).not.toBeInTheDocument();

		const row = screen.getByText("Manual IDP").closest("tr");
		if (!row) throw new Error("provider row not found");
		fireEvent.click(row);
		fireEvent.keyDown(row, { key: "Enter" });
		expect(onEdit).not.toHaveBeenCalled();

		const actionsCell = screen
			.getByRole("button", {
				name: "external_auth_provider_copy_callback_url",
			})
			.closest("td");
		if (!actionsCell) throw new Error("actions cell not found");
		expect(
			within(actionsCell).getByRole("button", {
				name: "external_auth_provider_copy_callback_url",
			}),
		).toBeDisabled();
		expect(
			within(actionsCell).getByRole("button", {
				name: "external_auth_provider_test",
			}),
		).toBeDisabled();
		expect(
			within(actionsCell).getByRole("button", {
				name: "external_auth_provider_delete",
			}),
		).toBeDisabled();
	});
});
