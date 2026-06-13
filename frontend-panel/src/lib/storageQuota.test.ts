import { describe, expect, it } from "vitest";
import {
	BYTES_PER_MB,
	formatStorageQuotaDraft,
	MAX_SAFE_STORAGE_QUOTA_MB,
	parseStorageQuotaMbToBytes,
	parseStorageQuotaValueToBytes,
	storageQuotaDraftIsValid,
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

	it("formats quota bytes using the largest exact unit and treats unlimited as blank megabytes", () => {
		expect(formatStorageQuotaDraft(0)).toEqual({
			unit: "megabytes",
			value: "",
		});
		expect(formatStorageQuotaDraft(null)).toEqual({
			unit: "megabytes",
			value: "",
		});
		expect(formatStorageQuotaDraft(1024)).toEqual({
			unit: "kilobytes",
			value: "1",
		});
		expect(formatStorageQuotaDraft(20 * BYTES_PER_MB)).toEqual({
			unit: "megabytes",
			value: "20",
		});
		expect(formatStorageQuotaDraft(3 * 1024 ** 3)).toEqual({
			unit: "gigabytes",
			value: "3",
		});
		expect(formatStorageQuotaDraft(2 * 1024 ** 4)).toEqual({
			unit: "terabytes",
			value: "2",
		});
		expect(formatStorageQuotaDraft(1536)).toEqual({
			unit: "bytes",
			value: "1536",
		});
	});

	it("parses quota drafts for every supported unit", () => {
		expect(parseStorageQuotaValueToBytes("", "megabytes")).toBe(0);
		expect(parseStorageQuotaValueToBytes("0", "gigabytes")).toBe(0);
		expect(parseStorageQuotaValueToBytes("512", "bytes")).toBe(512);
		expect(parseStorageQuotaValueToBytes("2", "kilobytes")).toBe(2048);
		expect(parseStorageQuotaValueToBytes("4", "megabytes")).toBe(
			4 * BYTES_PER_MB,
		);
		expect(parseStorageQuotaValueToBytes("5", "gigabytes")).toBe(5 * 1024 ** 3);
		expect(parseStorageQuotaValueToBytes("6", "terabytes")).toBe(6 * 1024 ** 4);
	});

	it("validates quota drafts with zod and rejects invalid or overflowing drafts", () => {
		expect(storageQuotaDraftIsValid("", "megabytes")).toBe(true);
		expect(storageQuotaDraftIsValid("7", "megabytes")).toBe(true);
		expect(storageQuotaDraftIsValid("1.5", "megabytes")).toBe(false);
		expect(storageQuotaDraftIsValid("-1", "megabytes")).toBe(false);
		expect(storageQuotaDraftIsValid("1e3", "megabytes")).toBe(false);
		expect(storageQuotaDraftIsValid("abc", "megabytes")).toBe(false);
		expect(
			storageQuotaDraftIsValid(String(Number.MAX_SAFE_INTEGER), "terabytes"),
		).toBe(false);
		expect(
			parseStorageQuotaValueToBytes(
				String(Number.MAX_SAFE_INTEGER),
				"terabytes",
			),
		).toBeNull();
	});
});
