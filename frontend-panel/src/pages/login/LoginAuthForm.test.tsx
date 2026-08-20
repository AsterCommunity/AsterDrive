import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { LoginAuthForm } from "@/pages/login/LoginAuthForm";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			key === "external_auth_sign_in_with"
				? `external_auth_sign_in_with ${String(options?.provider)}`
				: key.replace(/^core:/, ""),
	}),
}));

function renderForm(
	overrides: Partial<React.ComponentProps<typeof LoginAuthForm>> = {},
) {
	const props: React.ComponentProps<typeof LoginAuthForm> = {
		checking: false,
		errors: {},
		extraField: "",
		extraLabel: "email",
		extraPlaceholder: "you@example.com",
		identifier: "user@example.com",
		identifierLabel: "email_or_username",
		identifierPlaceholder: "you@example.com",
		isSubmitDisabled: false,
		mode: "login",
		modeActionText: "sign_in",
		password: "secret123",
		externalAuthBusyProvider: null,
		externalAuthLoading: false,
		externalAuthProviders: [],
		passkeyLoginEnabled: false,
		passwordLoginEnabled: true,
		passkeySubmitting: false,
		passkeySupported: false,
		registrationClosed: false,
		showPassword: false,
		submitLabel: "sign_in",
		submitting: false,
		onExtraFieldChange: vi.fn(),
		onForgotPassword: vi.fn(),
		onIdentifierChange: vi.fn(),
		onPasswordChange: vi.fn(),
		onExternalAuthLogin: vi.fn(),
		onPasskeyLogin: vi.fn(),
		onResendActivationRequest: vi.fn(),
		onShowPasswordChange: vi.fn(),
		onSwitchAuthMode: vi.fn(),
		...overrides,
	};
	return { props, ...render(<LoginAuthForm {...props} />) };
}

describe("LoginAuthForm", () => {
	it("shows password login, recovery, and registration controls when enabled", () => {
		const onForgotPassword = vi.fn();
		const onResendActivationRequest = vi.fn();
		const onSwitchAuthMode = vi.fn();
		renderForm({
			onForgotPassword,
			onResendActivationRequest,
			onSwitchAuthMode,
		});

		expect(screen.getByLabelText("password")).toHaveAttribute(
			"autocomplete",
			"current-password",
		);
		fireEvent.click(screen.getByRole("button", { name: "forgot_password" }));
		fireEvent.click(screen.getByRole("button", { name: "resend_activation" }));
		fireEvent.click(screen.getByRole("button", { name: "sign_up" }));
		expect(onForgotPassword).toHaveBeenCalledTimes(1);
		expect(onResendActivationRequest).toHaveBeenCalledTimes(1);
		expect(onSwitchAuthMode).toHaveBeenCalledWith("register");
	});

	it("toggles password visibility in both directions", () => {
		const onShowPasswordChange = vi.fn();
		const view = renderForm({ onShowPasswordChange });

		fireEvent.click(screen.getByRole("button", { name: "show_password" }));
		expect(onShowPasswordChange).toHaveBeenCalledWith(true);

		view.rerender(<LoginAuthForm {...view.props} showPassword={true} />);
		fireEvent.click(screen.getByRole("button", { name: "hide_password" }));
		expect(onShowPasswordChange).toHaveBeenLastCalledWith(false);
	});

	it("shows an unavailable state when every login method is disabled", () => {
		renderForm({
			passkeyLoginEnabled: false,
			passwordLoginEnabled: false,
		});

		expect(screen.queryByLabelText("password")).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "forgot_password" }),
		).not.toBeInTheDocument();
		expect(screen.getByText("no_login_methods_available")).toHaveAttribute(
			"role",
			"status",
		);
		expect(
			screen.queryByRole("button", { name: "sign_up" }),
		).not.toBeInTheDocument();
	});

	it("distinguishes supported, unsupported, loading, and external methods", () => {
		const view = renderForm({
			passkeyLoginEnabled: true,
			passkeySupported: true,
			passwordLoginEnabled: false,
		});
		expect(
			screen.getByRole("button", { name: "passkey_sign_in" }),
		).toBeEnabled();
		expect(
			screen.queryByText("no_login_methods_available"),
		).not.toBeInTheDocument();

		view.rerender(
			<LoginAuthForm
				{...view.props}
				passkeyLoginEnabled={true}
				passkeySupported={false}
				passwordLoginEnabled={false}
			/>,
		);
		expect(screen.getByText("passkey_unsupported")).toBeInTheDocument();
		expect(screen.getByText("no_login_methods_available")).toBeInTheDocument();

		view.rerender(
			<LoginAuthForm
				{...view.props}
				externalAuthLoading={true}
				passwordLoginEnabled={false}
			/>,
		);
		expect(
			screen.getByText("external_auth_loading_providers"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("no_login_methods_available"),
		).not.toBeInTheDocument();

		view.rerender(
			<LoginAuthForm
				{...view.props}
				externalAuthProviders={[
					{
						display_name: "Example IDP",
						icon_url: null,
						key: "example",
						kind: "oidc",
					},
				]}
				passwordLoginEnabled={false}
			/>,
		);
		expect(
			screen.getByRole("button", {
				name: "external_auth_sign_in_with Example IDP",
			}),
		).toBeInTheDocument();
		expect(
			screen.queryByText("no_login_methods_available"),
		).not.toBeInTheDocument();
	});

	it("keeps account-creation password fields but omits login-only recovery", () => {
		const view = renderForm({
			mode: "register",
			passwordLoginEnabled: false,
			submitLabel: "sign_up",
		});
		expect(screen.getByLabelText("password")).toHaveAttribute(
			"autocomplete",
			"new-password",
		);
		expect(screen.getByRole("button", { name: "sign_up" })).toHaveAttribute(
			"type",
			"submit",
		);
		expect(
			screen.queryByRole("button", { name: "forgot_password" }),
		).not.toBeInTheDocument();
		expect(screen.getByRole("button", { name: "sign_in" })).toBeInTheDocument();

		view.rerender(
			<LoginAuthForm
				{...view.props}
				mode="setup"
				passwordLoginEnabled={false}
				submitLabel="create_admin"
			/>,
		);
		expect(screen.getByLabelText("password")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "sign_in" }),
		).not.toBeInTheDocument();
	});
});
