import { describe, expect, it } from "vitest";
import { onedriveCredentialStatusReasonKey } from "./onedriveCredentialReason";

describe("onedriveCredentialStatusReasonKey", () => {
	it.each([
		[null, null],
		[undefined, null],
		["   ", null],
		[
			"storage credential is missing refresh token; reauthorize Microsoft Graph",
			"onedrive_credential_reason_missing_refresh_token",
		],
		[
			"Microsoft Graph OAuth token exchange failed: invalid_grant",
			"onedrive_credential_reason_invalid_grant",
		],
		[
			"INVALID_CLIENT: client secret is invalid",
			"onedrive_credential_reason_invalid_client",
		],
		[
			"Microsoft Graph OAuth token response missing access_token",
			"onedrive_credential_reason_missing_access_token",
		],
		[
			"Microsoft Graph OAuth token response missing access token",
			"onedrive_credential_reason_missing_access_token",
		],
		[
			"drive resolution failed after authorization",
			"onedrive_credential_reason_drive_resolution_failed",
		],
		[
			"OneDrive target could not be resolved for this account",
			"onedrive_credential_reason_drive_resolution_failed",
		],
		[
			"failed to resolve OneDrive location",
			"onedrive_credential_reason_drive_resolution_failed",
		],
		[
			"provider returned a long diagnostic message",
			"onedrive_credential_reason_reauth_required",
		],
	])("maps %s to %s", (reason, expected) => {
		expect(onedriveCredentialStatusReasonKey(reason)).toBe(expected);
	});
});
