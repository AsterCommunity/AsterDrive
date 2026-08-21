import type { CheckResp, ExternalAuthPublicProvider } from "@/types/api";

export interface AuthPolicySnapshot {
	allowUserRegistration: boolean;
	checkedAt: number;
	externalProviders: ExternalAuthPublicProvider[];
	passkeyLoginEnabled: boolean;
	passwordLoginEnabled: boolean;
}

export interface AuthBootstrapSnapshot {
	check: CheckResp | null;
	checkError: unknown;
	policy: AuthPolicySnapshot;
	providersError: unknown;
}

interface AuthPolicyLoader {
	check(): Promise<CheckResp>;
	listExternalAuthProviders(): Promise<ExternalAuthPublicProvider[]>;
}

export function createAuthRequestCoordinator() {
	let revision = 0;
	return {
		begin() {
			revision += 1;
			return revision;
		},
		invalidate() {
			revision += 1;
		},
		isCurrent(candidate: number) {
			return candidate === revision;
		},
	};
}

export async function loadAuthBootstrap(
	loader: AuthPolicyLoader,
	fallback: Pick<
		AuthPolicySnapshot,
		"passkeyLoginEnabled" | "passwordLoginEnabled"
	>,
	now = Date.now(),
): Promise<AuthBootstrapSnapshot> {
	const [checkResult, providersResult] = await Promise.allSettled([
		loader.check(),
		loader.listExternalAuthProviders(),
	]);
	const check = checkResult.status === "fulfilled" ? checkResult.value : null;
	const externalProviders =
		providersResult.status === "fulfilled" ? providersResult.value : [];

	return {
		check,
		checkError: checkResult.status === "rejected" ? checkResult.reason : null,
		policy: {
			allowUserRegistration: check?.allow_user_registration !== false,
			checkedAt: now,
			externalProviders,
			passkeyLoginEnabled:
				check?.passkey_login_enabled !== false &&
				(check ? true : fallback.passkeyLoginEnabled),
			passwordLoginEnabled:
				check?.password_login_enabled !== false &&
				(check ? true : fallback.passwordLoginEnabled),
		},
		providersError:
			providersResult.status === "rejected" ? providersResult.reason : null,
	};
}
