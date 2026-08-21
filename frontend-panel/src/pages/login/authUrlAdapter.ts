import type { MfaMethod } from "@/services/authService";

const MFA_METHODS: MfaMethod[] = ["totp", "recovery_code", "email_code"];
const DEFAULT_MFA_TTL_SECONDS = 300;
const MAX_MFA_TTL_SECONDS = 600;

export type RecoverableAuthUrlFlow =
	| {
			flowToken: string;
			kind: "external-auth-recovery";
			returnPath: string;
	  }
	| {
			expiresAt: number;
			flowToken: string;
			kind: "mfa";
			methods: MfaMethod[];
			returnPath: string;
	  };

export function parseRecoverableAuthUrlFlow(
	search: string,
	now = Date.now(),
): RecoverableAuthUrlFlow | null {
	const params = new URLSearchParams(search);
	const flowToken = params.get("flow")?.trim();
	if (!flowToken) return null;
	const returnPath = normalizeReturnPath(params.get("return_path"));

	if (params.get("mfa") === "required") {
		return {
			expiresAt: now + parseMfaTtlSeconds(params.get("expires_in")) * 1000,
			flowToken,
			kind: "mfa",
			methods: parseMfaMethods(params.get("methods")),
			returnPath,
		};
	}
	if (params.get("external_auth") === "email_required") {
		return {
			flowToken,
			kind: "external-auth-recovery",
			returnPath,
		};
	}
	return null;
}

export function clearAuthFlowRedirectSearch(search: string) {
	const params = new URLSearchParams(search);
	for (const key of [
		"external_auth",
		"mfa",
		"code",
		"message",
		"invitation",
		"flow",
		"expires_in",
		"methods",
		"return_path",
	]) {
		params.delete(key);
	}
	const cleaned = params.toString();
	return cleaned ? `?${cleaned}` : "";
}

function normalizeReturnPath(value: string | null) {
	if (!value?.startsWith("/") || value.startsWith("//")) return "/";
	return value;
}

function parseMfaMethods(value: string | null): MfaMethod[] {
	if (!value) return ["totp", "recovery_code"];
	const methods = value
		.split(",")
		.map((method) => method.trim())
		.filter((method): method is MfaMethod =>
			MFA_METHODS.includes(method as MfaMethod),
		);
	return methods.length > 0 ? [...new Set(methods)] : ["totp", "recovery_code"];
}

function parseMfaTtlSeconds(value: string | null) {
	const parsed = Number(value);
	if (!Number.isFinite(parsed) || parsed <= 0) return DEFAULT_MFA_TTL_SECONDS;
	return Math.min(Math.floor(parsed), MAX_MFA_TTL_SECONDS);
}
