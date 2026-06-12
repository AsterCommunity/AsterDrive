import { describe, expect, it } from "vitest";
import type { ApiErrorCode as GeneratedApiErrorCode } from "@/types/api";
import { ApiErrorCode, isApiErrorCode } from "@/types/api-helpers";

type RuntimeApiErrorCode = (typeof ApiErrorCode)[keyof typeof ApiErrorCode];
type MissingGeneratedApiErrorCodes = Exclude<
	GeneratedApiErrorCode,
	RuntimeApiErrorCode
>;
type ExtraRuntimeApiErrorCodes = Exclude<
	RuntimeApiErrorCode,
	GeneratedApiErrorCode
>;
type AssertNever<T extends never> = T;

const apiErrorCodeCoverageCheck: [
	AssertNever<MissingGeneratedApiErrorCodes>,
	AssertNever<ExtraRuntimeApiErrorCodes>,
] = [] as never;

describe("ApiErrorCode helpers", () => {
	it("covers every generated API error code without extra runtime values", () => {
		expect(apiErrorCodeCoverageCheck).toHaveLength(0);
	});

	it("accepts every runtime ApiErrorCode constant", () => {
		for (const code of Object.values(ApiErrorCode)) {
			expect(isApiErrorCode(code)).toBe(true);
		}
	});

	it("keeps ApiErrorCode runtime values unique", () => {
		const values = Object.values(ApiErrorCode);

		expect(new Set(values).size).toBe(values.length);
	});

	it.each([
		"",
		"AuthFailed",
		"StorageTransient",
		"auth.failed ",
		" auth.failed",
		"AUTH.FAILED",
		"auth_failed",
		"2000",
		"remote.dynamic",
		"storage.remote_permission",
	])("rejects non-generated or non-error API error code value %s", (value) => {
		expect(isApiErrorCode(value)).toBe(false);
	});
});
