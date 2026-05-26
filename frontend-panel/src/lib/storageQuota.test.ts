import { describe, expect, it } from "vitest";
import {
	BYTES_PER_MB,
	MAX_SAFE_STORAGE_QUOTA_MB,
	parseStorageQuotaMbToBytes,
} from "@/lib/storageQuota";

describe("storageQuota", () => {
	it("converts whole megabytes to bytes", () => {
		expect(parseStorageQuotaMbToBytes("20")).toBe(20 * BYTES_PER_MB);
		expect(parseStorageQuotaMbToBytes("0")).toBe(0);
	});

	it("rejects non-integer and overflowing megabyte values", () => {
		expect(parseStorageQuotaMbToBytes("1.5")).toBeNull();
		expect(parseStorageQuotaMbToBytes("999999999999999999999999")).toBeNull();
		expect(
			parseStorageQuotaMbToBytes(String(MAX_SAFE_STORAGE_QUOTA_MB + 1)),
		).toBeNull();
	});
});
