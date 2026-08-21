import { describe, expect, it } from "vitest";
import {
	clearAuthFlowRedirectSearch,
	parseRecoverableAuthUrlFlow,
} from "./authUrlAdapter";

describe("auth URL adapter", () => {
	it("parses a typed MFA reference and deduplicates supported methods", () => {
		expect(
			parseRecoverableAuthUrlFlow(
				"?mfa=required&flow=abc&expires_in=60.9&methods=totp,bad,totp,email_code&return_path=%2Ffiles",
				1_000,
			),
		).toEqual({
			expiresAt: 61_000,
			flowToken: "abc",
			kind: "mfa",
			methods: ["totp", "email_code"],
			returnPath: "/files",
		});
	});

	it("bounds untrusted TTL and return path values", () => {
		expect(
			parseRecoverableAuthUrlFlow(
				"?mfa=required&flow=abc&expires_in=999999&return_path=https%3A%2F%2Fevil.example",
				0,
			),
		).toMatchObject({ expiresAt: 600_000, returnPath: "/" });
		expect(
			parseRecoverableAuthUrlFlow(
				"?external_auth=email_required&flow=abc&return_path=%2F%2Fevil.example",
			),
		).toMatchObject({ kind: "external-auth-recovery", returnPath: "/" });
	});

	it("rejects missing references and unsupported statuses", () => {
		expect(parseRecoverableAuthUrlFlow("?mfa=required")).toBeNull();
		expect(parseRecoverableAuthUrlFlow("?mfa=success&flow=abc")).toBeNull();
	});

	it("removes flow transport fields while preserving unrelated query state", () => {
		expect(
			clearAuthFlowRedirectSearch(
				"?mfa=required&flow=abc&methods=totp&theme=dark",
			),
		).toBe("?theme=dark");
	});
});
