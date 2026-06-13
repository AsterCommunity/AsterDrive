import { afterEach, describe, expect, it, vi } from "vitest";
import {
	type AuthPanelState,
	authPanelReducer,
	initialAuthPanelState,
	type MfaChallengeState,
} from "./loginPageState";

function mfaChallenge(
	overrides: Partial<MfaChallengeState> = {},
): MfaChallengeState {
	return {
		expiresAt: Date.now() + 300_000,
		flowToken: "mfa-flow",
		methods: ["totp", "recovery_code"],
		returnPath: "/",
		successMessage: "signed in",
		...overrides,
	};
}

describe("authPanelReducer", () => {
	afterEach(() => {
		vi.useRealTimers();
	});

	it("ignores password reset edits outside the password reset panel", () => {
		const sameState = authPanelReducer(initialAuthPanelState, {
			type: "set_password_reset_email",
			email: "alice@example.com",
			error: "",
		});

		expect(sameState).toBe(initialAuthPanelState);

		const mfaState: AuthPanelState = {
			challenge: mfaChallenge(),
			code: "",
			emailCodeError: "",
			emailCodeExpiresAt: null,
			emailCodeResendAt: 0,
			emailCodeSending: false,
			emailCodeSent: false,
			error: "",
			kind: "mfa",
			now: Date.now(),
			selectedMethod: "totp",
			submitting: false,
		};

		expect(
			authPanelReducer(mfaState, {
				type: "set_password_reset_error",
				error: "invalid-email",
			}),
		).toBe(mfaState);
	});

	it("updates password reset email and error while the panel is active", () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-05-24T08:00:00.000Z"));

		const opened = authPanelReducer(initialAuthPanelState, {
			type: "open_password_reset",
			email: "old@example.com",
		});

		const edited = authPanelReducer(opened, {
			type: "set_password_reset_email",
			email: "new@example.com",
			error: "invalid-email",
		});

		expect(edited).toEqual({
			kind: "password-reset",
			passwordReset: {
				email: "new@example.com",
				error: "invalid-email",
				requesting: false,
			},
		});

		expect(
			authPanelReducer(edited, {
				type: "set_password_reset_error",
				error: "",
			}),
		).toEqual({
			kind: "password-reset",
			passwordReset: {
				email: "new@example.com",
				error: "",
				requesting: false,
			},
		});
	});

	it("updates activation resend email and request lifecycle while the panel is active", () => {
		const opened = authPanelReducer(initialAuthPanelState, {
			type: "open_activation_resend",
			email: "old@example.com",
		});

		const edited = authPanelReducer(opened, {
			type: "set_activation_resend_email",
			email: "new@example.com",
			error: "invalid-email",
		});

		expect(edited).toEqual({
			activationResend: {
				email: "new@example.com",
				error: "invalid-email",
				requesting: false,
			},
			kind: "activation-resend",
		});

		const requesting = authPanelReducer(edited, {
			type: "set_activation_resend_requesting",
			requesting: true,
		});

		expect(requesting).toMatchObject({
			activationResend: {
				email: "new@example.com",
				error: "invalid-email",
				requesting: true,
			},
			kind: "activation-resend",
		});

		expect(
			authPanelReducer(requesting, {
				type: "set_activation_resend_error",
				error: "",
			}),
		).toMatchObject({
			activationResend: {
				error: "",
			},
		});
	});

	it("closes the activation resend panel back to auth", () => {
		const opened = authPanelReducer(initialAuthPanelState, {
			type: "open_activation_resend",
			email: "old@example.com",
		});

		expect(authPanelReducer(opened, { type: "close_activation_resend" })).toBe(
			initialAuthPanelState,
		);
	});

	it("ignores activation resend edits outside the activation resend panel", () => {
		expect(
			authPanelReducer(initialAuthPanelState, {
				type: "set_activation_resend_email",
				email: "new@example.com",
				error: "",
			}),
		).toBe(initialAuthPanelState);
		expect(
			authPanelReducer(initialAuthPanelState, {
				type: "set_activation_resend_error",
				error: "invalid-email",
			}),
		).toBe(initialAuthPanelState);
		expect(
			authPanelReducer(initialAuthPanelState, {
				type: "set_activation_resend_requesting",
				requesting: true,
			}),
		).toBe(initialAuthPanelState);
	});

	it("opens email-only MFA challenges with email selected", () => {
		const opened = authPanelReducer(initialAuthPanelState, {
			type: "open_mfa",
			challenge: mfaChallenge({ methods: ["email_code"] }),
		});

		expect(opened).toMatchObject({
			code: "",
			emailCodeError: "",
			emailCodeExpiresAt: null,
			emailCodeResendAt: 0,
			emailCodeSending: false,
			emailCodeSent: false,
			error: "",
			kind: "mfa",
			selectedMethod: "email_code",
			submitting: false,
		});
	});

	it("falls back through recovery-code and totp MFA initial methods", () => {
		expect(
			authPanelReducer(initialAuthPanelState, {
				type: "open_mfa",
				challenge: mfaChallenge({ methods: ["recovery_code"] }),
			}),
		).toMatchObject({
			kind: "mfa",
			selectedMethod: "recovery_code",
		});
		expect(
			authPanelReducer(initialAuthPanelState, {
				type: "open_mfa",
				challenge: mfaChallenge({ methods: [] }),
			}),
		).toMatchObject({
			kind: "mfa",
			selectedMethod: "totp",
		});
	});

	it("ignores unavailable MFA methods and clears code when switching methods", () => {
		const state: AuthPanelState = {
			challenge: mfaChallenge({ methods: ["totp", "email_code"] }),
			code: "123456",
			emailCodeError: "",
			emailCodeExpiresAt: null,
			emailCodeResendAt: 0,
			emailCodeSending: false,
			emailCodeSent: false,
			error: "bad code",
			kind: "mfa",
			now: Date.now(),
			selectedMethod: "totp",
			submitting: false,
		};

		expect(
			authPanelReducer(state, {
				type: "set_mfa_method",
				method: "recovery_code",
			}),
		).toBe(state);

		expect(
			authPanelReducer(state, {
				type: "set_mfa_method",
				method: "email_code",
			}),
		).toEqual({
			...state,
			code: "",
			error: "",
			selectedMethod: "email_code",
		});
	});

	it("tracks email MFA send lifecycle and ignores it outside MFA", () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-05-24T08:00:00.000Z"));
		const now = Date.now();
		const opened = authPanelReducer(initialAuthPanelState, {
			type: "open_mfa",
			challenge: mfaChallenge({ methods: ["email_code"] }),
		});

		const sending = authPanelReducer(opened, {
			type: "set_mfa_email_code_sending",
			sending: true,
		});
		expect(sending).toMatchObject({
			emailCodeError: "",
			emailCodeSending: true,
		});

		const sent = authPanelReducer(sending, {
			type: "set_mfa_email_code_sent",
			expiresIn: 600,
			now,
			resendAfter: 60,
		});
		expect(sent).toMatchObject({
			emailCodeError: "",
			emailCodeExpiresAt: now + 600_000,
			emailCodeResendAt: now + 60_000,
			emailCodeSending: false,
			emailCodeSent: true,
		});

		expect(
			authPanelReducer(initialAuthPanelState, {
				type: "set_mfa_email_code_sending",
				sending: true,
			}),
		).toBe(initialAuthPanelState);
	});
});
