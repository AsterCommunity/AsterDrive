import { describe, expect, it } from "vitest";
import {
	ApiErrorCode,
	ApiSubcode,
	isApiErrorCode,
	isApiSubcode,
} from "@/types/api-helpers";

describe("ApiSubcode helpers", () => {
	it("accepts every runtime ApiSubcode constant", () => {
		for (const subcode of Object.values(ApiSubcode)) {
			expect(isApiSubcode(subcode)).toBe(true);
		}
	});

	it("keeps ApiSubcode runtime values unique", () => {
		const values = Object.values(ApiSubcode);

		expect(new Set(values).size).toBe(values.length);
	});

	it.each([
		"",
		"ArchivePreviewDisabled",
		"archive_preview.future_value",
		"remote.dynamic",
		"file.created",
	])("rejects non-generated or non-error subcode value %s", (value) => {
		expect(isApiSubcode(value)).toBe(false);
	});
});

describe("ApiErrorCode helpers", () => {
	it("accepts every runtime ApiErrorCode constant", () => {
		for (const code of Object.values(ApiErrorCode)) {
			expect(isApiErrorCode(code)).toBe(true);
		}
	});

	it("keeps ApiErrorCode runtime values unique", () => {
		const values = Object.values(ApiErrorCode);

		expect(new Set(values).size).toBe(values.length);
	});

	it("covers every legacy ApiSubcode during the transition period", () => {
		const codes = new Set(Object.values(ApiErrorCode));

		for (const subcode of Object.values(ApiSubcode)) {
			expect(codes.has(subcode)).toBe(true);
		}
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
