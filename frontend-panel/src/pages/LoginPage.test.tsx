import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import LoginPage from "@/pages/LoginPage";
import { useThemeStore } from "@/stores/themeStore";

const MockApiError = vi.hoisted(
	() =>
		class MockApiError extends Error {
			code: number;
			constructor(code: number, message: string) {
				super(message);
				this.code = code;
			}
		},
);

const mockState = vi.hoisted(() => ({
	check: vi.fn(),
	handleApiError: vi.fn(),
	allowUserRegistration: true,
	conditionalPasskeySupported: false,
	finishPasskeyLogin: vi.fn(),
	getPasskeyCredential: vi.fn(),
	login: vi.fn(),
	location: {
		hash: "",
		pathname: "/login",
		search: "",
	},
	navigate: vi.fn(),
	refreshUser: vi.fn(),
	register: vi.fn(),
	requestPasswordReset: vi.fn(),
	resendRegisterActivation: vi.fn(),
	setup: vi.fn(),
	startPasskeyLogin: vi.fn(),
	syncSession: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
	webAuthnSupported: false,
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
	ErrorCode: {
		PendingActivation: 2004,
	},
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
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

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/validation", () => ({
	emailSchema: {
		safeParse: (value: string) =>
			/^[^@]+@[^@]+\.[^@]+$/.test(value)
				? { success: true }
				: {
						error: { issues: [{ message: "invalid-email" }] },
						success: false,
					},
	},
	existingPasswordSchema: {
		safeParse: (value: string) =>
			value.length > 0
				? { success: true }
				: {
						error: { issues: [{ message: "password-required" }] },
						success: false,
					},
	},
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
			/^[a-zA-Z0-9_-]{4,16}$/.test(value)
				? { success: true }
				: {
						error: { issues: [{ message: "invalid-username" }] },
						success: false,
					},
	},
}));

vi.mock("@/services/authService", () => ({
	authService: {
		check: (...args: unknown[]) => mockState.check(...args),
		finishPasskeyLogin: (...args: unknown[]) =>
			mockState.finishPasskeyLogin(...args),
		requestPasswordReset: (...args: unknown[]) =>
			mockState.requestPasswordReset(...args),
		register: (...args: unknown[]) => mockState.register(...args),
		resendRegisterActivation: (...args: unknown[]) =>
			mockState.resendRegisterActivation(...args),
		setup: (...args: unknown[]) => mockState.setup(...args),
		startPasskeyLogin: (...args: unknown[]) =>
			mockState.startPasskeyLogin(...args),
	},
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (
		selector: (state: {
			login: typeof mockState.login;
			refreshUser: typeof mockState.refreshUser;
			syncSession: typeof mockState.syncSession;
		}) => unknown,
	) =>
		selector({
			login: mockState.login,
			refreshUser: mockState.refreshUser,
			syncSession: mockState.syncSession,
		}),
}));

vi.mock("@/lib/webauthn", () => ({
	getPasskeyCredential: (...args: unknown[]) =>
		mockState.getPasskeyCredential(...args),
	isConditionalPasskeyLoginAvailable: () =>
		Promise.resolve(mockState.conditionalPasskeySupported),
	isWebAuthnSupported: () => mockState.webAuthnSupported,
	WebAuthnCancelledError: class WebAuthnCancelledError extends Error {},
	WebAuthnUnsupportedError: class WebAuthnUnsupportedError extends Error {},
}));

vi.mock("@/stores/brandingStore", () => ({
	useBrandingStore: (
		selector: (state: {
			allowUserRegistration: boolean;
			branding: {
				title: string;
				wordmarkDarkUrl: string;
				wordmarkLightUrl: string;
			};
		}) => unknown,
	) =>
		selector({
			allowUserRegistration: mockState.allowUserRegistration,
			branding: {
				title: "AsterDrive",
				wordmarkDarkUrl: "/static/asterdrive/asterdrive-dark.svg",
				wordmarkLightUrl: "/static/asterdrive/asterdrive-light.svg",
			},
		}),
}));

describe("LoginPage", () => {
	beforeEach(() => {
		document.documentElement.classList.remove("dark");
		mockState.allowUserRegistration = true;
		mockState.conditionalPasskeySupported = false;
		mockState.check.mockReset();
		mockState.finishPasskeyLogin.mockReset();
		mockState.getPasskeyCredential.mockReset();
		mockState.handleApiError.mockReset();
		mockState.login.mockReset();
		mockState.location = {
			hash: "",
			pathname: "/login",
			search: "",
		};
		mockState.navigate.mockReset();
		mockState.refreshUser.mockReset();
		mockState.register.mockReset();
		mockState.requestPasswordReset.mockReset();
		mockState.resendRegisterActivation.mockReset();
		mockState.setup.mockReset();
		mockState.startPasskeyLogin.mockReset();
		mockState.syncSession.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.webAuthnSupported = false;
		mockState.finishPasskeyLogin.mockResolvedValue({ expiresIn: 900 });
		mockState.getPasskeyCredential.mockResolvedValue({ id: "credential-1" });
		mockState.login.mockResolvedValue(undefined);
		mockState.refreshUser.mockResolvedValue(undefined);
		mockState.register.mockResolvedValue({ email_verified: false });
		mockState.requestPasswordReset.mockResolvedValue(undefined);
		mockState.resendRegisterActivation.mockResolvedValue(undefined);
		mockState.setup.mockResolvedValue(undefined);
		mockState.startPasskeyLogin.mockResolvedValue({
			flow_id: "flow-1",
			public_key: { publicKey: { challenge: "AQID" } },
		});
		mockState.syncSession.mockReturnValue(undefined);
		mockState.check.mockResolvedValue({
			has_users: true,
			allow_user_registration: true,
		});
		useThemeStore.setState({ resolvedTheme: "light" });
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("loads public auth state and signs users in from the default login mode", async () => {
		render(<LoginPage />);

		await waitFor(() => {
			expect(mockState.check).toHaveBeenCalledWith();
		});
		expect(
			await screen.findByRole("button", { name: "sign_in" }),
		).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "user@example.com" },
		});

		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret7" },
		});
		fireEvent.click(screen.getByRole("button", { name: "sign_in" }));

		await waitFor(() => {
			expect(mockState.login).toHaveBeenCalledWith(
				"user@example.com",
				"secret7",
			);
		});
		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith("/", { replace: true });
		});
	});

	it("preserves caret position when editing login fields in the middle", async () => {
		render(<LoginPage />);

		const identifierInput = (await screen.findByLabelText(
			"email_or_username",
		)) as HTMLInputElement;
		fireEvent.change(identifierInput, {
			target: { value: "esap" },
		});
		identifierInput.focus();
		identifierInput.setSelectionRange(2, 2);
		fireEvent.change(identifierInput, {
			target: { selectionEnd: 3, selectionStart: 3, value: "esXap" },
		});

		await waitFor(() => {
			expect(identifierInput).toHaveValue("esXap");
			expect(identifierInput.selectionStart).toBe(3);
			expect(identifierInput.selectionEnd).toBe(3);
		});

		const passwordInput = screen.getByLabelText("password") as HTMLInputElement;
		fireEvent.change(passwordInput, {
			target: { value: "secret" },
		});
		passwordInput.focus();
		passwordInput.setSelectionRange(3, 3);
		fireEvent.change(passwordInput, {
			target: { selectionEnd: 4, selectionStart: 4, value: "secXret" },
		});

		await waitFor(() => {
			expect(passwordInput).toHaveValue("secXret");
			expect(passwordInput.selectionStart).toBe(4);
			expect(passwordInput.selectionEnd).toBe(4);
		});
	});

	it("shows the passkey fallback when WebAuthn is unavailable", async () => {
		render(<LoginPage />);

		const passkeyButton = await screen.findByRole("button", {
			name: /passkey_sign_in/,
		});
		expect(screen.getByLabelText("email_or_username")).toHaveAttribute(
			"autocomplete",
			"username webauthn",
		);
		expect(passkeyButton).toBeDisabled();
		expect(screen.getByText("passkey_unsupported")).toBeInTheDocument();
	});

	it("signs users in with a supported passkey without an identifier", async () => {
		mockState.webAuthnSupported = true;

		render(<LoginPage />);

		fireEvent.click(
			await screen.findByRole("button", { name: /passkey_sign_in/ }),
		);

		await waitFor(() => {
			expect(mockState.startPasskeyLogin).toHaveBeenCalledWith({});
		});
		expect(mockState.getPasskeyCredential).toHaveBeenCalledWith({
			publicKey: { challenge: "AQID" },
		});
		expect(mockState.finishPasskeyLogin).toHaveBeenCalledWith("flow-1", {
			id: "credential-1",
		});
		expect(mockState.syncSession).toHaveBeenCalledWith(900);
		expect(mockState.refreshUser).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"passkey_login_success",
		);
		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith("/", { replace: true });
		});
	});

	it("starts conditional passkey login from the username field", async () => {
		mockState.webAuthnSupported = true;
		mockState.conditionalPasskeySupported = true;

		render(<LoginPage />);

		const identifierInput = await screen.findByLabelText("email_or_username");
		await waitFor(() => {
			expect(identifierInput).toHaveAttribute(
				"autocomplete",
				"username webauthn",
			);
		});

		await waitFor(() => {
			expect(mockState.startPasskeyLogin).toHaveBeenCalledWith({});
		});
		await waitFor(() => {
			expect(mockState.getPasskeyCredential).toHaveBeenCalledWith(
				{ publicKey: { challenge: "AQID" } },
				"conditional",
				expect.any(AbortSignal),
			);
		});
		await waitFor(() => {
			expect(mockState.finishPasskeyLogin).toHaveBeenCalledWith("flow-1", {
				id: "credential-1",
			});
		});
		expect(mockState.syncSession).toHaveBeenCalledWith(900);
		expect(mockState.refreshUser).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).not.toHaveBeenCalledWith(
			"passkey_login_success",
		);
		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith("/", { replace: true });
		});
	});

	it("aborts conditional passkey login before starting explicit passkey login", async () => {
		mockState.webAuthnSupported = true;
		mockState.conditionalPasskeySupported = true;
		let conditionalSignal: AbortSignal | undefined;
		mockState.getPasskeyCredential.mockImplementation(
			(
				_options: unknown,
				mediation?: CredentialMediationRequirement,
				signal?: AbortSignal,
			) => {
				if (mediation === "conditional") {
					conditionalSignal = signal;
					return new Promise(() => undefined);
				}
				return Promise.resolve({ id: "credential-1" });
			},
		);

		render(<LoginPage />);

		const passkeyButton = await screen.findByRole("button", {
			name: /passkey_sign_in/,
		});
		await waitFor(() => {
			expect(conditionalSignal).toBeDefined();
		});

		fireEvent.click(passkeyButton);

		await waitFor(() => {
			expect(conditionalSignal?.aborted).toBe(true);
		});
		await waitFor(() => {
			expect(mockState.getPasskeyCredential).toHaveBeenCalledWith({
				publicKey: { challenge: "AQID" },
			});
		});
		expect(mockState.finishPasskeyLogin).toHaveBeenCalledWith("flow-1", {
			id: "credential-1",
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"passkey_login_success",
		);
	});

	it("shows a query toast passed from the verification redirect", async () => {
		mockState.location = {
			hash: "",
			pathname: "/login",
			search: "?contact_verification=email-changed&email=changed%40example.com",
		};

		render(<LoginPage />);

		await waitFor(() =>
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"settings:settings_email_change_confirmed_login_hint",
				{
					id: "contact-verification-email-changed-login:changed@example.com",
				},
			),
		);
		expect(mockState.navigate).toHaveBeenCalledWith(
			{
				hash: "",
				pathname: "/login",
				search: "",
			},
			{ replace: true },
		);
	});

	it("shows an error toast for expired verification redirects", async () => {
		mockState.location = {
			hash: "",
			pathname: "/login",
			search: "?contact_verification=expired",
		};

		render(<LoginPage />);

		await waitFor(() =>
			expect(mockState.toastError).toHaveBeenCalledWith(
				"verify_contact_expired_title",
				{
					description: "verify_contact_expired_desc",
					id: "contact-verification-expired-login",
				},
			),
		);
		expect(mockState.navigate).toHaveBeenCalledWith(
			{
				hash: "",
				pathname: "/login",
				search: "",
			},
			{ replace: true },
		);
	});

	it("shows a success toast for password reset redirects", async () => {
		mockState.location = {
			hash: "",
			pathname: "/login",
			search: "?password_reset=success",
		};

		render(<LoginPage />);

		await waitFor(() =>
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"password_reset_success_login",
				{
					id: "password-reset-success-login",
				},
			),
		);
		expect(mockState.navigate).toHaveBeenCalledWith(
			{
				hash: "",
				pathname: "/login",
				search: "",
			},
			{ replace: true },
		);
	});

	it("keeps the desktop brand logo matched to the dark hero surface", () => {
		render(<LoginPage />);

		const [desktopLogo, mobileLogo] = screen.getAllByRole("img", {
			name: "AsterDrive",
		});

		expect(desktopLogo).toHaveAttribute(
			"src",
			"/static/asterdrive/asterdrive-light.svg",
		);
		expect(mobileLogo).toHaveAttribute(
			"src",
			"/static/asterdrive/asterdrive-dark.svg",
		);
	});

	it("runs initial setup for the first user and then signs them in", async () => {
		mockState.check.mockResolvedValueOnce({
			has_users: false,
			allow_user_registration: true,
		});

		render(<LoginPage />);

		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "admin@example.com" },
		});

		expect(await screen.findByLabelText("username")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "create_admin" }),
		).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: "adminuser" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret123" },
		});
		fireEvent.click(screen.getByRole("button", { name: "create_admin" }));

		await waitFor(() => {
			expect(mockState.setup).toHaveBeenCalledWith(
				"adminuser",
				"admin@example.com",
				"secret123",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("setup_complete");
		expect(mockState.login).toHaveBeenCalledWith(
			"admin@example.com",
			"secret123",
		);
		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith("/", { replace: true });
		});
	});

	it("uses a username placeholder for the first setup identifier field", async () => {
		mockState.check.mockResolvedValueOnce({
			has_users: false,
			allow_user_registration: true,
		});

		render(<LoginPage />);

		const usernameInput = await screen.findByLabelText("username");
		expect(usernameInput).toHaveAttribute("placeholder", "choose_username");
		expect(usernameInput).not.toHaveAttribute("placeholder", "you@example.com");
	});

	it("shows an activation waiting state after register instead of logging in", async () => {
		mockState.check.mockResolvedValueOnce({
			has_users: true,
			allow_user_registration: true,
		});

		render(<LoginPage />);

		fireEvent.click(await screen.findByRole("button", { name: "sign_up" }));
		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "new@example.com" },
		});

		fireEvent.change(await screen.findByLabelText("username"), {
			target: { value: "newuser" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret123" },
		});
		fireEvent.click(screen.getByRole("button", { name: "sign_up" }));

		await waitFor(() => {
			expect(mockState.register).toHaveBeenCalledWith(
				"newuser",
				"new@example.com",
				"secret123",
			);
		});
		expect(mockState.login).not.toHaveBeenCalled();
		expect(
			await screen.findByText("activation_pending_notice"),
		).toBeInTheDocument();

		fireEvent.click(
			await screen.findByRole("button", { name: /resend_activation/ }),
		);

		await waitFor(() => {
			expect(mockState.resendRegisterActivation).toHaveBeenCalledWith(
				"new@example.com",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("register_success");
		expect(mockState.toastSuccess).toHaveBeenCalledWith("activation_resent");
	});

	it("returns to sign-in mode when registration does not require activation", async () => {
		mockState.check.mockResolvedValueOnce({
			has_users: true,
			allow_user_registration: true,
		});
		mockState.register.mockResolvedValueOnce({ email_verified: true });

		render(<LoginPage />);

		fireEvent.click(await screen.findByRole("button", { name: "sign_up" }));
		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "direct@example.com" },
		});
		fireEvent.change(await screen.findByLabelText("username"), {
			target: { value: "directuser" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret123" },
		});
		fireEvent.click(screen.getByRole("button", { name: "sign_up" }));

		await waitFor(() => {
			expect(mockState.register).toHaveBeenCalledWith(
				"directuser",
				"direct@example.com",
				"secret123",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"register_success_direct",
		);
		expect(mockState.login).not.toHaveBeenCalled();
		expect(
			screen.queryByText("activation_pending_notice"),
		).not.toBeInTheDocument();
		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: "sign_in" }),
			).toBeInTheDocument();
		});
		expect(screen.getByLabelText("email")).toHaveValue("direct@example.com");
	});

	it("switches pending-activation login failures into the activation state", async () => {
		mockState.login.mockRejectedValueOnce(new MockApiError(2004, "pending"));

		render(<LoginPage />);

		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "user@example.com" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret123" },
		});

		await screen.findByRole("button", { name: "sign_in" });
		fireEvent.click(screen.getByRole("button", { name: "sign_in" }));

		expect(
			await screen.findByText("activation_pending_notice"),
		).toBeInTheDocument();

		fireEvent.click(
			await screen.findByRole("button", { name: /resend_activation/ }),
		);

		await waitFor(() => {
			expect(mockState.resendRegisterActivation).toHaveBeenCalledWith(
				"user@example.com",
			);
		});
	});

	it("requests a password reset from the login view", async () => {
		render(<LoginPage />);

		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "user@example.com" },
		});

		await screen.findByRole("button", { name: "sign_in" });
		fireEvent.click(screen.getByRole("button", { name: "forgot_password" }));

		fireEvent.click(
			await screen.findByRole("button", { name: /send_password_reset/ }),
		);

		await waitFor(() => {
			expect(mockState.requestPasswordReset).toHaveBeenCalledWith({
				email: "user@example.com",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"password_reset_request_sent",
		);
	});

	it("keeps forgot password visible in register mode and prefills the email field", async () => {
		mockState.check.mockResolvedValueOnce({
			has_users: true,
			allow_user_registration: true,
		});

		render(<LoginPage />);

		fireEvent.click(await screen.findByRole("button", { name: "sign_up" }));
		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "newuser" },
		});

		const emailField = await screen.findByLabelText("email");
		fireEvent.change(emailField, {
			target: { value: "new@example.com" },
		});

		fireEvent.click(screen.getByRole("button", { name: "forgot_password" }));

		expect(await screen.findByLabelText("email")).toHaveValue(
			"new@example.com",
		);
	});

	it("shows validation errors and reports submit failures without navigating", async () => {
		const error = new Error("login failed");
		mockState.login.mockRejectedValueOnce(error);

		render(<LoginPage />);

		await waitFor(() => {
			expect(mockState.check).toHaveBeenCalledWith();
		});

		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "bad" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "123" },
		});
		fireEvent.click(screen.getByRole("button", { name: "sign_in" }));

		expect(screen.getByText("invalid-username")).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "user@example.com" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret123" },
		});
		await screen.findByRole("button", { name: "sign_in" });

		fireEvent.click(screen.getByRole("button", { name: "sign_in" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(mockState.navigate).not.toHaveBeenCalled();
	});

	it("keeps submit disabled until login fields are filled", async () => {
		render(<LoginPage />);

		const submitButton = screen.getByRole("button", { name: "continue" });
		expect(submitButton).toBeDisabled();

		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "user@example.com" },
		});
		expect(submitButton).toBeDisabled();

		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret123" },
		});

		await waitFor(() => {
			expect(screen.getByRole("button", { name: "sign_in" })).toBeEnabled();
		});
	});

	it("keeps submit disabled until register fields are filled", async () => {
		mockState.check.mockResolvedValueOnce({
			has_users: true,
			allow_user_registration: true,
		});

		render(<LoginPage />);

		fireEvent.click(await screen.findByRole("button", { name: "sign_up" }));
		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "new@example.com" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret123" },
		});

		await waitFor(() => {
			expect(screen.getByRole("button", { name: "sign_up" })).toBeDisabled();
		});

		fireEvent.change(await screen.findByLabelText("username"), {
			target: { value: "newuser" },
		});

		await waitFor(() => {
			expect(screen.getByRole("button", { name: "sign_up" })).toBeEnabled();
		});
	});

	it("falls back to sign-in mode when the initial auth-state check fails", async () => {
		mockState.check.mockRejectedValueOnce(new Error("network error"));

		render(<LoginPage />);

		await waitFor(() => {
			expect(mockState.check).toHaveBeenCalledWith();
		});
		expect(
			await screen.findByRole("button", { name: "sign_in" }),
		).toBeInTheDocument();
	});

	it("falls back to the normal sign-in mode when public registration is disabled", async () => {
		mockState.check.mockResolvedValueOnce({
			has_users: true,
			allow_user_registration: false,
		});

		render(<LoginPage />);

		await waitFor(() => {
			expect(mockState.check).toHaveBeenCalledWith();
		});
		expect(
			await screen.findByRole("button", { name: "sign_in" }),
		).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "sign_up" }),
		).not.toBeInTheDocument();
		expect(screen.queryByLabelText("username")).not.toBeInTheDocument();
		await waitFor(() => {
			expect(screen.getByText("enter_password")).toBeInTheDocument();
		});
		expect(
			screen.queryByText("registration_closed_desc"),
		).not.toBeInTheDocument();
	});
});
