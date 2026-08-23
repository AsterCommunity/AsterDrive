import { describe, expect, it } from "vitest";
import { createAuthCommandCoordinator } from "./authCommandCoordinator";
import { initialAuthUiFlow } from "./loginPageState";

describe("auth command coordinator", () => {
	it("owns the single flow reducer and rejects stale async commands", () => {
		const coordinator = createAuthCommandCoordinator(initialAuthUiFlow);
		const first = coordinator.begin();
		coordinator.dispatch({
			type: "open_mfa",
			challenge: {
				expiresAt: Date.now() + 300_000,
				flowToken: "mfa-flow",
				methods: ["totp"],
				returnPath: "/",
				successMessage: "signed in",
			},
		});
		const second = coordinator.begin();
		expect(coordinator.isCurrent(first)).toBe(false);
		expect(coordinator.isCurrent(second)).toBe(true);
		coordinator.dispatch({ type: "open_auth" }, first);
		expect(coordinator.state().kind).toBe("mfa");
		coordinator.dispatch({ type: "open_auth" }, second);
		expect(coordinator.state().kind).toBe("login");
		coordinator.cancel();
		expect(coordinator.isCurrent(second)).toBe(false);
	});

	it("serializes top-level mode changes through the same command boundary", () => {
		const coordinator = createAuthCommandCoordinator({ kind: "login" });
		coordinator.dispatch({ type: "switch_auth_mode", mode: "register" });
		expect(coordinator.state()).toEqual({ kind: "register" });
		coordinator.dispatch({
			type: "open_password_reset",
			email: "a@example.com",
		});
		expect(coordinator.state().kind).toBe("password-reset");
	});
});
