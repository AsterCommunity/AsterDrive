import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import * as React from "react";
import { useState } from "react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ActualAdminExternalAuthPage from "@/pages/admin/AdminExternalAuthPage";
import type { AdminExternalAuthProviderKindInfo } from "@/types/api";

const mockState = vi.hoisted(() => ({
	create: vi.fn(),
	deleteProvider: vi.fn(),
	get: vi.fn(),
	handleApiError: vi.fn(),
	list: vi.fn(),
	listKinds: vi.fn(),
	test: vi.fn(),
	testParams: vi.fn(),
	toastSuccess: vi.fn(),
	update: vi.fn(),
	writeTextToClipboard: vi.fn(),
	blocker: {
		proceed: vi.fn(),
		reset: vi.fn(),
		state: "unblocked" as "blocked" | "proceeding" | "unblocked",
	},
}));

vi.mock("react-router-dom", async (importOriginal) => {
	const actual = await importOriginal<typeof import("react-router-dom")>();
	return {
		...actual,
		useBlocker: (shouldBlock: boolean | (() => boolean)) => ({
			...mockState.blocker,
			state: (typeof shouldBlock === "function" ? shouldBlock() : shouldBlock)
				? mockState.blocker.state
				: "unblocked",
		}),
	};
});

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (key === "policy_wizard_progress") {
				return `${options?.current}/${options?.total}`;
			}
			return key;
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: vi.fn(),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: ({
		confirmLabel,
		description,
		onConfirm,
		onOpenChange,
		open,
		title,
	}: {
		confirmLabel: string;
		description?: string;
		onConfirm: () => void;
		onOpenChange: (open: boolean) => void;
		open: boolean;
		title: string;
	}) =>
		open ? (
			<dialog open>
				<h2>{title}</h2>
				<p>{description}</p>
				<button type="button" onClick={onConfirm}>
					{confirmLabel}
				</button>
				<button type="button" onClick={() => onOpenChange(false)}>
					close-confirm
				</button>
			</dialog>
		) : null,
}));

vi.mock("@/components/admin/AdminOffsetPagination", () => ({
	AdminOffsetPagination: ({ total }: { total: number }) => (
		<div>{`pagination:${total}`}</div>
	),
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: ({
		description,
		title,
	}: {
		description: string;
		title: string;
	}) => (
		<div>
			<h2>{title}</h2>
			<p>{description}</p>
		</div>
	),
}));

vi.mock("@/components/common/SkeletonTable", () => ({
	SkeletonTable: () => <div data-testid="skeleton-table" />,
}));

vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/layout/AdminPageHeader", () => ({
	AdminPageHeader: ({
		actions,
		description,
		title,
	}: {
		actions?: React.ReactNode;
		description: string;
		title: string;
	}) => (
		<header>
			<h1>{title}</h1>
			<p>{description}</p>
			<div>{actions}</div>
		</header>
	),
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: React.ReactNode }) => (
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
		form,
		onClick,
		title,
		type,
	}: {
		"aria-label"?: string;
		children: React.ReactNode;
		disabled?: boolean;
		form?: string;
		onClick?: (event: React.MouseEvent<HTMLButtonElement>) => void;
		title?: string;
		type?: "button" | "submit";
	}) => (
		<button
			type={type ?? "button"}
			aria-label={ariaLabel}
			disabled={disabled}
			form={form}
			onClick={onClick}
			title={title}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div>{children}</div> : null,
	DialogContent: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	DialogFooter: ({ children }: { children: React.ReactNode }) => (
		<footer>{children}</footer>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: () => <span aria-hidden="true" />,
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
		checked: boolean;
		id?: string;
		onCheckedChange: (checked: boolean) => void;
	}) => (
		<input
			id={id}
			type="checkbox"
			checked={checked}
			onChange={(event) => onCheckedChange(event.target.checked)}
		/>
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/useConfirmDialog", () => ({
	useConfirmDialog: (handler: (id: number) => Promise<void>) => {
		const [confirmId, setConfirmId] = useState<number | null>(null);
		return {
			confirmId,
			dialogProps: {
				onConfirm: () => {
					if (confirmId !== null) {
						void handler(confirmId);
					}
				},
				open: confirmId !== null,
			},
			requestConfirm: (id: number) => setConfirmId(id),
		};
	},
}));

vi.mock("@/hooks/usePageTitle", () => ({
	usePageTitle: vi.fn(),
}));

vi.mock("@/lib/clipboard", () => ({
	writeTextToClipboard: (...args: unknown[]) =>
		mockState.writeTextToClipboard(...args),
}));

vi.mock("@/services/adminService", () => ({
	adminExternalAuthService: {
		create: (...args: unknown[]) => mockState.create(...args),
		delete: (...args: unknown[]) => mockState.deleteProvider(...args),
		get: (...args: unknown[]) => mockState.get(...args),
		list: (...args: unknown[]) => mockState.list(...args),
		listKinds: (...args: unknown[]) => mockState.listKinds(...args),
		test: (...args: unknown[]) => mockState.test(...args),
		testParams: (...args: unknown[]) => mockState.testParams(...args),
		update: (...args: unknown[]) => mockState.update(...args),
	},
}));

function AdminExternalAuthPage() {
	return (
		<Routes>
			<Route
				path="/admin/external-auth"
				element={<ActualAdminExternalAuthPage />}
			/>
			<Route
				path="/admin/external-auth/new"
				element={<ActualAdminExternalAuthPage variant="create" />}
			/>
			<Route
				path="/admin/external-auth/:providerId"
				element={
					<ActualAdminExternalAuthPage variant="detail" providerId={1} />
				}
			/>
		</Routes>
	);
}

function savedProvider(overrides: Record<string, unknown> = {}) {
	return {
		allowed_domains: [],
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

function providerKind(
	overrides: Partial<AdminExternalAuthProviderKindInfo> = {},
): AdminExternalAuthProviderKindInfo {
	const kind = overrides.kind ?? "oidc";
	const displayName = overrides.display_name ?? "OpenID Connect";
	const defaultScopes = overrides.default_scopes ?? "openid email profile";
	return {
		authorization_url_required: false,
		create_defaults: {
			auto_link_verified_email_enabled: false,
			auto_provision_enabled: false,
			display_name:
				kind === "oidc" || kind === "generic_oauth2" ? "" : displayName,
			enabled: true,
			options: kind === "microsoft" ? { microsoft: { tenant: "common" } } : {},
			require_email_verified: kind !== "microsoft" && kind !== "qq",
			scopes: defaultScopes,
		},
		default_scopes: defaultScopes,
		description: "OpenID Connect authorization-code sign-in.",
		display_name: displayName,
		issuer_url_supported: kind === "oidc" || kind === "generic_oauth2",
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

function escapeRegExp(value: string) {
	return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function clickProviderKindCard(name = "OpenID Connect") {
	fireEvent.click(
		screen.getByRole("button", {
			name: new RegExp(`^${escapeRegExp(name)}\\b`),
		}),
	);
	fireEvent.click(screen.getByRole("button", { name: "policy_wizard_next" }));
}

describe("AdminExternalAuthPage", () => {
	beforeEach(() => {
		mockState.create.mockReset();
		mockState.deleteProvider.mockReset();
		mockState.get.mockReset();
		mockState.handleApiError.mockReset();
		mockState.list.mockReset();
		mockState.listKinds.mockReset();
		mockState.test.mockReset();
		mockState.testParams.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.update.mockReset();
		mockState.writeTextToClipboard.mockReset();
		mockState.blocker.proceed.mockReset();
		mockState.blocker.reset.mockReset();
		mockState.blocker.state = "unblocked";

		mockState.writeTextToClipboard.mockResolvedValue(undefined);
		mockState.listKinds.mockResolvedValue([providerKind()]);
		mockState.list.mockResolvedValue({
			items: [],
			limit: 20,
			offset: 0,
			total: 0,
		});
		mockState.get.mockResolvedValue(savedProvider());
		mockState.create.mockResolvedValue({
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
			icon_url: "/static/external-auth/example.svg",
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
		});
		mockState.test.mockResolvedValue({
			authorization_endpoint: "https://idp.example.com/authorize",
			checks: [
				{ message: "JWKS contains 1 key(s)", name: "jwks", success: true },
			],
			issuer: "https://idp.example.com",
			jwks_key_count: 1,
			provider: "OpenID Connect",
			token_endpoint: "https://idp.example.com/token",
			userinfo_endpoint: null,
		});
		mockState.testParams.mockResolvedValue({
			authorization_endpoint: "https://idp.example.com/authorize",
			checks: [
				{ message: "JWKS contains 1 key(s)", name: "jwks", success: true },
			],
			issuer: "https://idp.example.com",
			jwks_key_count: 1,
			provider: "OpenID Connect",
			token_endpoint: "https://idp.example.com/token",
			userinfo_endpoint: null,
		});
	});

	it("creates a provider from the stepped form and opens its detail", async () => {
		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalled());
		const createButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(createButtons[createButtons.length - 1]);

		expect(screen.getAllByText("OpenID Connect").length).toBeGreaterThanOrEqual(
			2,
		);
		clickProviderKindCard();

		fireEvent.change(
			screen.getByLabelText("external_auth_provider_display_name"),
			{
				target: { value: "Example IDP" },
			},
		);
		fireEvent.change(screen.getByLabelText("external_auth_provider_icon_url"), {
			target: { value: " /static/external-auth/example.svg " },
		});
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_issuer_url"),
			{
				target: { value: "https://idp.example.com" },
			},
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_id"),
			{
				target: { value: "client-123" },
			},
		);
		expect(
			screen.queryByText("external_auth_provider_callback_url"),
		).not.toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		expect(mockState.create).not.toHaveBeenCalled();
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_allowed_domains"),
			{
				target: { value: "Example.COM, example.com" },
			},
		);
		const submitButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(submitButtons[submitButtons.length - 1]);

		await waitFor(() => expect(mockState.create).toHaveBeenCalledTimes(1));
		expect(mockState.create).toHaveBeenCalledWith(
			expect.objectContaining({
				allowed_domains: ["example.com"],
				client_id: "client-123",
				display_name: "Example IDP",
				enabled: true,
				icon_url: "/static/external-auth/example.svg",
				issuer_url: "https://idp.example.com",
				provider_kind: "oidc",
				scopes: "openid email profile",
			}),
		);
		expect(
			await screen.findByRole("heading", { name: "Example IDP", level: 1 }),
		).toBeInTheDocument();
		expect(
			screen.getAllByText(
				/\/api\/v1\/auth\/external-auth\/oidc\/example\/callback/,
			).length,
		).toBeGreaterThan(0);
		expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
	});

	it("refreshes the provider list and navigates back from provider details", async () => {
		const list = render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);
		await waitFor(() => expect(mockState.list).toHaveBeenCalled());
		fireEvent.click(screen.getByRole("button", { name: "core:refresh" }));
		await waitFor(() => expect(mockState.list).toHaveBeenCalledTimes(2));
		list.unmount();

		render(
			<MemoryRouter initialEntries={["/admin/external-auth/1"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);
		await screen.findByRole("heading", { name: "Example IDP" });
		fireEvent.click(
			screen.getByRole("button", { name: "external_auth_back_to_providers" }),
		);
		expect(
			await screen.findByRole("heading", { name: "external_auth" }),
		).toBeVisible();
	});

	it("keeps the create page open and reports provider creation failures", async () => {
		const createError = new Error("create provider failed");
		mockState.create.mockRejectedValueOnce(createError);
		render(
			<MemoryRouter initialEntries={["/admin/external-auth/new"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);
		await screen.findAllByText("OpenID Connect");
		clickProviderKindCard();
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_display_name"),
			{ target: { value: "Failed provider" } },
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_issuer_url"),
			{ target: { value: "https://idp.example.com" } },
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_id"),
			{ target: { value: "client-123" } },
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "external_auth_provider_create" }),
		);
		await waitFor(() =>
			expect(mockState.handleApiError).toHaveBeenCalledWith(createError),
		);
		expect(
			screen.getByRole("heading", { name: "external_auth_provider_create" }),
		).toBeVisible();
	});

	it("rebuilds the entire create draft when switching provider schemas", async () => {
		mockState.listKinds.mockResolvedValue([
			providerKind(),
			providerKind({
				authorization_url_required: true,
				description: "Generic OAuth2 authorization-code sign-in.",
				display_name: "Generic OAuth2",
				issuer_url_required: false,
				kind: "generic_oauth2",
				manual_endpoint_configuration_supported: true,
				protocol: "oauth2",
				supports_discovery: false,
				token_url_required: true,
				userinfo_url_required: true,
			}),
		]);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalled());
		const createButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(createButtons[createButtons.length - 1]);
		clickProviderKindCard("Generic OAuth2");

		fireEvent.change(
			screen.getByLabelText("external_auth_provider_display_name"),
			{ target: { value: "Custom OAuth Provider" } },
		);
		fireEvent.change(screen.getByLabelText("external_auth_provider_icon_url"), {
			target: { value: "/static/oauth.svg" },
		});
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_id"),
			{ target: { value: "oauth-client" } },
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_secret"),
			{ target: { value: "oauth-secret" } },
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_authorization_url"),
			{ target: { value: "https://oauth.example.com/authorize" } },
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_token_url"),
			{ target: { value: "https://oauth.example.com/token" } },
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_userinfo_url"),
			{ target: { value: "https://oauth.example.com/userinfo" } },
		);

		fireEvent.click(screen.getByRole("button", { name: "core:back" }));
		clickProviderKindCard("OpenID Connect");

		expect(
			screen.getByLabelText("external_auth_provider_display_name"),
		).toHaveValue("");
		expect(
			screen.getByLabelText("external_auth_provider_icon_url"),
		).toHaveValue("");
		expect(
			screen.getByLabelText("external_auth_provider_client_id"),
		).toHaveValue("");
		expect(
			screen.getByLabelText("external_auth_provider_client_secret"),
		).toHaveValue("");
		expect(
			screen.getByLabelText("external_auth_provider_issuer_url"),
		).toHaveValue("");
		expect(
			screen.queryByLabelText("external_auth_provider_authorization_url"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_token_url"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_userinfo_url"),
		).not.toBeInTheDocument();

		fireEvent.change(
			screen.getByLabelText("external_auth_provider_issuer_url"),
			{ target: { value: "https://oidc.example.com" } },
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_id"),
			{ target: { value: "oidc-client" } },
		);
		fireEvent.click(screen.getByRole("button", { name: "test_connection" }));

		await waitFor(() => expect(mockState.testParams).toHaveBeenCalledTimes(1));
		expect(mockState.testParams).toHaveBeenCalledWith(
			expect.objectContaining({
				authorization_url: null,
				provider_kind: "oidc",
				token_url: null,
				userinfo_url: null,
			}),
		);
	});

	it("preserves the create draft when reselecting the same provider schema", async () => {
		mockState.listKinds.mockResolvedValue([
			providerKind({
				authorization_url_required: true,
				description: "Generic OAuth2 authorization-code sign-in.",
				display_name: "Generic OAuth2",
				issuer_url_required: false,
				kind: "generic_oauth2",
				manual_endpoint_configuration_supported: true,
				protocol: "oauth2",
				supports_discovery: false,
				token_url_required: true,
				userinfo_url_required: true,
			}),
		]);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalled());
		const createButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(createButtons[createButtons.length - 1]);
		clickProviderKindCard("Generic OAuth2");
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_id"),
			{ target: { value: "preserved-client" } },
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_authorization_url"),
			{ target: { value: "https://oauth.example.com/authorize" } },
		);

		fireEvent.click(screen.getByRole("button", { name: "core:back" }));
		clickProviderKindCard("Generic OAuth2");

		expect(
			screen.getByLabelText("external_auth_provider_client_id"),
		).toHaveValue("preserved-client");
		expect(
			screen.getByLabelText("external_auth_provider_authorization_url"),
		).toHaveValue("https://oauth.example.com/authorize");
	});

	it("applies Google create-page defaults and hides manual OIDC fields", async () => {
		mockState.listKinds.mockResolvedValue([
			providerKind(),
			providerKind({
				default_scopes: "openid profile email",
				description: "Google OpenID Connect sign-in.",
				display_name: "Google",
				issuer_url_required: false,
				kind: "google",
			}),
		]);
		mockState.create.mockResolvedValue(
			savedProvider({
				display_name: "Google",
				issuer_url: null,
				key: "google",
				provider_kind: "google",
				scopes: "openid profile email",
			}),
		);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalled());
		const createButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(createButtons[createButtons.length - 1]);
		clickProviderKindCard("Google");

		expect(screen.getByDisplayValue("Google")).toHaveAttribute(
			"id",
			"external-auth-provider-display-name",
		);
		expect(
			screen.queryByLabelText("external_auth_provider_issuer_url"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_authorization_url"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByText("external_auth_provider_google_fixed_title"),
		).not.toBeInTheDocument();

		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_id"),
			{
				target: { value: "google-client" },
			},
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		expect(screen.getByText("openid profile email")).toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_google_claims_title"),
		).toBeInTheDocument();

		const submitButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(submitButtons[submitButtons.length - 1]);

		await waitFor(() => expect(mockState.create).toHaveBeenCalledTimes(1));
		expect(mockState.create).toHaveBeenCalledWith(
			expect.objectContaining({
				authorization_url: null,
				client_id: "google-client",
				display_name: "Google",
				issuer_url: null,
				provider_kind: "google",
				scopes: "openid profile email",
				token_url: null,
				userinfo_url: null,
			}),
		);
		expect(
			await screen.findByRole("heading", { name: "Example IDP", level: 1 }),
		).toBeInTheDocument();
	});

	it("applies Microsoft create-page defaults and derives issuer from tenant", async () => {
		mockState.listKinds.mockResolvedValue([
			providerKind(),
			providerKind({
				default_scopes: "openid profile email",
				description: "Microsoft OpenID Connect sign-in.",
				display_name: "Microsoft",
				issuer_url_required: false,
				kind: "microsoft",
				supports_email_verified_claim: false,
			}),
		]);
		mockState.create.mockResolvedValue(
			savedProvider({
				display_name: "Microsoft",
				issuer_url: "https://login.microsoftonline.com/organizations/v2.0",
				key: "microsoft",
				provider_kind: "microsoft",
				require_email_verified: false,
				scopes: "openid profile email",
			}),
		);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalled());
		const createButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(createButtons[createButtons.length - 1]);
		clickProviderKindCard("Microsoft");

		expect(screen.getByDisplayValue("Microsoft")).toHaveAttribute(
			"id",
			"external-auth-provider-display-name",
		);
		expect(
			screen.getByLabelText("external_auth_provider_microsoft_tenant"),
		).toHaveValue("common");
		expect(
			screen.queryByLabelText("external_auth_provider_issuer_url"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_authorization_url"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByText("external_auth_provider_microsoft_fixed_title"),
		).not.toBeInTheDocument();

		fireEvent.change(
			screen.getByLabelText("external_auth_provider_microsoft_tenant"),
			{
				target: { value: "organizations" },
			},
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_id"),
			{
				target: { value: "microsoft-client" },
			},
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		expect(screen.getByText("openid profile email")).toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_microsoft_claims_title"),
		).toBeInTheDocument();
		expect(
			screen.getByText("tenant: organizations · OIDC discovery"),
		).toBeInTheDocument();

		const submitButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(submitButtons[submitButtons.length - 1]);

		await waitFor(() => expect(mockState.create).toHaveBeenCalledTimes(1));
		expect(mockState.create).toHaveBeenCalledWith(
			expect.objectContaining({
				authorization_url: null,
				client_id: "microsoft-client",
				display_name: "Microsoft",
				issuer_url: null,
				options: {
					microsoft: {
						tenant: "organizations",
					},
				},
				provider_kind: "microsoft",
				require_email_verified: false,
				scopes: "openid profile email",
				token_url: null,
				userinfo_url: null,
			}),
		);
		expect(
			await screen.findByRole("heading", { name: "Example IDP", level: 1 }),
		).toBeInTheDocument();
	});

	it("applies QQ create-page defaults and submits only app credentials", async () => {
		mockState.listKinds.mockResolvedValue([
			providerKind(),
			providerKind({
				default_scopes: "get_user_info",
				description: "QQ Connect OAuth2 sign-in.",
				display_name: "QQ",
				issuer_url_required: false,
				kind: "qq",
				protocol: "oauth2",
				supports_discovery: false,
				supports_email_verified_claim: false,
			}),
		]);
		mockState.create.mockResolvedValue(
			savedProvider({
				display_name: "QQ",
				issuer_url: "qq:100000001",
				key: "qq",
				provider_kind: "qq",
				require_email_verified: false,
				scopes: "get_user_info",
			}),
		);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalled());
		const createButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(createButtons[createButtons.length - 1]);
		clickProviderKindCard("QQ");

		expect(screen.getByDisplayValue("QQ")).toHaveAttribute(
			"id",
			"external-auth-provider-display-name",
		);
		expect(
			screen.queryByLabelText("external_auth_provider_issuer_url"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_authorization_url"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("external_auth_provider_email_claim"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByText("external_auth_provider_qq_fixed_title"),
		).not.toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_qq_email_title"),
		).toBeInTheDocument();

		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_id"),
			{
				target: { value: "100000001" },
			},
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_secret"),
			{
				target: { value: "qq-app-key" },
			},
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		expect(screen.getByText("get_user_info")).toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_qq_claims_title"),
		).toBeInTheDocument();
		expect(
			screen.getByText(
				"subject=openid · display=nickname · email=not returned",
			),
		).toBeInTheDocument();

		const submitButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(submitButtons[submitButtons.length - 1]);

		await waitFor(() => expect(mockState.create).toHaveBeenCalledTimes(1));
		expect(mockState.create).toHaveBeenCalledWith(
			expect.objectContaining({
				authorization_url: null,
				client_id: "100000001",
				client_secret: "qq-app-key",
				display_name: "QQ",
				issuer_url: null,
				provider_kind: "qq",
				require_email_verified: false,
				scopes: "get_user_info",
				token_url: null,
				userinfo_url: null,
			}),
		);
		expect(
			await screen.findByRole("heading", { name: "Example IDP", level: 1 }),
		).toBeInTheDocument();
	});

	it("keeps creation on the type step when provider kinds are unavailable", async () => {
		const loadKindsError = new Error("provider kinds unavailable");
		mockState.listKinds
			.mockResolvedValueOnce([])
			.mockRejectedValueOnce(loadKindsError);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalledTimes(1));
		const createButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(createButtons[createButtons.length - 1]);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(loadKindsError);
		});

		expect(
			screen.getByRole("button", { name: "policy_wizard_next" }),
		).toBeDisabled();
		expect(
			screen.getAllByText("external_auth_provider_wizard_step_type_title")
				.length,
		).toBeGreaterThan(0);
		expect(
			screen.queryByLabelText("external_auth_provider_display_name"),
		).not.toBeInTheDocument();
	});

	it("applies provider kind defaults loaded after opening the create page", async () => {
		mockState.listKinds.mockResolvedValueOnce([]).mockResolvedValueOnce([
			providerKind({
				default_scopes: "get_user_info",
				description: "QQ Connect OAuth2 sign-in.",
				display_name: "QQ",
				issuer_url_required: false,
				kind: "qq",
				protocol: "oauth2",
				supports_discovery: false,
				supports_email_verified_claim: false,
			}),
		]);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalledTimes(1));
		const createButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(createButtons[createButtons.length - 1]);

		await screen.findAllByText("QQ");
		clickProviderKindCard("QQ");

		expect(
			screen.queryByLabelText("external_auth_provider_issuer_url"),
		).not.toBeInTheDocument();
		expect(
			screen.getByLabelText("external_auth_provider_client_id"),
		).toBeInTheDocument();
	});

	it("navigates straight to the create page without opening a provider dialog", async () => {
		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalledTimes(1));
		const createButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(createButtons[createButtons.length - 1]);
		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalledTimes(2));
		expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
			"external_auth_provider_create",
		);
		expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
	});

	it("confirms before leaving a changed provider creation page", async () => {
		render(
			<MemoryRouter initialEntries={["/admin/external-auth/new"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findAllByText("OpenID Connect");
		mockState.blocker.state = "blocked";
		clickProviderKindCard();
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_display_name"),
			{ target: { value: "Changed provider" } },
		);

		expect(
			await screen.findByText("external_auth_provider_discard_title"),
		).toBeInTheDocument();
		const beforeUnload = new Event("beforeunload", { cancelable: true });
		window.dispatchEvent(beforeUnload);
		expect(beforeUnload.defaultPrevented).toBe(true);
		fireEvent.click(screen.getByRole("button", { name: "close-confirm" }));
		await waitFor(() => expect(mockState.blocker.reset).toHaveBeenCalled());
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_display_name"),
			{ target: { value: "Changed provider again" } },
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: "external_auth_provider_discard_confirm",
			}),
		);
		expect(mockState.blocker.proceed).toHaveBeenCalledTimes(1);
	});

	it("shows a detail loading fallback and returns from a missing provider", async () => {
		mockState.get.mockRejectedValueOnce(new Error("missing provider"));
		render(
			<MemoryRouter initialEntries={["/admin/external-auth/1"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);
		expect(screen.getByText("core:loading")).toBeVisible();
		expect(
			await screen.findByText("external_auth_provider_not_found"),
		).toBeVisible();
		fireEvent.click(
			screen.getByRole("button", { name: "external_auth_back_to_providers" }),
		);
		expect(
			await screen.findByRole("heading", { name: "external_auth" }),
		).toBeVisible();
	});

	it("tests provider draft parameters while creating", async () => {
		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalled());
		const createButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(createButtons[createButtons.length - 1]);
		clickProviderKindCard();
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_issuer_url"),
			{
				target: { value: "https://idp.example.com" },
			},
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_id"),
			{
				target: { value: "client-123" },
			},
		);

		fireEvent.click(screen.getByRole("button", { name: "test_connection" }));

		await waitFor(() => expect(mockState.testParams).toHaveBeenCalledTimes(1));
		expect(mockState.testParams).toHaveBeenCalledWith({
			authorization_url: null,
			client_id: "client-123",
			client_secret: null,
			issuer_url: "https://idp.example.com",
			options: {},
			provider_kind: "oidc",
			scopes: "openid email profile",
			token_url: null,
			userinfo_url: null,
		});
		expect(mockState.test).not.toHaveBeenCalled();
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"external_auth_provider_test_success",
		);
	});

	it("tests the saved provider when edit connection fields are unchanged", async () => {
		mockState.list.mockResolvedValue({
			items: [savedProvider()],
			limit: 20,
			offset: 0,
			total: 1,
		});

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findByText("Example IDP");
		fireEvent.click(screen.getByText("Example IDP"));
		fireEvent.click(
			await screen.findByRole("button", { name: "test_connection" }),
		);

		await waitFor(() => expect(mockState.test).toHaveBeenCalledWith(1));
		expect(mockState.testParams).not.toHaveBeenCalled();
	});

	it("tests a provider from the providers list and shows the result summary", async () => {
		mockState.list.mockResolvedValue({
			items: [savedProvider()],
			limit: 20,
			offset: 0,
			total: 1,
		});

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findByText("Example IDP");
		fireEvent.click(
			screen.getByRole("button", { name: "external_auth_provider_test" }),
		);

		await waitFor(() => expect(mockState.test).toHaveBeenCalledWith(1));
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"external_auth_provider_test_success",
		);
		expect(
			await screen.findByText("external_auth_provider_test_check_ok"),
		).toBeInTheDocument();
	});

	it("hides derived issuer URL when editing a Microsoft provider", async () => {
		mockState.listKinds.mockResolvedValue([
			providerKind(),
			providerKind({
				default_scopes: "openid profile email",
				description: "Microsoft OpenID Connect sign-in.",
				display_name: "Microsoft",
				issuer_url_required: false,
				kind: "microsoft",
				supports_email_verified_claim: false,
			}),
		]);
		const microsoftProvider = savedProvider({
			display_name: "Microsoft",
			issuer_url: "https://login.microsoftonline.com/common/v2.0",
			key: "microsoft",
			provider_kind: "microsoft",
			require_email_verified: false,
			scopes: "openid profile email",
		});
		mockState.get.mockResolvedValue(microsoftProvider);
		mockState.list.mockResolvedValue({
			items: [microsoftProvider],
			limit: 20,
			offset: 0,
			total: 1,
		});

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		const microsoftLabels = await screen.findAllByText("Microsoft");
		fireEvent.click(microsoftLabels[0]);

		expect(
			await screen.findByLabelText("external_auth_provider_microsoft_tenant"),
		).toHaveValue("common");
		expect(
			screen.queryByLabelText("external_auth_provider_issuer_url"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByDisplayValue(
				"https://login.microsoftonline.com/common/v2.0",
			),
		).not.toBeInTheDocument();
	});

	it("tests draft parameters while editing when connection fields changed", async () => {
		mockState.list.mockResolvedValue({
			items: [savedProvider()],
			limit: 20,
			offset: 0,
			total: 1,
		});

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findByText("Example IDP");
		fireEvent.click(screen.getByText("Example IDP"));
		fireEvent.change(
			await screen.findByLabelText("external_auth_provider_issuer_url"),
			{
				target: { value: "https://changed.example.com" },
			},
		);
		fireEvent.click(screen.getByRole("button", { name: "test_connection" }));

		await waitFor(() => expect(mockState.testParams).toHaveBeenCalledTimes(1));
		expect(mockState.testParams).toHaveBeenCalledWith(
			expect.objectContaining({
				client_id: "client-123",
				issuer_url: "https://changed.example.com",
				provider_kind: "oidc",
			}),
		);
		expect(mockState.test).not.toHaveBeenCalled();
	});

	it("reports draft connection test failures while creating", async () => {
		const testError = new Error("draft test failed");
		mockState.testParams.mockRejectedValueOnce(testError);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => expect(mockState.listKinds).toHaveBeenCalled());
		const createButtons = screen.getAllByRole("button", {
			name: /external_auth_provider_create/,
		});
		fireEvent.click(createButtons[createButtons.length - 1]);
		clickProviderKindCard();
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_issuer_url"),
			{
				target: { value: "https://idp.example.com" },
			},
		);
		fireEvent.change(
			screen.getByLabelText("external_auth_provider_client_id"),
			{
				target: { value: "client-123" },
			},
		);

		fireEvent.click(screen.getByRole("button", { name: "test_connection" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(testError);
		});
	});

	it("shows one readable provider kind badge in the providers list", async () => {
		mockState.list.mockResolvedValue({
			items: [
				{
					allowed_domains: [],
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
					protocol: "oidc",
					provider_kind: "oidc",
					require_email_verified: true,
					scopes: "openid email profile",
					subject_claim: null,
					token_url: null,
					updated_at: "2026-05-17T10:00:00Z",
					userinfo_url: null,
					username_claim: null,
				},
			],
			limit: 20,
			offset: 0,
			total: 1,
		});

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findByText("Example IDP");

		expect(screen.queryByText("OIDC")).not.toBeInTheDocument();
		expect(screen.getByText("OpenID Connect")).toBeInTheDocument();
	});

	it("shows default claim guidance and all claim override entries while editing", async () => {
		mockState.list.mockResolvedValue({
			items: [
				{
					allowed_domains: [],
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
					icon_url: "https://cdn.example.com/idp.svg",
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
				},
			],
			limit: 20,
			offset: 0,
			total: 1,
		});

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findByText("Example IDP");
		fireEvent.click(screen.getByText("Example IDP"));

		expect(
			await screen.findByLabelText("external_auth_provider_subject_claim"),
		).toBeInTheDocument();
		expect(
			screen.getByLabelText("external_auth_provider_email_verified_claim"),
		).toBeInTheDocument();
		expect(
			screen.getByLabelText("external_auth_provider_avatar_url_claim"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("external_auth_provider_key_hint"),
		).not.toBeInTheDocument();
		expect(
			screen.getByText("external_auth_provider_allowed_domains_hint"),
		).toBeInTheDocument();
		expect(
			screen.getAllByText("external_auth_provider_claim_default_hint").length,
		).toBeGreaterThanOrEqual(7);
	});

	it("copies callback URLs from the provider list and reports clipboard failures", async () => {
		mockState.list.mockResolvedValue({
			items: [savedProvider()],
			limit: 20,
			offset: 0,
			total: 1,
		});

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findByText("Example IDP");
		fireEvent.click(
			screen.getByRole("button", {
				name: "external_auth_provider_copy_callback_url",
			}),
		);

		await waitFor(() => {
			expect(mockState.writeTextToClipboard).toHaveBeenCalledWith(
				expect.stringContaining(
					"/api/v1/auth/external-auth/oidc/example/callback",
				),
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"core:copied_to_clipboard",
		);

		mockState.writeTextToClipboard.mockRejectedValueOnce(
			new Error("clipboard denied"),
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: "external_auth_provider_copy_callback_url",
			}),
		);

		await waitFor(() => {
			expect(mockState.writeTextToClipboard).toHaveBeenCalledTimes(2);
		});
	});

	it("updates an existing provider and keeps the detail page current", async () => {
		mockState.list.mockResolvedValue({
			items: [savedProvider()],
			limit: 20,
			offset: 0,
			total: 1,
		});
		mockState.update.mockResolvedValue(
			savedProvider({
				display_name: "Updated IDP",
				icon_url: "/static/idp-updated.svg",
			}),
		);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findByText("Example IDP");
		fireEvent.click(screen.getByText("Example IDP"));
		fireEvent.change(
			await screen.findByLabelText("external_auth_provider_display_name"),
			{
				target: { value: "Updated IDP" },
			},
		);
		fireEvent.change(screen.getByLabelText("external_auth_provider_icon_url"), {
			target: { value: "/static/idp-updated.svg" },
		});
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.update).toHaveBeenCalledWith(
				1,
				expect.objectContaining({
					display_name: "Updated IDP",
					icon_url: "/static/idp-updated.svg",
				}),
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"external_auth_provider_updated",
		);
		expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
			"Updated IDP",
		);
	});

	it("keeps the detail page open and reports update failures", async () => {
		const updateError = new Error("update failed");
		mockState.list.mockResolvedValue({
			items: [savedProvider()],
			limit: 20,
			offset: 0,
			total: 1,
		});
		mockState.update.mockRejectedValueOnce(updateError);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findByText("Example IDP");
		fireEvent.click(screen.getByText("Example IDP"));
		fireEvent.change(
			await screen.findByLabelText("external_auth_provider_display_name"),
			{
				target: { value: "Updated IDP" },
			},
		);
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(updateError);
		});
		expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
			"Example IDP",
		);
	});

	it("handles test, update, delete, and list loading failures through the shared error handler", async () => {
		const loadError = new Error("load failed");
		mockState.listKinds.mockRejectedValueOnce(loadError);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(loadError);
		});
	});

	it("deletes providers and reloads the current page", async () => {
		mockState.list.mockResolvedValue({
			items: [savedProvider()],
			limit: 20,
			offset: 0,
			total: 1,
		});
		mockState.deleteProvider.mockResolvedValue(undefined);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findByText("Example IDP");
		fireEvent.click(
			screen.getByRole("button", { name: "external_auth_provider_delete" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "core:delete" }));

		await waitFor(() => {
			expect(mockState.deleteProvider).toHaveBeenCalledWith(1);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"external_auth_provider_deleted",
		);
		expect(mockState.list).toHaveBeenLastCalledWith({
			limit: 20,
			offset: 0,
		});
	});

	it("reports provider row test and delete failures", async () => {
		const testError = new Error("provider test failed");
		const deleteError = new Error("delete failed");
		mockState.list.mockResolvedValue({
			items: [savedProvider()],
			limit: 20,
			offset: 0,
			total: 1,
		});
		mockState.test.mockRejectedValueOnce(testError);
		mockState.deleteProvider.mockRejectedValueOnce(deleteError);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findByText("Example IDP");
		fireEvent.click(
			screen.getByRole("button", { name: "external_auth_provider_test" }),
		);
		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(testError);
		});

		fireEvent.click(
			screen.getByRole("button", { name: "external_auth_provider_delete" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "core:delete" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(deleteError);
		});
	});

	it("moves back a page after deleting the last provider on an offset page", async () => {
		mockState.list.mockResolvedValue({
			items: [savedProvider()],
			limit: 20,
			offset: 20,
			total: 21,
		});
		mockState.deleteProvider.mockResolvedValue(undefined);

		render(
			<MemoryRouter initialEntries={["/admin/external-auth?offset=20"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await screen.findByText("Example IDP");
		fireEvent.click(
			screen.getByRole("button", { name: "external_auth_provider_delete" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "core:delete" }));

		await waitFor(() => {
			expect(mockState.deleteProvider).toHaveBeenCalledWith(1);
		});
		await waitFor(() => {
			expect(mockState.list).toHaveBeenLastCalledWith({
				limit: 20,
				offset: 0,
			});
		});
	});

	it("moves an out-of-range URL offset back to the last populated page", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [],
				limit: 20,
				offset: 40,
				total: 21,
			})
			.mockResolvedValueOnce({
				items: [savedProvider()],
				limit: 20,
				offset: 20,
				total: 21,
			});

		render(
			<MemoryRouter initialEntries={["/admin/external-auth?offset=40"]}>
				<AdminExternalAuthPage />
			</MemoryRouter>,
		);

		await waitFor(() => {
			expect(mockState.list).toHaveBeenLastCalledWith({
				limit: 20,
				offset: 20,
			});
		});
		expect(await screen.findByText("Example IDP")).toBeInTheDocument();
	});
});
