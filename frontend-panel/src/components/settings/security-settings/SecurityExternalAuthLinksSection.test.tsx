import {
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SecurityExternalAuthLinksSection } from "@/components/settings/security-settings/SecurityExternalAuthLinksSection";
import type { ExternalAuthLinkInfo } from "@/types/api";

const mockState = vi.hoisted(() => ({
	authService: {
		deleteExternalAuthLink: vi.fn(),
		listExternalAuthLinks: vi.fn(),
	},
	handleApiError: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/AnimatedCollapsible", () => ({
	AnimatedCollapsible: ({
		children,
		open,
	}: {
		children: React.ReactNode;
		open: boolean;
	}) => (open ? <div>{children}</div> : null),
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: ({
		confirmLabel,
		description,
		onConfirm,
		open,
		title,
	}: {
		confirmLabel: string;
		description?: string;
		onConfirm: () => void;
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
			</dialog>
		) : null,
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		type,
		...props
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
	}) => (
		<button
			{...props}
			type={type ?? "button"}
			disabled={disabled}
			onClick={onClick}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/format", () => ({
	formatDateAbsolute: (value: string) => `date:${value}`,
	formatDateAbsoluteWithOffset: (value: string) => `offset:${value}`,
}));

vi.mock("@/services/authService", () => ({
	authService: {
		deleteExternalAuthLink: (...args: unknown[]) =>
			mockState.authService.deleteExternalAuthLink(...args),
		listExternalAuthLinks: (...args: unknown[]) =>
			mockState.authService.listExternalAuthLinks(...args),
	},
}));

function link(
	overrides: Partial<ExternalAuthLinkInfo> = {},
): ExternalAuthLinkInfo {
	return {
		created_at: "2026-05-01T08:00:00Z",
		display_name_snapshot: "Example User",
		email_snapshot: "user@example.com",
		id: 1,
		issuer: "https://idp.example.com",
		last_login_at: null,
		provider_display_name: "Example IDP",
		provider_icon_url: "/static/external-auth/example.svg",
		provider_key: "example",
		provider_kind: "oidc",
		subject: "subject-1234567890abcdef",
		updated_at: "2026-05-01T08:00:00Z",
		...overrides,
	};
}

function imageBySrc(src: string) {
	return (
		Array.from(document.querySelectorAll("img")).find(
			(image) => image.getAttribute("src") === src,
		) ?? null
	);
}

describe("SecurityExternalAuthLinksSection", () => {
	beforeEach(() => {
		mockState.authService.deleteExternalAuthLink.mockReset();
		mockState.authService.deleteExternalAuthLink.mockResolvedValue(undefined);
		mockState.authService.listExternalAuthLinks.mockReset();
		mockState.authService.listExternalAuthLinks.mockResolvedValue([]);
		mockState.handleApiError.mockReset();
		mockState.toastSuccess.mockReset();
	});

	it("shows loading, then renders the empty state and refresh errors through the shared handler", async () => {
		let resolveInitial: ((links: ExternalAuthLinkInfo[]) => void) | undefined;
		mockState.authService.listExternalAuthLinks.mockImplementationOnce(
			() =>
				new Promise<ExternalAuthLinkInfo[]>((resolve) => {
					resolveInitial = resolve;
				}),
		);

		render(<SecurityExternalAuthLinksSection />);

		expect(screen.getByText("core:loading")).toBeInTheDocument();
		resolveInitial?.([]);
		expect(
			await screen.findByText("settings:settings_external_auth_links_empty"),
		).toBeInTheDocument();

		const error = new Error("refresh failed");
		mockState.authService.listExternalAuthLinks.mockRejectedValueOnce(error);
		fireEvent.click(screen.getByRole("button", { name: /core:refresh/ }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(
			mockState.authService.listExternalAuthLinks,
		).toHaveBeenLastCalledWith({ force: true });
	});

	it("renders linked providers, toggles details, and removes a link after confirmation", async () => {
		mockState.authService.listExternalAuthLinks.mockResolvedValue([
			link({
				id: 1,
				last_login_at: "2026-05-02T09:30:00Z",
				subject: "very-long-subject-1234567890abcdef",
			}),
			link({
				display_name_snapshot: "   ",
				email_snapshot: null,
				id: 2,
				provider_display_name: "Fallback IDP",
				provider_key: "fallback",
				subject: "short-subject",
			}),
		]);

		render(<SecurityExternalAuthLinksSection />);

		expect(await screen.findByText("Example IDP")).toBeInTheDocument();
		expect(screen.getByText("Fallback IDP")).toBeInTheDocument();
		expect(screen.getByText("Example User")).toBeInTheDocument();
		expect(screen.getByText("fallback")).toBeInTheDocument();
		expect(screen.getByText("very-long-...90abcdef")).toBeInTheDocument();
		expect(screen.getByText("short-subject")).toBeInTheDocument();

		fireEvent.click(
			screen.getAllByRole("button", {
				name: "settings:settings_security_show_details",
			})[0],
		);

		expect(screen.getByText("https://idp.example.com")).toBeInTheDocument();
		expect(screen.getByText("date:2026-05-01T08:00:00Z")).toBeInTheDocument();
		expect(screen.getByText("date:2026-05-02T09:30:00Z")).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_security_hide_details",
			}),
		);

		expect(
			screen.queryByText("https://idp.example.com"),
		).not.toBeInTheDocument();

		const fallbackCard = screen
			.getByText("Fallback IDP")
			.closest(".rounded-xl");
		if (!fallbackCard) {
			throw new Error("fallback link card not found");
		}
		fireEvent.click(
			within(fallbackCard).getByRole("button", {
				name: /settings:settings_external_auth_links_delete/,
			}),
		);
		const dialog = screen.getByRole("dialog");
		fireEvent.click(
			within(dialog).getByRole("button", {
				name: "settings:settings_external_auth_links_delete",
			}),
		);

		await waitFor(() => {
			expect(mockState.authService.deleteExternalAuthLink).toHaveBeenCalledWith(
				2,
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"settings:settings_external_auth_links_deleted",
		);
		expect(screen.queryByText("Fallback IDP")).not.toBeInTheDocument();
	});

	it("keeps the link visible and reports delete failures", async () => {
		const error = new Error("delete failed");
		mockState.authService.listExternalAuthLinks.mockResolvedValue([link()]);
		mockState.authService.deleteExternalAuthLink.mockRejectedValueOnce(error);

		render(<SecurityExternalAuthLinksSection />);

		await screen.findByText("Example IDP");
		fireEvent.click(
			screen.getByRole("button", {
				name: /settings:settings_external_auth_links_delete/,
			}),
		);
		fireEvent.click(
			within(screen.getByRole("dialog")).getByRole("button", {
				name: "settings:settings_external_auth_links_delete",
			}),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(screen.getByText("Example IDP")).toBeInTheDocument();
	});

	it("uses the configured provider icon before kind defaults", async () => {
		mockState.authService.listExternalAuthLinks.mockResolvedValue([
			link({
				provider_icon_url: "https://cdn.example.com/idp.svg",
				provider_kind: "github",
			}),
		]);

		render(<SecurityExternalAuthLinksSection />);

		await screen.findByText("Example IDP");
		expect(imageBySrc("https://cdn.example.com/idp.svg")).toHaveAttribute(
			"src",
			"https://cdn.example.com/idp.svg",
		);
		expect(imageBySrc("/static/external-auth/github-logo.svg")).toBeNull();
	});

	it("falls back to the provider kind icon when the configured icon is absent or invalid", async () => {
		mockState.authService.listExternalAuthLinks.mockResolvedValue([
			link({
				id: 1,
				provider_display_name: "GitHub",
				provider_icon_url: null,
				provider_kind: "github",
			}),
			link({
				id: 2,
				provider_display_name: "Invalid Icon IDP",
				provider_icon_url: "javascript:alert(1)",
				provider_kind: "oidc",
			}),
		]);

		render(<SecurityExternalAuthLinksSection />);

		await screen.findByText("GitHub");
		expect(screen.getByText("Invalid Icon IDP")).toBeInTheDocument();
		expect(imageBySrc("/static/external-auth/github-logo.svg")).toHaveAttribute(
			"src",
			"/static/external-auth/github-logo.svg",
		);
		expect(
			imageBySrc("/static/external-auth/openid-seeklogo.svg"),
		).toHaveAttribute("src", "/static/external-auth/openid-seeklogo.svg");
		expect(imageBySrc("javascript:alert(1)")).toBeNull();
	});

	it("uses the QQ provider kind icon when no configured icon is present", async () => {
		mockState.authService.listExternalAuthLinks.mockResolvedValue([
			link({
				provider_display_name: "QQ",
				provider_icon_url: null,
				provider_kind: "qq",
			}),
		]);

		render(<SecurityExternalAuthLinksSection />);

		await screen.findByText("QQ");
		expect(imageBySrc("/static/external-auth/qq-logo.svg")).toHaveAttribute(
			"src",
			"/static/external-auth/qq-logo.svg",
		);
	});

	it("falls back to the provider kind icon when the configured icon contains whitespace", async () => {
		mockState.authService.listExternalAuthLinks.mockResolvedValue([
			link({
				provider_icon_url: "/static/external-auth/custom icon.svg",
				provider_kind: "generic_oauth2",
			}),
		]);

		render(<SecurityExternalAuthLinksSection />);

		await screen.findByText("Example IDP");
		expect(imageBySrc("/static/external-auth/oauth-logo.svg")).toHaveAttribute(
			"src",
			"/static/external-auth/oauth-logo.svg",
		);
		expect(imageBySrc("/static/external-auth/custom icon.svg")).toBeNull();
	});

	it("falls back from a broken configured icon to the provider kind icon", async () => {
		mockState.authService.listExternalAuthLinks.mockResolvedValue([
			link({
				provider_icon_url: "/broken-provider-icon.svg",
				provider_kind: "github",
			}),
		]);

		render(<SecurityExternalAuthLinksSection />);

		await screen.findByText("Example IDP");
		const icon = imageBySrc("/broken-provider-icon.svg");
		expect(icon).toHaveAttribute("src", "/broken-provider-icon.svg");

		if (!icon) {
			throw new Error("configured provider icon not found");
		}
		fireEvent.error(icon);

		await waitFor(() => {
			expect(
				imageBySrc("/static/external-auth/github-logo.svg"),
			).toHaveAttribute("src", "/static/external-auth/github-logo.svg");
		});
	});

	it("falls back to the generic icon when the provider kind icon also fails to load", async () => {
		mockState.authService.listExternalAuthLinks.mockResolvedValue([
			link({
				provider_icon_url: null,
				provider_kind: "github",
			}),
		]);

		render(<SecurityExternalAuthLinksSection />);

		await screen.findByText("Example IDP");
		const icon = imageBySrc("/static/external-auth/github-logo.svg");
		expect(icon).toHaveAttribute(
			"src",
			"/static/external-auth/github-logo.svg",
		);

		if (!icon) {
			throw new Error("provider kind icon not found");
		}
		fireEvent.error(icon);

		await waitFor(() => {
			expect(screen.getByText("Globe")).toBeInTheDocument();
		});
		expect(imageBySrc("/static/external-auth/github-logo.svg")).toBeNull();
	});
});
