import type { ExternalAuthProviderKind } from "@/types/api";

export function externalAuthKindIconPath(
	kind: ExternalAuthProviderKind,
): string {
	switch (kind) {
		case "generic_oauth2":
			return "/static/external-auth/oauth-logo.svg";
		case "github":
			return "/static/external-auth/github-logo.svg";
		case "oidc":
			return "/static/external-auth/openid-seeklogo.svg";
	}
	return "";
}

export function normalizeExternalAuthIconUrl(
	iconUrl: string | null | undefined,
) {
	const normalized = iconUrl?.trim();
	if (!normalized) return "";
	if (
		normalized.startsWith("/") &&
		!normalized.startsWith("//") &&
		!/\s/.test(normalized)
	) {
		return normalized;
	}

	try {
		const parsed = new URL(normalized);
		if (parsed.protocol === "http:" || parsed.protocol === "https:") {
			return parsed.toString();
		}
	} catch {
		return "";
	}

	return "";
}
