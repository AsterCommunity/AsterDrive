import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
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

const MockWebAuthnCancelledError = vi.hoisted(
	() =>
		class MockWebAuthnCancelledError extends Error {
			constructor(message = "cancelled") {
				super(message);
				this.name = "WebAuthnCancelledError";
			}
		},
);

const MockWebAuthnUnsupportedError = vi.hoisted(
	() =>
		class MockWebAuthnUnsupportedError extends Error {
			constructor(message = "unsupported") {
				super(message);
				this.name = "WebAuthnUnsupportedError";
			}
		},
);

const mockTranslate = vi.hoisted(
	() => (key: string, options?: Record<string, unknown>) => {
		const normalized = key.replace(/^core:/, "");
		if (
			normalized === "external_auth_sign_in_with" &&
			typeof options?.provider === "string"
		) {
			return `external_auth_sign_in_with ${options.provider}`;
		}
		return normalized;
	},
);

const mockState = vi.hoisted(() => ({
	check: vi.fn(),
	conditionalPasskeyError: null as Error | null,
	handleApiError: vi.fn(),
	allowUserRegistration: true,
	conditionalPasskeySupported: false,
	forceEnableDisabledButtons: false,
	finishPasskeyLogin: vi.fn(),
	getPasskeyCredential: vi.fn(),
	locationAssign: vi.fn(),
	linkExternalAuthWithPassword: vi.fn(),
	listExternalAuthProviders: vi.fn(),
	login: vi.fn(),
	loggerWarn: vi.fn(),
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
	startExternalAuthEmailVerification: vi.fn(),
	startExternalAuthLogin: vi.fn(),
	startPasskeyLogin: vi.fn(),
	syncSession: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
	verifyMfaChallenge: vi.fn(),
	webAuthnSupported: false,
}));

function requestAnimationFrameCallback(callback: FrameRequestCallback) {
	callback(0);
	return 0;
}

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: mockTranslate,
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
			disabled={disabled && !mockState.forceEnableDisabledButtons}
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

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: (...args: unknown[]) => mockState.loggerWarn(...args),
	},
}));

vi.mock("@/pages/login/ExternalAuthRecoveryPanel", () => ({
	ExternalAuthRecoveryPanel: ({
		email,
		emailError,
		identifier,
		identifierError,
		mode,
		onBack,
		onEmailChange,
		onIdentifierChange,
		onModeChange,
		onPasswordChange,
		password,
		passwordError,
		sent,
		t,
	}: {
		email: string;
		emailError: string;
		identifier: string;
		identifierError: string;
		mode: "password" | "email";
		onBack: () => void;
		onEmailChange: (value: string, error: string) => void;
		onIdentifierChange: (value: string) => void;
		onModeChange: (mode: "password" | "email") => void;
		onPasswordChange: (value: string) => void;
		password: string;
		passwordError: string;
		sent: boolean;
		t: (key: string) => string;
	}) => (
		<div>
			<div>external_auth_email_verification_title</div>
			<div>{sent ? "external_auth_email_verification_sent_title" : ""}</div>
			<button
				type="button"
				role="tab"
				aria-selected={mode === "password"}
				onClick={() => onModeChange("password")}
			>
				external_auth_password_link_tab
			</button>
			<button
				type="button"
				role="tab"
				aria-selected={mode === "email"}
				onClick={() => onModeChange("email")}
			>
				external_auth_email_verification_tab
			</button>
			{mode === "password" ? (
				<div>
					<label htmlFor="external-auth-password-link-identifier">
						email_or_username
					</label>
					<input
						id="external-auth-password-link-identifier"
						value={identifier}
						onChange={(event) => onIdentifierChange(event.target.value)}
					/>
					<div>{identifierError}</div>
					<label htmlFor="external-auth-password-link-password">password</label>
					<input
						id="external-auth-password-link-password"
						value={password}
						onChange={(event) => onPasswordChange(event.target.value)}
					/>
					<div>{passwordError}</div>
					<button type="submit">
						{t("external_auth_password_link_submit")}
					</button>
				</div>
			) : (
				<div>
					<label htmlFor="external-auth-recovery-email">email</label>
					<input
						id="external-auth-recovery-email"
						value={email}
						onChange={(event) => {
							const value = event.target.value;
							onEmailChange(
								value,
								/^[^@]+@[^@]+\.[^@]+$/.test(value) ? "" : "invalid-email",
							);
						}}
					/>
					<div>{emailError}</div>
					<button type="submit">
						{t("external_auth_email_verification_send")}
					</button>
					<div>{sent && email ? `email: ${email}` : ""}</div>
				</div>
			)}
			<button type="button" onClick={onBack}>
				back_to_sign_in
			</button>
		</div>
	),
}));

vi.mock("@/services/authService", () => ({
	authService: {
		check: (...args: unknown[]) => mockState.check(...args),
		finishPasskeyLogin: (...args: unknown[]) =>
			mockState.finishPasskeyLogin(...args),
		linkExternalAuthWithPassword: (...args: unknown[]) =>
			mockState.linkExternalAuthWithPassword(...args),
		listExternalAuthProviders: (...args: unknown[]) =>
			mockState.listExternalAuthProviders(...args),
		login: (...args: unknown[]) => mockState.login(...args),
		requestPasswordReset: (...args: unknown[]) =>
			mockState.requestPasswordReset(...args),
		register: (...args: unknown[]) => mockState.register(...args),
		resendRegisterActivation: (...args: unknown[]) =>
			mockState.resendRegisterActivation(...args),
		setup: (...args: unknown[]) => mockState.setup(...args),
		startExternalAuthEmailVerification: (...args: unknown[]) =>
			mockState.startExternalAuthEmailVerification(...args),
		startExternalAuthLogin: (...args: unknown[]) =>
			mockState.startExternalAuthLogin(...args),
		startPasskeyLogin: (...args: unknown[]) =>
			mockState.startPasskeyLogin(...args),
		verifyMfaChallenge: (...args: unknown[]) =>
			mockState.verifyMfaChallenge(...args),
	},
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (
		selector: (state: {
			refreshUser: typeof mockState.refreshUser;
			syncSession: typeof mockState.syncSession;
		}) => unknown,
	) =>
		selector({
			refreshUser: mockState.refreshUser,
			syncSession: mockState.syncSession,
		}),
}));

vi.mock("@/lib/webauthn", () => ({
	getPasskeyCredential: (...args: unknown[]) =>
		mockState.getPasskeyCredential(...args),
	isConditionalPasskeyLoginAvailable: () =>
		mockState.conditionalPasskeyError
			? Promise.reject(mockState.conditionalPasskeyError)
			: Promise.resolve(mockState.conditionalPasskeySupported),
	isWebAuthnSupported: () => mockState.webAuthnSupported,
	WebAuthnCancelledError: MockWebAuthnCancelledError,
	WebAuthnUnsupportedError: MockWebAuthnUnsupportedError,
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
		vi.spyOn(window, "requestAnimationFrame").mockImplementation(
			requestAnimationFrameCallback,
		);
		Object.defineProperty(window, "location", {
			configurable: true,
			value: {
				...window.location,
				assign: mockState.locationAssign,
			},
		});
		document.documentElement.classList.remove("dark");
		mockState.allowUserRegistration = true;
		mockState.conditionalPasskeyError = null;
		mockState.conditionalPasskeySupported = false;
		mockState.forceEnableDisabledButtons = false;
		mockState.check.mockReset();
		mockState.finishPasskeyLogin.mockReset();
		mockState.getPasskeyCredential.mockReset();
		mockState.handleApiError.mockReset();
		mockState.locationAssign.mockReset();
		mockState.linkExternalAuthWithPassword.mockReset();
		mockState.listExternalAuthProviders.mockReset();
		mockState.login.mockReset();
		mockState.loggerWarn.mockReset();
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
		mockState.startExternalAuthEmailVerification.mockReset();
		mockState.startExternalAuthLogin.mockReset();
		mockState.startPasskeyLogin.mockReset();
		mockState.syncSession.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.verifyMfaChallenge.mockReset();
		mockState.webAuthnSupported = false;
		mockState.finishPasskeyLogin.mockResolvedValue({ expiresIn: 900 });
		mockState.getPasskeyCredential.mockResolvedValue({ id: "credential-1" });
		mockState.listExternalAuthProviders.mockResolvedValue([]);
		mockState.linkExternalAuthWithPassword.mockResolvedValue({
			status: "authenticated",
			expiresIn: 900,
		});
		mockState.login.mockResolvedValue({
			status: "authenticated",
			expiresIn: 900,
		});
		mockState.refreshUser.mockResolvedValue(undefined);
		mockState.register.mockResolvedValue({ email_verified: false });
		mockState.requestPasswordReset.mockResolvedValue(undefined);
		mockState.resendRegisterActivation.mockResolvedValue(undefined);
		mockState.setup.mockResolvedValue(undefined);
		mockState.startExternalAuthEmailVerification.mockResolvedValue({
			message: "sent",
		});
		mockState.startExternalAuthLogin.mockResolvedValue({
			authorization_url: "https://idp.example.com/authorize",
		});
		mockState.startPasskeyLogin.mockResolvedValue({
			flow_id: "flow-1",
			public_key: { publicKey: { challenge: "AQID" } },
		});
		mockState.syncSession.mockReturnValue(undefined);
		mockState.verifyMfaChallenge.mockResolvedValue({ expiresIn: 900 });
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
		expect(mockState.syncSession).toHaveBeenCalledWith(900);
		expect(mockState.refreshUser).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("login_success");
	});

	it("moves password login into the MFA panel and signs in after verification", async () => {
		mockState.login.mockResolvedValueOnce({
			status: "mfa_required",
			flowToken: "mfa-flow",
			expiresIn: 300,
			methods: ["totp", "recovery_code"],
		});

		render(<LoginPage />);

		fireEvent.change(await screen.findByLabelText("email_or_username"), {
			target: { value: "user@example.com" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret123" },
		});
		fireEvent.click(screen.getByRole("button", { name: "sign_in" }));

		expect(await screen.findByText("mfa_panel_title")).toBeInTheDocument();
		expect(mockState.syncSession).not.toHaveBeenCalled();

		fireEvent.change(screen.getByLabelText("mfa_code_label"), {
			target: { value: "123456" },
		});
		fireEvent.click(await screen.findByRole("button", { name: /mfa_verify/ }));

		await waitFor(() => {
			expect(mockState.verifyMfaChallenge).toHaveBeenCalledWith({
				flow_token: "mfa-flow",
				method: "totp",
				code: "123456",
			});
		});
		expect(mockState.syncSession).toHaveBeenCalledWith(900);
		expect(mockState.refreshUser).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("login_success");
		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith("/", { replace: true });
		});
	});

	it("submits recovery codes from the single MFA field", async () => {
		mockState.login.mockResolvedValueOnce({
			status: "mfa_required",
			flowToken: "mfa-flow",
			expiresIn: 300,
			methods: ["totp", "recovery_code"],
		});

		render(<LoginPage />);

		fireEvent.change(await screen.findByLabelText("email_or_username"), {
			target: { value: "user@example.com" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret123" },
		});
		fireEvent.click(screen.getByRole("button", { name: "sign_in" }));

		await screen.findByText("mfa_panel_title");
		fireEvent.change(screen.getByLabelText("mfa_code_label"), {
			target: { value: "G3THI-TMIHN" },
		});
		fireEvent.click(await screen.findByRole("button", { name: /mfa_verify/ }));

		await waitFor(() => {
			expect(mockState.verifyMfaChallenge).toHaveBeenCalledWith({
				flow_token: "mfa-flow",
				method: "recovery_code",
				code: "G3THI-TMIHN",
			});
		});
	});

	it("opens an MFA challenge from the redirect query and verifies back to the requested return path", async () => {
		mockState.location = {
			hash: "#top",
			pathname: "/login",
			search:
				"?mfa=required&flow=redirect-flow&return_path=%2Fsettings%2Fsecurity",
		};

		render(<LoginPage />);

		expect(await screen.findByText("mfa_panel_title")).toBeInTheDocument();
		expect(mockState.navigate).toHaveBeenCalledWith(
			{
				hash: "#top",
				pathname: "/login",
				search: "",
			},
			{ replace: true },
		);

		await screen.findByRole("button", { name: /mfa_verify/ });
		fireEvent.change(screen.getByLabelText("mfa_code_label"), {
			target: { value: "123456" },
		});
		fireEvent.click(screen.getByRole("button", { name: /mfa_verify/ }));

		await waitFor(() => {
			expect(mockState.verifyMfaChallenge).toHaveBeenCalledWith({
				code: "123456",
				flow_token: "redirect-flow",
				method: "totp",
			});
		});
		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith("/settings/security", {
				replace: true,
			});
		});
	});

	it("validates MFA challenge expiry and required code before verification", async () => {
		vi.useFakeTimers({ shouldAdvanceTime: true });
		vi.setSystemTime(new Date("2026-05-24T08:00:00.000Z"));
		mockState.login.mockResolvedValueOnce({
			status: "mfa_required",
			flowToken: "mfa-flow",
			expiresIn: 300,
			methods: ["totp"],
		});

		render(<LoginPage />);

		fireEvent.change(await screen.findByLabelText("email_or_username"), {
			target: { value: "user@example.com" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret123" },
		});
		fireEvent.click(screen.getByRole("button", { name: "sign_in" }));

		await screen.findByText("mfa_panel_title");
		await screen.findByRole("button", { name: /mfa_verify/ });
		const mfaForm = screen.getByLabelText("mfa_code_label").closest("form");
		if (!mfaForm) {
			throw new Error("MFA form not found");
		}
		fireEvent.submit(mfaForm);
		expect(screen.getByText("mfa_code_required")).toBeInTheDocument();
		expect(mockState.verifyMfaChallenge).not.toHaveBeenCalled();

		fireEvent.change(screen.getByLabelText("mfa_code_label"), {
			target: { value: "123456" },
		});
		act(() => {
			vi.setSystemTime(new Date("2026-05-24T08:06:00.000Z"));
			vi.advanceTimersByTime(1000);
		});
		await screen.findByText("mfa_flow_expired");
		fireEvent.submit(mfaForm);

		expect(screen.getAllByText("mfa_flow_expired").length).toBeGreaterThan(0);
		expect(mockState.verifyMfaChallenge).not.toHaveBeenCalled();
	});

	it("reports MFA verification failures and allows returning to sign-in", async () => {
		const error = new Error("invalid mfa");
		mockState.login.mockResolvedValueOnce({
			status: "mfa_required",
			flowToken: "mfa-flow",
			expiresIn: 300,
			methods: [],
		});
		mockState.verifyMfaChallenge.mockRejectedValueOnce(error);

		render(<LoginPage />);

		fireEvent.change(await screen.findByLabelText("email_or_username"), {
			target: { value: "user@example.com" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret123" },
		});
		fireEvent.click(screen.getByRole("button", { name: "sign_in" }));

		await screen.findByText("mfa_panel_title");
		await screen.findByRole("button", { name: /mfa_verify/ });
		fireEvent.change(screen.getByLabelText("mfa_code_label"), {
			target: { value: "123456" },
		});
		fireEvent.click(screen.getByRole("button", { name: /mfa_verify/ }));

		await waitFor(() => {
			expect(mockState.verifyMfaChallenge).toHaveBeenCalledWith({
				code: "123456",
				flow_token: "mfa-flow",
				method: "totp",
			});
		});
		expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		expect(screen.getByText("mfa_panel_title")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /back_to_sign_in/ }));
		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: "sign_in" }),
			).toBeInTheDocument();
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

	it("handles passkey support detection failures and blocked explicit passkey requests", async () => {
		const detectionError = new Error("conditional detection failed");
		mockState.conditionalPasskeyError = detectionError;
		mockState.forceEnableDisabledButtons = true;

		render(<LoginPage />);

		await screen.findByRole("button", { name: /passkey_sign_in/ });
		await waitFor(() => {
			expect(mockState.loggerWarn).toHaveBeenCalledWith(
				"conditional passkey support detection failed",
				detectionError,
			);
		});

		fireEvent.click(screen.getByRole("button", { name: /passkey_sign_in/ }));

		await waitFor(() => {
			expect(mockState.toastError).toHaveBeenCalledWith("passkey_unsupported");
		});
		expect(mockState.startPasskeyLogin).not.toHaveBeenCalled();
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
		await waitFor(() => {
			expect(mockState.getPasskeyCredential).toHaveBeenCalledWith({
				publicKey: { challenge: "AQID" },
			});
			expect(mockState.finishPasskeyLogin).toHaveBeenCalledWith("flow-1", {
				id: "credential-1",
			});
			expect(mockState.syncSession).toHaveBeenCalledWith(900);
			expect(mockState.refreshUser).toHaveBeenCalledTimes(1);
			expect(mockState.toastSuccess).toHaveBeenCalledWith("login_success");
		});
		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith("/", { replace: true });
		});
	});

	it("passes a trimmed identifier into explicit passkey login", async () => {
		mockState.webAuthnSupported = true;

		render(<LoginPage />);

		fireEvent.change(await screen.findByLabelText("email_or_username"), {
			target: { value: "  user@example.com  " },
		});
		fireEvent.click(screen.getByRole("button", { name: /passkey_sign_in/ }));

		await waitFor(() => {
			expect(mockState.startPasskeyLogin).toHaveBeenCalledWith({
				identifier: "user@example.com",
			});
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
			expect(mockState.startPasskeyLogin).toHaveBeenCalledWith({
				conditional: true,
			});
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
		expect(mockState.toastSuccess).toHaveBeenCalledWith("login_success");
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
		expect(mockState.toastSuccess).toHaveBeenCalledWith("login_success");
	});

	it("ignores a conditional passkey result after the request has been aborted", async () => {
		mockState.webAuthnSupported = true;
		mockState.conditionalPasskeySupported = true;
		let conditionalSignal: AbortSignal | undefined;
		let resolveConditionalCredential:
			| ((credential: { id: string }) => void)
			| undefined;
		const conditionalCredentialPromise = new Promise<{ id: string }>(
			(resolve) => {
				resolveConditionalCredential = resolve;
			},
		);
		mockState.getPasskeyCredential.mockImplementation(
			(
				_options: unknown,
				mediation?: CredentialMediationRequirement,
				signal?: AbortSignal,
			) => {
				if (mediation === "conditional") {
					conditionalSignal = signal;
					return conditionalCredentialPromise;
				}
				return Promise.resolve({ id: "credential-1" });
			},
		);

		const view = render(<LoginPage />);

		await waitFor(() => {
			expect(conditionalSignal).toBeDefined();
		});
		view.unmount();
		await act(async () => {
			resolveConditionalCredential?.({ id: "late-credential" });
			await conditionalCredentialPromise;
		});

		expect(mockState.finishPasskeyLogin).not.toHaveBeenCalled();
		expect(mockState.navigate).not.toHaveBeenCalled();
	});

	it("shows explicit passkey login errors for unsupported, cancelled, and API failures", async () => {
		mockState.webAuthnSupported = true;

		render(<LoginPage />);

		await screen.findByRole("button", { name: /passkey_sign_in/ });
		mockState.getPasskeyCredential.mockRejectedValueOnce(
			new MockWebAuthnUnsupportedError(),
		);
		fireEvent.click(screen.getByRole("button", { name: /passkey_sign_in/ }));

		await waitFor(() =>
			expect(mockState.toastError).toHaveBeenCalledWith("passkey_unsupported"),
		);

		mockState.getPasskeyCredential.mockRejectedValueOnce(
			new MockWebAuthnCancelledError(),
		);
		fireEvent.click(screen.getByRole("button", { name: /passkey_sign_in/ }));

		await waitFor(() =>
			expect(mockState.toastError).toHaveBeenCalledWith("passkey_cancelled"),
		);

		const error = new Error("passkey login failed");
		mockState.getPasskeyCredential.mockRejectedValueOnce(error);
		fireEvent.click(screen.getByRole("button", { name: /passkey_sign_in/ }));

		await waitFor(() =>
			expect(mockState.handleApiError).toHaveBeenCalledWith(error),
		);
	});

	it("ignores cancelled conditional passkey login and reports API failures", async () => {
		mockState.webAuthnSupported = true;
		mockState.conditionalPasskeySupported = true;
		mockState.getPasskeyCredential.mockRejectedValueOnce(
			new MockWebAuthnCancelledError(),
		);

		const firstView = render(<LoginPage />);

		await waitFor(() => {
			expect(mockState.getPasskeyCredential).toHaveBeenCalledWith(
				{ publicKey: { challenge: "AQID" } },
				"conditional",
				expect.any(AbortSignal),
			);
		});
		await waitFor(() => {
			expect(mockState.handleApiError).not.toHaveBeenCalled();
		});
		firstView.unmount();

		const error = new Error("conditional start failed");
		mockState.getPasskeyCredential.mockReset();
		mockState.startPasskeyLogin.mockRejectedValueOnce(error);

		render(<LoginPage />);

		await waitFor(() =>
			expect(mockState.handleApiError).toHaveBeenCalledWith(error),
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

	it("shows query toasts for invalid, missing, and activated verification redirects", async () => {
		const cases = [
			{
				id: "contact-verification-invalid-login",
				search: "?contact_verification=invalid",
				title: "verify_contact_invalid_title",
				description: "verify_contact_invalid_desc",
			},
			{
				id: "contact-verification-missing-login",
				search: "?contact_verification=missing",
				title: "verify_contact_missing_token_title",
				description: "verify_contact_missing_token_desc",
			},
		];

		for (const item of cases) {
			mockState.location = {
				hash: "",
				pathname: "/login",
				search: item.search,
			};
			mockState.navigate.mockReset();
			mockState.toastError.mockReset();

			const view = render(<LoginPage />);

			await waitFor(() =>
				expect(mockState.toastError).toHaveBeenCalledWith(item.title, {
					description: item.description,
					id: item.id,
				}),
			);
			expect(mockState.navigate).toHaveBeenCalledWith(
				{
					hash: "",
					pathname: "/login",
					search: "",
				},
				{ replace: true },
			);
			view.unmount();
		}

		mockState.location = {
			hash: "",
			pathname: "/login",
			search: "?contact_verification=register-activated",
		};
		mockState.navigate.mockReset();
		mockState.toastSuccess.mockReset();

		render(<LoginPage />);

		await waitFor(() =>
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"activation_confirmed",
				{
					id: "contact-verification-register-activated-login",
				},
			),
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

	it("shows an external auth error toast and clears the redirect query", async () => {
		mockState.location = {
			hash: "",
			pathname: "/login",
			search: "?external_auth=error&code=2003&return_path=%2Ffiles",
		};

		render(<LoginPage />);

		await waitFor(() =>
			expect(mockState.toastError).toHaveBeenCalledWith(
				"external_auth_login_failed",
				{
					description: "external_auth_login_failed_desc",
					id: "external-auth-login-error",
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

	it("opens the external auth recovery panel from an email-required redirect", async () => {
		mockState.location = {
			hash: "",
			pathname: "/login",
			search:
				"?external_auth=email_required&flow=flow-token&return_path=%2Fsettings%2Fsecurity",
		};

		render(<LoginPage />);

		expect(
			await screen.findByText("external_auth_email_verification_title"),
		).toBeInTheDocument();
		expect(
			await screen.findByText("external_auth_password_link_tab"),
		).toBeInTheDocument();
		expect(mockState.navigate).toHaveBeenCalledWith(
			{
				hash: "",
				pathname: "/login",
				search: "",
			},
			{ replace: true },
		);
	});

	it("links an external auth flow with an existing password", async () => {
		mockState.location = {
			hash: "",
			pathname: "/login",
			search: "?external_auth=email_required&flow=flow-token",
		};

		render(<LoginPage />);

		await screen.findByRole("button", {
			name: /external_auth_password_link_submit/,
		});
		const identifierInput = document.querySelector<HTMLInputElement>(
			"#external-auth-password-link-identifier",
		);
		const passwordInput = document.querySelector<HTMLInputElement>(
			"#external-auth-password-link-password",
		);
		if (!identifierInput || !passwordInput) {
			throw new Error("external auth password-link fields not found");
		}
		fireEvent.change(identifierInput, {
			target: { value: "  user@example.com  " },
		});
		fireEvent.change(passwordInput, {
			target: { value: "secret123" },
		});
		const submitButton = screen.getByRole("button", {
			name: /external_auth_password_link_submit/,
		});
		await waitFor(() => {
			expect(submitButton).toBeEnabled();
		});
		fireEvent.click(submitButton);

		await waitFor(() => {
			expect(mockState.linkExternalAuthWithPassword).toHaveBeenCalledWith({
				flow_token: "flow-token",
				identifier: "user@example.com",
				password: "secret123",
			});
		});
		expect(mockState.syncSession).toHaveBeenCalledWith(900);
		expect(mockState.refreshUser).toHaveBeenCalledTimes(1);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"external_auth_password_link_success",
		);

		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith("/", { replace: true });
		});
	});

	it("validates external auth password-link fields before submitting", async () => {
		mockState.forceEnableDisabledButtons = true;
		mockState.location = {
			hash: "",
			pathname: "/login",
			search: "?external_auth=email_required&flow=flow-token",
		};

		render(<LoginPage />);

		await screen.findByRole("button", {
			name: /external_auth_password_link_submit/,
		});
		fireEvent.click(
			screen.getByRole("button", {
				name: /external_auth_password_link_submit/,
			}),
		);

		expect(screen.getAllByText("email_or_username").length).toBeGreaterThan(0);
		expect(screen.getByText("password-required")).toBeInTheDocument();
		expect(mockState.linkExternalAuthWithPassword).not.toHaveBeenCalled();

		const identifierInput = document.querySelector<HTMLInputElement>(
			"#external-auth-password-link-identifier",
		);
		if (!identifierInput) {
			throw new Error("external auth password-link identifier not found");
		}
		fireEvent.change(identifierInput, {
			target: { value: "user@example.com" },
		});
	});

	it("sends external auth email verification and shows the sent state", async () => {
		mockState.location = {
			hash: "",
			pathname: "/login",
			search: "?external_auth=email_required&flow=flow-token",
		};

		render(<LoginPage />);

		const emailTab = await screen.findByRole("tab", {
			name: /external_auth_email_verification_tab/,
		});
		fireEvent.click(emailTab);
		await waitFor(() => {
			expect(emailTab).toHaveAttribute("aria-selected", "true");
		});
		const emailInput = await waitFor(() => {
			const input = document.querySelector<HTMLInputElement>(
				"#external-auth-recovery-email",
			);
			if (!input) throw new Error("email recovery field not ready");
			return input;
		});
		fireEvent.change(emailInput, {
			target: { value: "verify@example.com" },
		});
		fireEvent.click(
			screen.getByRole("button", {
				name: /external_auth_email_verification_send/,
			}),
		);

		await waitFor(() => {
			expect(mockState.startExternalAuthEmailVerification).toHaveBeenCalledWith(
				{
					email: "verify@example.com",
					flow_token: "flow-token",
				},
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"external_auth_email_verification_sent_toast",
		);
		expect(
			await screen.findByText("external_auth_email_verification_sent_title"),
		).toBeInTheDocument();
		expect(screen.getByText("email: verify@example.com")).toBeInTheDocument();
	});

	it("validates and reports external auth email verification failures", async () => {
		const error = new Error("email verification failed");
		mockState.startExternalAuthEmailVerification.mockRejectedValueOnce(error);
		mockState.location = {
			hash: "",
			pathname: "/login",
			search: "?external_auth=email_required&flow=flow-token",
		};

		render(<LoginPage />);

		const emailTab = await screen.findByRole("tab", {
			name: /external_auth_email_verification_tab/,
		});
		fireEvent.click(emailTab);
		await waitFor(() => {
			expect(emailTab).toHaveAttribute("aria-selected", "true");
		});
		const emailInput = await waitFor(() => {
			const input = document.querySelector<HTMLInputElement>(
				"#external-auth-recovery-email",
			);
			if (!input) throw new Error("email recovery field not ready");
			return input;
		});
		fireEvent.change(emailInput, {
			target: { value: "bad" },
		});
		fireEvent.click(
			screen.getByRole("button", {
				name: /external_auth_email_verification_send/,
			}),
		);

		expect(screen.getByText("invalid-email")).toBeInTheDocument();
		expect(mockState.startExternalAuthEmailVerification).not.toHaveBeenCalled();

		fireEvent.change(emailInput, {
			target: { value: "verify@example.com" },
		});
		fireEvent.click(
			screen.getByRole("button", {
				name: /external_auth_email_verification_send/,
			}),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
	});

	it("closes external auth recovery and returns to the sign-in form", async () => {
		mockState.location = {
			hash: "",
			pathname: "/login",
			search: "?external_auth=email_required&flow=flow-token",
		};

		render(<LoginPage />);

		fireEvent.click(
			await screen.findByRole("button", { name: /back_to_sign_in/ }),
		);

		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: "sign_in" }),
			).toBeInTheDocument();
		});
	});

	it("shows external auth recovery redirect error toasts", async () => {
		const cases = [
			{
				description: "external_auth_email_verification_missing_token_desc",
				id: "external-auth-recovery-missing",
				search: "?external_auth=email_verification_missing",
				title: "external_auth_email_verification_missing_token_title",
			},
			{
				description: "external_auth_email_verification_invalid_desc",
				id: "external-auth-recovery-invalid",
				search: "?external_auth=email_verification_invalid",
				title: "external_auth_email_verification_invalid_title",
			},
			{
				description: "external_auth_email_verification_expired_desc",
				id: "external-auth-recovery-expired",
				search: "?external_auth=email_verification_expired",
				title: "external_auth_email_verification_expired_title",
			},
		];

		for (const item of cases) {
			mockState.location = {
				hash: "",
				pathname: "/login",
				search: item.search,
			};
			mockState.navigate.mockReset();
			mockState.toastError.mockReset();

			const view = render(<LoginPage />);

			await waitFor(() => {
				expect(mockState.toastError).toHaveBeenCalledWith(item.title, {
					description: item.description,
					id: item.id,
				});
			});
			expect(mockState.navigate).toHaveBeenCalledWith(
				{ hash: "", pathname: "/login", search: "" },
				{ replace: true },
			);
			view.unmount();
		}
	});

	it("starts external auth login with the provider kind and key", async () => {
		const provider = {
			display_name: "Example IDP",
			icon_url: "/static/external-auth/example.svg",
			key: "example",
			kind: "oidc",
		};
		mockState.listExternalAuthProviders.mockResolvedValue([provider]);

		render(<LoginPage />);

		const externalAuthButton = await screen.findByRole("button", {
			name: /Example IDP/,
		});
		expect(externalAuthButton.querySelector("img")).toHaveAttribute(
			"src",
			"/static/external-auth/example.svg",
		);
		fireEvent.click(externalAuthButton);

		await waitFor(() =>
			expect(mockState.startExternalAuthLogin).toHaveBeenCalledWith(provider, {
				return_path: "/?external_auth=success",
			}),
		);
		expect(mockState.locationAssign).toHaveBeenCalledWith(
			"https://idp.example.com/authorize",
		);
	});

	it("reports external auth provider loading and login start failures", async () => {
		const loadError = new Error("providers failed");
		mockState.listExternalAuthProviders.mockRejectedValueOnce(loadError);

		const firstView = render(<LoginPage />);

		await waitFor(() => {
			expect(mockState.loggerWarn).toHaveBeenCalledWith(
				"failed to load external auth providers",
				loadError,
			);
		});
		firstView.unmount();

		const provider = {
			display_name: "Example IDP",
			icon_url: null,
			key: "example",
			kind: "oidc",
		};
		const startError = new Error("start failed");
		mockState.listExternalAuthProviders.mockResolvedValueOnce([provider]);
		mockState.startExternalAuthLogin.mockRejectedValueOnce(startError);

		render(<LoginPage />);

		fireEvent.click(await screen.findByRole("button", { name: /Example IDP/ }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(startError);
		});
		expect(mockState.locationAssign).not.toHaveBeenCalled();
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
