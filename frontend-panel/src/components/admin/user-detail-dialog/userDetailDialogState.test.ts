import { describe, expect, it } from "vitest";
import { userDetailDraftKey } from "./userDetailDialogState";

describe("userDetailDraftKey", () => {
	it("captures user fields that should reset the detail draft", () => {
		expect(
			userDetailDraftKey({
				created_at: "2026-06-01T00:00:00Z",
				email: "alice@example.com",
				email_verified: true,
				id: 42,
				must_change_password: true,
				policy_group_id: 7,
				role: "admin",
				status: "active",
				storage_quota: 1024,
				storage_used: 128,
				updated_at: "2026-06-01T00:00:00Z",
				username: "alice",
			}),
		).toBe("42:verified:must-change-password:7:admin:active:1024");
	});

	it("uses stable fallback segments for nullable draft inputs", () => {
		expect(
			userDetailDraftKey({
				created_at: "2026-06-01T00:00:00Z",
				email: "bob@example.com",
				email_verified: false,
				id: 43,
				must_change_password: false,
				policy_group_id: null,
				role: "user",
				status: "disabled",
				storage_quota: null,
				storage_used: 0,
				updated_at: "2026-06-01T00:00:00Z",
				username: "bob",
			}),
		).toBe("43:unverified:password-ok:none:user:disabled:0");
	});
});
