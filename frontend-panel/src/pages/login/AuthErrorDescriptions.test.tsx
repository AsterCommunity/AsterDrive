import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MfaChallengePanel } from "./MfaChallengePanel";
import { PasswordResetRequestPanel } from "./PasswordResetRequestPanel";

describe("authentication error descriptions", () => {
	it("links and clears the MFA error", () => {
		const props = {
			code: "",
			emailCodeError: "",
			emailCodeExpiresAt: null,
			emailCodeResendAt: 0,
			emailCodeSending: false,
			emailCodeSent: false,
			expired: false,
			methods: ["totp"] as const,
			remainingSeconds: 60,
			selectedMethod: "totp" as const,
			submitting: false,
			t: (key: string) => key,
			onBack: vi.fn(),
			onCodeChange: vi.fn(),
			onEmailCodeSend: vi.fn(),
			onMethodChange: vi.fn(),
		};
		const { rerender } = render(
			<MfaChallengePanel
				{...props}
				methods={[...props.methods]}
				error="bad code"
			/>,
		);
		const field = screen.getByLabelText("mfa_totp_code_label");
		expect(field).toHaveAccessibleDescription("bad code");
		rerender(
			<MfaChallengePanel {...props} methods={[...props.methods]} error="" />,
		);
		expect(field).not.toHaveAttribute("aria-describedby");
	});

	it("links and clears the password reset request error", () => {
		const props = {
			emailSchema: { safeParse: () => ({ success: true }) } as never,
			passwordResetEmail: "",
			requestingPasswordReset: false,
			t: (key: string) => key,
			onBack: vi.fn(),
			onEmailChange: vi.fn(),
			onSubmit: vi.fn(),
		};
		const { rerender } = render(
			<PasswordResetRequestPanel {...props} passwordResetError="bad email" />,
		);
		const field = screen.getByLabelText("core:email");
		expect(field).toHaveAccessibleDescription("bad email");
		rerender(<PasswordResetRequestPanel {...props} passwordResetError="" />);
		expect(field).not.toHaveAttribute("aria-describedby");
	});
});
