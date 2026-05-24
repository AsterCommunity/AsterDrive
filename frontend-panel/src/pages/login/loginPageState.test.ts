import { describe, expect, it, vi } from "vitest";
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
			error: "",
			kind: "mfa",
			now: Date.now(),
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
});
