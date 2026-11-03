import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ResetPasswordPage from "@/pages/ResetPasswordPage";

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
	confirmPasswordReset: vi.fn(),
	handleApiError: vi.fn(),
	location: {
		hash: "",
		pathname: "/reset-password",
		search: "",
	},
	navigate: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key.replace(/^core:/, ""),
	}),
}));

vi.mock("react-router-dom", () => ({
	useLocation: () => mockState.location,
	useNavigate: () => mockState.navigate,
}));

vi.mock("@/services/http", () => ({
	ApiError: MockApiError,
}));

vi.mock("@/types/api-helpers", () => ({
	ApiErrorCode: {
		ContactVerificationInvalid: "auth.contact_verification_invalid",
		ContactVerificationExpired: "auth.contact_verification_expired",
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/usePageTitle", () => ({
	usePageTitle: vi.fn(),
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
}));

vi.mock("@/services/authService", () => ({
	authService: {
		confirmPasswordReset: (...args: unknown[]) =>
			mockState.confirmPasswordReset(...args),
	},
}));

describe("ResetPasswordPage", () => {
	beforeEach(() => {
		mockState.confirmPasswordReset.mockReset();
		mockState.confirmPasswordReset.mockResolvedValue(undefined);
		mockState.handleApiError.mockReset();
		mockState.location = {
			hash: "",
			pathname: "/reset-password",
			search: "",
		};
		mockState.navigate.mockReset();
	});

	it("shows a missing-token state when no token is present", () => {
		render(<ResetPasswordPage />);

		expect(
			screen.getByText("reset_password_missing_token_title"),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "go_to_login" }),
		).toBeInTheDocument();
	});

	it("associates the password error and clears the description when corrected", () => {
		mockState.location.search = "?token=reset-token";
		render(<ResetPasswordPage />);
		const input = screen.getByLabelText("password");
		expect(input).not.toHaveAttribute("aria-describedby");
		fireEvent.change(input, { target: { value: "short" } });
		fireEvent.click(
			screen.getByRole("button", { name: "reset_password_submit" }),
		);
		expect(input).toHaveAccessibleDescription("invalid-password");
		fireEvent.change(input, { target: { value: "long-enough" } });
		expect(input).not.toHaveAttribute("aria-describedby");
	});

	it("submits a new password and redirects back to login", async () => {
		mockState.location = {
			hash: "",
			pathname: "/reset-password",
			search: "?token=reset-token",
		};

		render(<ResetPasswordPage />);

		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "newsecret456" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "reset_password_submit" }),
		);

		await waitFor(() => {
			expect(mockState.confirmPasswordReset).toHaveBeenCalledWith({
				token: "reset-token",
				new_password: "newsecret456",
			});
		});
		expect(mockState.navigate).toHaveBeenCalledWith(
			"/login?password_reset=success",
			{ replace: true },
		);
	});

	it("switches to an invalid state for used or bad tokens", async () => {
		mockState.location = {
			hash: "",
			pathname: "/reset-password",
			search: "?token=reset-token",
		};
		mockState.confirmPasswordReset.mockRejectedValueOnce(
			new MockApiError("auth.contact_verification_invalid", "invalid"),
		);

		render(<ResetPasswordPage />);

		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "newsecret456" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "reset_password_submit" }),
		);

		expect(
			await screen.findByText("reset_password_invalid_title"),
		).toBeInTheDocument();
	});

	it("switches to an expired state for expired tokens", async () => {
		mockState.location = {
			hash: "",
			pathname: "/reset-password",
			search: "?token=reset-token",
		};
		mockState.confirmPasswordReset.mockRejectedValueOnce(
			new MockApiError("auth.contact_verification_expired", "expired"),
		);

		render(<ResetPasswordPage />);

		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "newsecret456" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "reset_password_submit" }),
		);

		expect(
			await screen.findByText("reset_password_expired_title"),
		).toBeInTheDocument();
	});
});
