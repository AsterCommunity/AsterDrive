export function onedriveCredentialStatusReasonKey(
	reason: string | null | undefined,
) {
	const normalized = reason?.trim().toLowerCase();
	if (!normalized) return null;
	if (normalized.includes("missing refresh token")) {
		return "onedrive_credential_reason_missing_refresh_token";
	}
	if (normalized.includes("invalid_grant")) {
		return "onedrive_credential_reason_invalid_grant";
	}
	if (normalized.includes("invalid_client")) {
		return "onedrive_credential_reason_invalid_client";
	}
	if (
		normalized.includes("missing access_token") ||
		normalized.includes("missing access token")
	) {
		return "onedrive_credential_reason_missing_access_token";
	}
	if (
		normalized.includes("drive resolution failed") ||
		normalized.includes("onedrive target could not be resolved") ||
		normalized.includes("resolve onedrive")
	) {
		return "onedrive_credential_reason_drive_resolution_failed";
	}
	return "onedrive_credential_reason_reauth_required";
}
