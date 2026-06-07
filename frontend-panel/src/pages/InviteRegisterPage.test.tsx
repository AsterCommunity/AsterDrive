import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import InviteRegisterPage from "@/pages/InviteRegisterPage";
import { createMeResponse } from "@/test/fixtures";
import type { MeResponse } from "@/types/api";

const MockApiError = vi.hoisted(
	() =>
		class MockApiError extends Error {
			code: string;
			constructor(code: string, message: string) {
				super(message);
				this.code = code;
			}
		},
);

const mockState = vi.hoisted(() => ({
	acceptInvitation: vi.fn(),
	authUser: null as MeResponse | null,
	handleApiError: vi.fn(),
	logout: vi.fn(),
	navigate: vi.fn(),
	params: {} as { token?: string },
	verifyInvitation: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, values?: Record<string, unknown>) =>
			values?.email ? `${key}:${values.email}` : key.replace(/^core:/, ""),
	}),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
	useParams: () => mockState.params,
}));

vi.mock("@/services/http", () => ({
	ApiError: MockApiError,
}));

vi.mock("@/types/api-helpers", () => ({
	ApiErrorCode: {
		AuthInvitationAccepted: "auth.invitation_accepted",
		AuthInvitationExpired: "auth.invitation_expired",
		AuthInvitationInvalid: "auth.invitation_invalid",
		AuthInvitationRevoked: "auth.invitation_revoked",
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/usePageTitle", () => ({
	usePageTitle: vi.fn(),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (
		selector: (state: {
			logout: typeof mockState.logout;
			user: MeResponse | null;
		}) => unknown,
	) =>
		selector({
			logout: mockState.logout,
			user: mockState.authUser,
		}),
}));

vi.mock("@/components/common/AsterDriveWordmark", () => ({
	AsterDriveWordmark: ({ alt }: { alt: string }) => <img alt={alt} />,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		type,
		disabled,
		onClick,
		className,
	}: {
		children: React.ReactNode;
		type?: "button" | "submit";
		disabled?: boolean;
		onClick?: () => void;
		className?: string;
	}) => (
		<button
			type={type ?? "button"}
			disabled={disabled}
			onClick={onClick}
			className={className}
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
		className,
	}: {
		children: React.ReactNode;
		htmlFor?: string;
		className?: string;
	}) => (
		<label htmlFor={htmlFor} className={className}>
			{children}
		</label>
	),
}));

vi.mock("@/lib/validation", () => ({
	passwordSchema: {
		safeParse: (value: string) =>
			value.length >= 8
				? { success: true }
				: {
						error: { issues: [{ message: "invalid-password" }] },
						success: false,
					},
	},
	usernameSchema: {
		safeParse: (value: string) =>
			value.trim().length >= 4 && value.trim().length <= 16
				? { success: true }
				: {
						error: { issues: [{ message: "invalid-username" }] },
						success: false,
					},
	},
}));

vi.mock("@/services/authService", () => ({
	authService: {
		acceptInvitation: (...args: unknown[]) =>
			mockState.acceptInvitation(...args),
		verifyInvitation: (...args: unknown[]) =>
			mockState.verifyInvitation(...args),
	},
}));

describe("InviteRegisterPage", () => {
	beforeEach(() => {
		mockState.acceptInvitation.mockReset();
		mockState.acceptInvitation.mockResolvedValue(undefined);
		mockState.authUser = null;
		mockState.handleApiError.mockReset();
		mockState.logout.mockReset();
		mockState.logout.mockResolvedValue(undefined);
		mockState.navigate.mockReset();
		mockState.params = {};
		mockState.verifyInvitation.mockReset();
		mockState.verifyInvitation.mockResolvedValue({
			email: "invitee@example.com",
			expires_at: "2026-06-10T00:00:00Z",
		});
	});

	it("shows a missing-token state when no token is present", () => {
		render(<InviteRegisterPage />);

		expect(screen.getByText("invitation_missing_title")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "go_to_login" }),
		).toBeInTheDocument();
		expect(mockState.verifyInvitation).not.toHaveBeenCalled();
	});

	it("verifies an invitation, accepts credentials, and redirects to login", async () => {
		mockState.params = { token: "invite-token" };

		render(<InviteRegisterPage />);

		expect(screen.getByText("invitation_page_title")).toBeInTheDocument();
		expect(
			await screen.findByText("invitation_register_title"),
		).toBeInTheDocument();
		expect(mockState.verifyInvitation).toHaveBeenCalledWith("invite-token");
		expect(screen.getByText("invitation_invited_account")).toBeInTheDocument();
		expect(screen.getByText("invitee@example.com")).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: "invitee" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "password123" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "invitation_register_submit" }),
		);

		await waitFor(() => {
			expect(mockState.acceptInvitation).toHaveBeenCalledWith("invite-token", {
				username: "invitee",
				password: "password123",
			});
		});
		expect(mockState.navigate).toHaveBeenCalledWith(
			"/login?invitation=accepted",
			{ replace: true },
		);
	});

	it("expands the content area while showing the unauthenticated registration form", async () => {
		let resolveVerify:
			| ((value: { email: string; expires_at: string }) => void)
			| undefined;
		mockState.params = { token: "invite-token" };
		mockState.verifyInvitation.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveVerify = resolve;
				}),
		);

		render(<InviteRegisterPage />);

		expect(screen.getByTestId("invite-content")).toHaveClass("h-32");
		expect(screen.getByText("invitation_loading_title")).toBeInTheDocument();

		resolveVerify?.({
			email: "invitee@example.com",
			expires_at: "2026-06-10T00:00:00Z",
		});

		await waitFor(() => {
			expect(screen.getByTestId("invite-content")).toHaveClass("h-[16rem]");
		});
		expect(screen.getByLabelText("username")).toBeInTheDocument();
		expect(screen.getByLabelText("password")).toBeInTheDocument();
		await waitFor(() => {
			expect(screen.getByTestId("invite-content")).toHaveClass("min-h-[16rem]");
		});
	});

	it("validates username and password before accepting", async () => {
		mockState.params = { token: "invite-token" };

		render(<InviteRegisterPage />);

		await screen.findByText("invitation_register_title");
		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: "abc" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "short" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "invitation_register_submit" }),
		);

		expect(await screen.findByText("invalid-username")).toBeInTheDocument();
		expect(screen.getByText("invalid-password")).toBeInTheDocument();
		expect(mockState.acceptInvitation).not.toHaveBeenCalled();
	});

	it("resets form values and validation errors when the invitation token changes", async () => {
		mockState.params = { token: "first-token" };
		mockState.verifyInvitation
			.mockResolvedValueOnce({
				email: "first@example.com",
				expires_at: "2026-06-10T00:00:00Z",
			})
			.mockResolvedValueOnce({
				email: "second@example.com",
				expires_at: "2026-06-11T00:00:00Z",
			});

		const { rerender } = render(<InviteRegisterPage />);

		await screen.findByText("first@example.com");
		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: "abc" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "short" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "invitation_register_submit" }),
		);
		expect(await screen.findByText("invalid-username")).toBeInTheDocument();
		expect(screen.getByText("invalid-password")).toBeInTheDocument();

		mockState.params = { token: "second-token" };
		rerender(<InviteRegisterPage />);

		expect(await screen.findByText("second@example.com")).toBeInTheDocument();
		expect(screen.getByLabelText("username")).toHaveValue("");
		expect(screen.getByLabelText("password")).toHaveValue("");
		expect(screen.queryByText("invalid-username")).not.toBeInTheDocument();
		expect(screen.queryByText("invalid-password")).not.toBeInTheDocument();
		expect(mockState.verifyInvitation).toHaveBeenCalledWith("second-token");
	});

	it("keeps the password visibility toggle keyboard focusable", async () => {
		mockState.params = { token: "invite-token" };

		render(<InviteRegisterPage />);

		await screen.findByText("invitation_register_title");
		expect(
			screen.getByRole("button", { name: "show_password" }),
		).not.toHaveAttribute("tabindex", "-1");
	});

	it.each([
		["auth.invitation_invalid", "invitation_invalid_title"],
		["auth.invitation_expired", "invitation_expired_title"],
		["auth.invitation_revoked", "invitation_revoked_title"],
		["auth.invitation_accepted", "invitation_accepted_title"],
	])("maps verify error %s to %s", async (code, title) => {
		mockState.params = { token: "invite-token" };
		mockState.verifyInvitation.mockRejectedValueOnce(
			new MockApiError(code, "invitation error"),
		);

		render(<InviteRegisterPage />);

		expect(await screen.findByText(title)).toBeInTheDocument();
	});

	it("maps accept invitation status errors after submit", async () => {
		mockState.params = { token: "invite-token" };
		mockState.acceptInvitation.mockRejectedValueOnce(
			new MockApiError("auth.invitation_revoked", "revoked"),
		);

		render(<InviteRegisterPage />);

		await screen.findByText("invitation_register_title");
		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: "invitee" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "password123" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "invitation_register_submit" }),
		);

		expect(
			await screen.findByText("invitation_revoked_title"),
		).toBeInTheDocument();
		expect(mockState.navigate).not.toHaveBeenCalled();
	});

	it("shows an already-signed-in state when the current account matches the invitation", async () => {
		mockState.params = { token: "invite-token" };
		mockState.authUser = createMeResponse({
			email: "INVITEE@example.com",
		});

		render(<InviteRegisterPage />);

		expect(
			await screen.findByText("invitation_same_account_title"),
		).toBeInTheDocument();
		expect(screen.getByText("invitation_invited_account")).toBeInTheDocument();
		expect(screen.getByText("invitee@example.com")).toBeInTheDocument();
		expect(screen.getByText("invitation_current_account")).toBeInTheDocument();
		expect(screen.getByText("INVITEE@example.com")).toBeInTheDocument();
		expect(screen.queryByLabelText("username")).not.toBeInTheDocument();
		expect(screen.queryByLabelText("password")).not.toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "invitation_go_to_current_account" }),
		);

		expect(mockState.navigate).toHaveBeenCalledWith("/");
		expect(mockState.logout).not.toHaveBeenCalled();
	});

	it("starts the invite transition immediately before logout finishes", async () => {
		let resolveLogout: (() => void) | undefined;
		mockState.params = { token: "invite-token" };
		mockState.authUser = createMeResponse({
			email: "current@example.com",
		});
		mockState.logout.mockImplementationOnce(
			() =>
				new Promise<void>((resolve) => {
					resolveLogout = resolve;
				}),
		);

		render(<InviteRegisterPage />);

		expect(
			await screen.findByText("invitation_account_mismatch_title"),
		).toBeInTheDocument();
		expect(screen.getByText("invitation_invited_account")).toBeInTheDocument();
		expect(screen.getByText("invitee@example.com")).toBeInTheDocument();
		expect(screen.getByText("invitation_current_account")).toBeInTheDocument();
		expect(screen.getByText("current@example.com")).toBeInTheDocument();
		expect(screen.getByTestId("invite-account-status")).toBeInTheDocument();
		expect(screen.queryByLabelText("username")).not.toBeInTheDocument();
		expect(screen.queryByLabelText("password")).not.toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "invitation_logout_and_continue" }),
		);

		expect(mockState.logout).toHaveBeenCalledTimes(1);
		expect(screen.getByTestId("invite-content")).toHaveClass("h-[16rem]");
		expect(screen.getByTestId("invite-content")).toHaveClass(
			"transition-[height]",
		);
		expect(
			screen.queryByTestId("invite-account-status"),
		).not.toBeInTheDocument();
		expect(screen.queryByLabelText("username")).not.toBeInTheDocument();
		expect(screen.queryByLabelText("password")).not.toBeInTheDocument();
		expect(
			await screen.findByText("invitation_switching_title"),
		).toBeInTheDocument();

		resolveLogout?.();

		await waitFor(() => {
			expect(screen.getByText("invitation_register_title")).toBeInTheDocument();
		});
		expect(screen.getByTestId("invite-content")).toHaveClass("min-h-[16rem]");
		expect(screen.getByTestId("invite-content")).toHaveClass(
			"overflow-visible",
		);
		expect(screen.getByLabelText("username")).toBeInTheDocument();
		expect(screen.getByLabelText("password")).toBeInTheDocument();
	});
});
