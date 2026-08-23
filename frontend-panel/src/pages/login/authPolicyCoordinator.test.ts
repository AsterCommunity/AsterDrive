import { describe, expect, it } from "vitest";
import type { CheckResp, ExternalAuthPublicProvider } from "@/types/api";
import {
	createAuthRequestCoordinator,
	loadAuthBootstrap,
} from "./authPolicyCoordinator";

function check(overrides: Partial<CheckResp> = {}): CheckResp {
	return {
		allow_user_registration: true,
		has_users: true,
		passkey_login_enabled: true,
		password_login_enabled: true,
		setup_state: "ready",
		...overrides,
	};
}

describe("auth policy coordinator", () => {
	it("combines the authoritative check and provider list", async () => {
		const provider = {
			key: "company",
			kind: "oidc",
		} as ExternalAuthPublicProvider;
		const result = await loadAuthBootstrap(
			{
				check: async () =>
					check({
						allow_user_registration: false,
						password_login_enabled: false,
					}),
				listExternalAuthProviders: async () => [provider],
			},
			{ passkeyLoginEnabled: false, passwordLoginEnabled: true },
			42,
		);

		expect(result.policy).toEqual({
			allowUserRegistration: false,
			checkedAt: 42,
			externalProviders: [provider],
			passkeyLoginEnabled: true,
			passwordLoginEnabled: false,
		});
	});

	it("keeps provider failure separate from the login policy", async () => {
		const error = new Error("provider endpoint down");
		const result = await loadAuthBootstrap(
			{
				check: async () => check({ passkey_login_enabled: false }),
				listExternalAuthProviders: async () => Promise.reject(error),
			},
			{ passkeyLoginEnabled: true, passwordLoginEnabled: true },
		);

		expect(result.check).not.toBeNull();
		expect(result.policy.externalProviders).toEqual([]);
		expect(result.policy.passkeyLoginEnabled).toBe(false);
		expect(result.providersError).toBe(error);
	});

	it("uses public policy fallback only when auth check fails", async () => {
		const error = new Error("check failed");
		const result = await loadAuthBootstrap(
			{
				check: async () => Promise.reject(error),
				listExternalAuthProviders: async () => [],
			},
			{ passkeyLoginEnabled: false, passwordLoginEnabled: true },
		);

		expect(result.policy.passkeyLoginEnabled).toBe(false);
		expect(result.policy.passwordLoginEnabled).toBe(true);
		expect(result.checkError).toBe(error);
	});

	it("invalidates stale and unmounted request generations", () => {
		const coordinator = createAuthRequestCoordinator();
		const first = coordinator.begin();
		const second = coordinator.begin();
		expect(coordinator.isCurrent(first)).toBe(false);
		expect(coordinator.isCurrent(second)).toBe(true);
		coordinator.invalidate();
		expect(coordinator.isCurrent(second)).toBe(false);
	});
});
