import { describe, expect, it } from "vitest";
import {
	BYTE_FILTER_UNITS,
	formatByteSizeInput,
	parseByteSizeInput,
} from "@/lib/byteSizeFilter";

describe("byte size filters", () => {
	it("converts binary units and distinguishes empty, zero and invalid input", () => {
		expect(parseByteSizeInput("1.5", "MB")).toBe(1572864);
		expect(parseByteSizeInput("0", "B")).toBe(0);
		expect(parseByteSizeInput("", "GB")).toBeUndefined();
		for (const text of ["-1", "x", "1e3", "Infinity", "0.1"])
			expect(parseByteSizeInput(text, "B")).toBeNull();
		expect(
			parseByteSizeInput(String(Number.MAX_SAFE_INTEGER), "TB"),
		).toBeNull();
	});
	it("round-trips every unit without losing a byte", () => {
		for (const unit of BYTE_FILTER_UNITS)
			for (const bytes of [0, 1, 1023, 1024, 1572864, Number.MAX_SAFE_INTEGER])
				expect(
					parseByteSizeInput(
						formatByteSizeInput(bytes, unit.value),
						unit.value,
					),
				).toBe(bytes);
	});
});
