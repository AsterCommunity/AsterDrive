import { describe, expect, it } from "vitest";
import { isTokenAuthError } from "@/lib/authErrors";
import { ErrorCode } from "@/types/api-helpers";

describe("isTokenAuthError", () => {
	it("returns false for primitive errors and responses without API data", () => {
		expect(isTokenAuthError(null)).toBe(false);
		expect(isTokenAuthError("token expired")).toBe(false);
		expect(isTokenAuthError({ response: null })).toBe(false);
		expect(isTokenAuthError({ response: { data: "bad" } })).toBe(false);
	});

	it("detects token auth errors from direct and nested API codes", () => {
		expect(isTokenAuthError({ code: ErrorCode.TokenExpired })).toBe(true);
		expect(isTokenAuthError({ code: ErrorCode.TokenMissing })).toBe(true);
		expect(
			isTokenAuthError({
				response: {
					data: {
						code: ErrorCode.TokenInvalid,
					},
				},
			}),
		).toBe(true);
		expect(isTokenAuthError({ code: ErrorCode.InvalidCredentials })).toBe(
			false,
		);
	});
});
