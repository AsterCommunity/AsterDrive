import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ForcePasswordChangePage from "@/pages/ForcePasswordChangePage";

const mockState = vi.hoisted(() => ({
	changePassword: vi.fn(),
	handleApiError: vi.fn(),
	isAuthenticated: true,
	isChecking: false,
	loggerWarn: vi.fn(),
	logout: vi.fn(),
	mustChangePassword: true,
	navigate: vi.fn(),
	refreshUser: vi.fn(),
	syncSession: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key.replace(/^core:/, "").replace(/^settings:/, ""),
	}),
}));

vi.mock("react-router-dom", () => ({
	Navigate: ({ to }: { to: string }) => <div data-testid="navigate">{to}</div>,
	useNavigate: () => mockState.navigate,
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/AsterDriveWordmark", () => ({
	AsterDriveWordmark: () => <div>AsterDrive</div>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		type,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
	}) => (
		<button type={type ?? "button"} disabled={disabled} onClick={onClick}>
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

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: (...args: unknown[]) => mockState.loggerWarn(...args),
	},
}));

vi.mock("@/hooks/usePageTitle", () => ({
	usePageTitle: vi.fn(),
}));

vi.mock("@/lib/validation", () => ({
	passwordChangeMatchSchema: {
		safeParse: ({
			confirmPassword,
			currentPassword,
			newPassword,
		}: {
			confirmPassword: string;
			currentPassword: string;
			newPassword: string;
		}) => {
			const issues = [];
			if (
				currentPassword.length > 0 &&
				newPassword.length > 0 &&
				currentPassword === newPassword
			) {
				issues.push({
					message: "validation:password_same_as_current",
					path: ["newPassword"],
				});
			}
			if (newPassword.length > 0 && confirmPassword !== newPassword) {
				issues.push({
					message: "validation:password_confirm_mismatch",
					path: ["confirmPassword"],
				});
			}
			return issues.length === 0
				? { success: true }
				: { success: false, error: { issues } };
		},
	},
	passwordChangeSchema: {
		safeParse: ({
			confirmPassword,
			currentPassword,
			newPassword,
		}: {
			confirmPassword: string;
			currentPassword: string;
			newPassword: string;
		}) => {
			const issues = [];
			if (currentPassword.length === 0) {
				issues.push({
					message: "current-required",
					path: ["currentPassword"],
				});
			}
			if (newPassword.length < 8) {
				issues.push({
					message: "password-short",
					path: ["newPassword"],
				});
			}
			if (
				currentPassword.length > 0 &&
				newPassword.length > 0 &&
				currentPassword === newPassword
			) {
				issues.push({
					message: "validation:password_same_as_current",
					path: ["newPassword"],
				});
			}
			if (newPassword.length > 0 && confirmPassword !== newPassword) {
				issues.push({
					message: "validation:password_confirm_mismatch",
					path: ["confirmPassword"],
				});
			}
			return issues.length === 0
				? { success: true }
				: { success: false, error: { issues } };
		},
	},
}));

vi.mock("@/services/authService", () => ({
	authService: {
		changePassword: (...args: unknown[]) => mockState.changePassword(...args),
	},
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (
		selector: (state: {
			isAuthenticated: boolean;
			isChecking: boolean;
			logout: typeof mockState.logout;
			refreshUser: typeof mockState.refreshUser;
			syncSession: typeof mockState.syncSession;
			user: { must_change_password: boolean } | null;
		}) => unknown,
	) =>
		selector({
			isAuthenticated: mockState.isAuthenticated,
			isChecking: mockState.isChecking,
			logout: mockState.logout,
			refreshUser: mockState.refreshUser,
			syncSession: mockState.syncSession,
			user: mockState.isAuthenticated
				? { must_change_password: mockState.mustChangePassword }
				: null,
		}),
}));

function fillPasswordForm({
	confirm = "newsecret456",
	current = "temporary123",
	next = "newsecret456",
} = {}) {
	fireEvent.change(screen.getByLabelText("settings_password_current"), {
		target: { value: current },
	});
	fireEvent.change(screen.getByLabelText("settings_password_new"), {
		target: { value: next },
	});
	fireEvent.change(screen.getByLabelText("settings_password_confirm"), {
		target: { value: confirm },
	});
}

describe("ForcePasswordChangePage", () => {
	beforeEach(() => {
		mockState.changePassword.mockReset();
		mockState.handleApiError.mockReset();
		mockState.isAuthenticated = true;
		mockState.isChecking = false;
		mockState.loggerWarn.mockReset();
		mockState.logout.mockReset();
		mockState.mustChangePassword = true;
		mockState.navigate.mockReset();
		mockState.refreshUser.mockReset();
		mockState.syncSession.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.changePassword.mockResolvedValue({ expiresIn: 900 });
		mockState.logout.mockResolvedValue(undefined);
		mockState.refreshUser.mockResolvedValue(undefined);
	});

	it("shows a loading state while auth is being checked", () => {
		mockState.isChecking = true;

		render(<ForcePasswordChangePage />);

		expect(screen.getByText("Spinner")).toBeInTheDocument();
		expect(
			screen.queryByText("force_password_change_title"),
		).not.toBeInTheDocument();
	});

	it("redirects unauthenticated users to login", () => {
		mockState.isAuthenticated = false;

		render(<ForcePasswordChangePage />);

		expect(screen.getByTestId("navigate")).toHaveTextContent("/login");
	});

	it("redirects users who no longer need a password change to the app", () => {
		mockState.mustChangePassword = false;

		render(<ForcePasswordChangePage />);

		expect(screen.getByTestId("navigate")).toHaveTextContent("/");
	});

	it("rejects empty, short, and mismatched password inputs before calling the API", () => {
		render(<ForcePasswordChangePage />);

		fireEvent.click(
			screen.getByRole("button", { name: /force_password_change_submit/ }),
		);

		expect(screen.getByText("current-required")).toBeInTheDocument();
		expect(screen.getByText("password-short")).toBeInTheDocument();
		expect(mockState.changePassword).not.toHaveBeenCalled();

		fillPasswordForm({ confirm: "different456" });
		fireEvent.click(
			screen.getByRole("button", { name: /force_password_change_submit/ }),
		);

		expect(
			screen.getByText("validation:password_confirm_mismatch"),
		).toBeInTheDocument();
		expect(mockState.changePassword).not.toHaveBeenCalled();
	});

	it("clears field errors while recomputing password match errors during editing", () => {
		render(<ForcePasswordChangePage />);

		fireEvent.click(
			screen.getByRole("button", { name: /force_password_change_submit/ }),
		);
		expect(screen.getByText("current-required")).toBeInTheDocument();
		expect(screen.getByText("password-short")).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("settings_password_current"), {
			target: { value: "temporary123" },
		});
		expect(screen.queryByText("current-required")).not.toBeInTheDocument();
		expect(screen.getByText("password-short")).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("settings_password_new"), {
			target: { value: "temporary123" },
		});
		expect(screen.queryByText("password-short")).not.toBeInTheDocument();
		expect(
			screen.getByText("validation:password_same_as_current"),
		).toBeInTheDocument();
		expect(
			screen.getByText("validation:password_confirm_mismatch"),
		).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("settings_password_confirm"), {
			target: { value: "temporary123" },
		});
		expect(
			screen.queryByText("validation:password_confirm_mismatch"),
		).not.toBeInTheDocument();
		expect(
			screen.getByText("validation:password_same_as_current"),
		).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("settings_password_current"), {
			target: { value: "oldtemporary123" },
		});
		expect(
			screen.queryByText("validation:password_same_as_current"),
		).not.toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("settings_password_new"), {
			target: { value: "newsecret456" },
		});
		expect(
			screen.getByText("validation:password_confirm_mismatch"),
		).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("settings_password_confirm"), {
			target: { value: "newsecret456" },
		});
		expect(
			screen.queryByText("validation:password_confirm_mismatch"),
		).not.toBeInTheDocument();
		expect(mockState.changePassword).not.toHaveBeenCalled();
	});

	it("toggles all password fields between masked and visible input types", () => {
		render(<ForcePasswordChangePage />);
		const currentInput = screen.getByLabelText("settings_password_current");
		const newInput = screen.getByLabelText("settings_password_new");
		const confirmInput = screen.getByLabelText("settings_password_confirm");

		expect(currentInput).toHaveAttribute("type", "password");
		expect(newInput).toHaveAttribute("type", "password");
		expect(confirmInput).toHaveAttribute("type", "password");

		fireEvent.click(screen.getByRole("button", { name: /show_password/ }));

		expect(currentInput).toHaveAttribute("type", "text");
		expect(newInput).toHaveAttribute("type", "text");
		expect(confirmInput).toHaveAttribute("type", "text");

		fireEvent.click(screen.getByRole("button", { name: /hide_password/ }));

		expect(currentInput).toHaveAttribute("type", "password");
		expect(newInput).toHaveAttribute("type", "password");
		expect(confirmInput).toHaveAttribute("type", "password");
	});

	it("detects reused current password while editing and before calling the API", () => {
		render(<ForcePasswordChangePage />);
		fillPasswordForm({
			confirm: "temporary123",
			current: "temporary123",
			next: "temporary123",
		});

		expect(
			screen.getByText("validation:password_same_as_current"),
		).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("settings_password_new"), {
			target: { value: "newsecret456" },
		});
		expect(
			screen.queryByText("validation:password_same_as_current"),
		).not.toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("settings_password_new"), {
			target: { value: "temporary123" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: /force_password_change_submit/ }),
		);

		expect(
			screen.getByText("validation:password_same_as_current"),
		).toBeInTheDocument();
		expect(mockState.changePassword).not.toHaveBeenCalled();
	});

	it("changes the password, refreshes the user, and enters the app", async () => {
		render(<ForcePasswordChangePage />);
		fillPasswordForm();

		fireEvent.click(
			screen.getByRole("button", { name: /force_password_change_submit/ }),
		);

		await waitFor(() => {
			expect(mockState.changePassword).toHaveBeenCalledWith({
				current_password: "temporary123",
				new_password: "newsecret456",
			});
		});
		expect(mockState.syncSession).toHaveBeenCalledWith(900);
		expect(mockState.refreshUser).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"force_password_change_success",
		);
		expect(mockState.navigate).toHaveBeenCalledWith("/", { replace: true });
	});

	it("enters the app when the post-change user refresh fails", async () => {
		const error = new Error("refresh failed");
		mockState.refreshUser.mockRejectedValueOnce(error);
		render(<ForcePasswordChangePage />);
		fillPasswordForm();

		fireEvent.click(
			screen.getByRole("button", { name: /force_password_change_submit/ }),
		);

		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith("/", { replace: true });
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"force_password_change_success",
		);
		await waitFor(() => {
			expect(mockState.loggerWarn).toHaveBeenCalledWith(
				"refreshUser after password change failed",
				error,
			);
		});
		expect(mockState.handleApiError).not.toHaveBeenCalledWith(error);
	});

	it("reports API failures without clearing the form or navigating", async () => {
		const error = new Error("current password rejected");
		mockState.changePassword.mockRejectedValueOnce(error);
		render(<ForcePasswordChangePage />);
		fillPasswordForm();

		fireEvent.click(
			screen.getByRole("button", { name: /force_password_change_submit/ }),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(mockState.refreshUser).not.toHaveBeenCalled();
		expect(mockState.navigate).not.toHaveBeenCalled();
		expect(screen.getByLabelText("settings_password_current")).toHaveValue(
			"temporary123",
		);
	});

	it("allows signing out instead of changing the password", async () => {
		render(<ForcePasswordChangePage />);

		fireEvent.click(screen.getByRole("button", { name: /logout/ }));

		await waitFor(() => {
			expect(mockState.logout).toHaveBeenCalledTimes(1);
		});
		expect(mockState.navigate).toHaveBeenCalledWith("/login", {
			replace: true,
		});
		expect(mockState.changePassword).not.toHaveBeenCalled();
	});

	it("reports logout failures without navigating", async () => {
		const error = new Error("logout failed");
		mockState.logout.mockRejectedValueOnce(error);
		render(<ForcePasswordChangePage />);

		fireEvent.click(screen.getByRole("button", { name: /logout/ }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(mockState.navigate).not.toHaveBeenCalledWith("/login", {
			replace: true,
		});
	});
});
