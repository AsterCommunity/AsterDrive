import type { ExternalAuthProviderKind } from "@/types/api";

export function externalAuthKindIconPath(kind: ExternalAuthProviderKind) {
	switch (kind) {
		case "oidc":
			return "/static/external-auth/openid-seeklogo.svg";
	}
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
