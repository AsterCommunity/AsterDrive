import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminExternalAuthPage from "@/pages/admin/AdminExternalAuthPage";

const mockState = vi.hoisted(() => ({
	create: vi.fn(),
	deleteProvider: vi.fn(),
	handleApiError: vi.fn(),
	list: vi.fn(),
	listKinds: vi.fn(),
	test: vi.fn(),
	testParams: vi.fn(),
	toastSuccess: vi.fn(),
	update: vi.fn(),
}));

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
	ConfirmDialog: () => null,
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

vi.mock("@/components/layout/AdminSurface", () => ({
	AdminSurface: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <section className={className}>{children}</section>,
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
		onClick?: (event: React.MouseEvent<HTMLButtonElement>) => void;
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
	writeTextToClipboard: vi.fn(),
}));

vi.mock("@/services/adminService", () => ({
	adminExternalAuthService: {
		create: (...args: unknown[]) => mockState.create(...args),
		delete: (...args: unknown[]) => mockState.deleteProvider(...args),
		list: (...args: unknown[]) => mockState.list(...args),
		listKinds: (...args: unknown[]) => mockState.listKinds(...args),
		test: (...args: unknown[]) => mockState.test(...args),
		testParams: (...args: unknown[]) => mockState.testParams(...args),
		update: (...args: unknown[]) => mockState.update(...args),
	},
}));

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

describe("AdminExternalAuthPage", () => {
	beforeEach(() => {
		mockState.create.mockReset();
		mockState.deleteProvider.mockReset();
		mockState.handleApiError.mockReset();
		mockState.list.mockReset();
		mockState.listKinds.mockReset();
		mockState.test.mockReset();
		mockState.testParams.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.update.mockReset();

		mockState.listKinds.mockResolvedValue([
			{
				authorization_url_required: false,
				default_scopes: "openid email profile",
				description: "OpenID Connect authorization-code sign-in.",
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
			},
		]);
		mockState.list.mockResolvedValue({
			items: [],
			limit: 20,
			offset: 0,
			total: 0,
		});
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

	it("creates a provider from the SSO type wizard with provider_kind", async () => {
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

		expect(screen.getByText("OpenID Connect")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "policy_wizard_next" }));

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
			await screen.findByText("external_auth_provider_created_callback_title"),
		).toBeInTheDocument();
		expect(
			screen.getByText(
				/\/api\/v1\/auth\/external-auth\/oidc\/example\/callback/,
			),
		).toBeInTheDocument();
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
		fireEvent.click(screen.getByRole("button", { name: "policy_wizard_next" }));
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
		fireEvent.click(screen.getByRole("button", { name: "test_connection" }));

		await waitFor(() => expect(mockState.test).toHaveBeenCalledWith(1));
		expect(mockState.testParams).not.toHaveBeenCalled();
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
			screen.getByLabelText("external_auth_provider_issuer_url"),
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
			screen.getByLabelText("external_auth_provider_subject_claim"),
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
});
