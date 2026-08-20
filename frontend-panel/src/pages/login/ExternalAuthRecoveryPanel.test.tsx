import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ExternalAuthRecoveryPanel } from "@/pages/login/ExternalAuthRecoveryPanel";

const validEmailSchema = {
	safeParse: () => ({ success: true }),
};
const invalidEmailSchema = {
	safeParse: () => ({
		error: { issues: [{ message: "invalid-email" }] },
		success: false,
	}),
};

function renderPanel(
	overrides: Partial<
		React.ComponentProps<typeof ExternalAuthRecoveryPanel>
	> = {},
) {
	return render(
		<ExternalAuthRecoveryPanel
			email="verify@example.com"
			emailError=""
			emailSchema={validEmailSchema as never}
			identifier="user@example.com"
			identifierError=""
			mode="password"
			passwordLoginEnabled={true}
			password="secret123"
			passwordError=""
			sent={false}
			submittingEmail={false}
			submittingPassword={false}
			t={(key) => key.replace(/^core:/, "")}
			onBack={vi.fn()}
			onEmailChange={vi.fn()}
			onIdentifierChange={vi.fn()}
			onModeChange={vi.fn()}
			onPasswordChange={vi.fn()}
			{...overrides}
		/>,
	);
}

describe("ExternalAuthRecoveryPanel", () => {
	it("uses a submit button for password linking so Enter submits the parent form", () => {
		renderPanel({ mode: "password" });

		expect(
			screen.getByRole("button", {
				name: /external_auth_password_link_submit/,
			}),
		).toHaveAttribute("type", "submit");
	});

	it("uses a submit button for email verification so Enter submits the parent form", () => {
		renderPanel({ mode: "email" });

		expect(
			screen.getByRole("button", {
				name: /external_auth_email_verification_send/,
			}),
		).toHaveAttribute("type", "submit");
	});

	it("switches recovery modes through tabs", () => {
		const onModeChange = vi.fn();
		const view = renderPanel({ onModeChange });

		fireEvent.click(
			screen.getByRole("tab", {
				name: /external_auth_email_verification_tab/,
			}),
		);
		expect(onModeChange).toHaveBeenCalledWith("email");

		view.rerender(
			<ExternalAuthRecoveryPanel
				email="verify@example.com"
				emailError=""
				emailSchema={validEmailSchema as never}
				identifier="user@example.com"
				identifierError=""
				mode="email"
				passwordLoginEnabled={true}
				password="secret123"
				passwordError=""
				sent={false}
				submittingEmail={false}
				submittingPassword={false}
				t={(key) => key.replace(/^core:/, "")}
				onBack={vi.fn()}
				onEmailChange={vi.fn()}
				onIdentifierChange={vi.fn()}
				onModeChange={onModeChange}
				onPasswordChange={vi.fn()}
			/>,
		);

		fireEvent.click(
			screen.getByRole("tab", {
				name: /external_auth_password_link_tab/,
			}),
		);
		expect(onModeChange).toHaveBeenLastCalledWith("password");
	});

	it("validates email edits and forwards field updates", () => {
		const onEmailChange = vi.fn();
		renderPanel({
			emailSchema: invalidEmailSchema as never,
			mode: "email",
			onEmailChange,
		});

		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: "not-email" },
		});

		expect(onEmailChange).toHaveBeenCalledWith("not-email", "invalid-email");
	});

	it("forwards password-link field edits and the back action", () => {
		const onBack = vi.fn();
		const onIdentifierChange = vi.fn();
		const onPasswordChange = vi.fn();
		renderPanel({
			onBack,
			onIdentifierChange,
			onPasswordChange,
		});

		fireEvent.change(screen.getByLabelText("email_or_username"), {
			target: { value: "updated@example.com" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "new-secret" },
		});
		fireEvent.click(screen.getByRole("button", { name: /back_to_sign_in/ }));

		expect(onIdentifierChange).toHaveBeenCalledWith("updated@example.com");
		expect(onPasswordChange).toHaveBeenCalledWith("new-secret");
		expect(onBack).toHaveBeenCalledTimes(1);
	});

	it("disables recovery submit actions while inputs are incomplete or busy", () => {
		const { rerender } = renderPanel({
			identifier: "",
			mode: "password",
		});

		expect(
			screen.getByRole("button", {
				name: /external_auth_password_link_submit/,
			}),
		).toBeDisabled();

		rerender(
			<ExternalAuthRecoveryPanel
				email=""
				emailError=""
				emailSchema={validEmailSchema as never}
				identifier="user@example.com"
				identifierError=""
				mode="email"
				passwordLoginEnabled={true}
				password="secret123"
				passwordError=""
				sent={false}
				submittingEmail={true}
				submittingPassword={false}
				t={(key) => key.replace(/^core:/, "")}
				onBack={vi.fn()}
				onEmailChange={vi.fn()}
				onIdentifierChange={vi.fn()}
				onModeChange={vi.fn()}
				onPasswordChange={vi.fn()}
			/>,
		);

		expect(
			screen.getByRole("button", {
				name: /external_auth_email_verification_sending/,
			}),
		).toBeDisabled();

		rerender(
			<ExternalAuthRecoveryPanel
				email="not-email"
				emailError="invalid-email"
				emailSchema={validEmailSchema as never}
				identifier="user@example.com"
				identifierError=""
				mode="email"
				passwordLoginEnabled={true}
				password="secret123"
				passwordError=""
				sent={false}
				submittingEmail={false}
				submittingPassword={false}
				t={(key) => key.replace(/^core:/, "")}
				onBack={vi.fn()}
				onEmailChange={vi.fn()}
				onIdentifierChange={vi.fn()}
				onModeChange={vi.fn()}
				onPasswordChange={vi.fn()}
			/>,
		);

		expect(
			screen.getByRole("button", {
				name: /external_auth_email_verification_send/,
			}),
		).toBeDisabled();
	});

	it("shows sent email verification state and hides recovery fields", () => {
		renderPanel({
			email: "sent@example.com",
			mode: "email",
			sent: true,
		});

		expect(
			screen.getByText("external_auth_email_verification_sent_title"),
		).toBeInTheDocument();
		expect(screen.getByText("email: sent@example.com")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", {
				name: /external_auth_email_verification_send/,
			}),
		).not.toBeInTheDocument();
	});

	it("hides password linking when password login is disabled", () => {
		renderPanel({
			mode: "email",
			passwordLoginEnabled: false,
		});

		expect(
			screen.queryByRole("tab", {
				name: /external_auth_password_link_tab/,
			}),
		).not.toBeInTheDocument();
		expect(
			screen.getByRole("tab", {
				name: /external_auth_email_verification_tab/,
			}),
		).toBeInTheDocument();
		expect(
			screen.queryByLabelText("email_or_username"),
		).not.toBeInTheDocument();
	});
});
