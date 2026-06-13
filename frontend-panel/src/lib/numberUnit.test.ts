import { describe, expect, it } from "vitest";
import {
	convertNumberUnitValueToBaseUnit,
	numberUnitValueIsValid,
	parseNumberUnitValue,
} from "@/lib/numberUnit";

const unit = {
	labelKey: "unit",
	multiplier: 1024,
	value: "kb",
};

describe("numberUnit", () => {
	it("parses blank and non-negative whole-number drafts", () => {
		expect(parseNumberUnitValue("")).toBe(0);
		expect(parseNumberUnitValue("   ")).toBe(0);
		expect(parseNumberUnitValue("0")).toBe(0);
		expect(parseNumberUnitValue("42")).toBe(42);
		expect(parseNumberUnitValue(" 42 ")).toBe(42);
	});

	it("rejects decimal, negative, exponent, and unsafe drafts", () => {
		expect(parseNumberUnitValue("1.5")).toBeNull();
		expect(parseNumberUnitValue("-1")).toBeNull();
		expect(parseNumberUnitValue("1e3")).toBeNull();
		expect(parseNumberUnitValue("abc")).toBeNull();
		expect(
			parseNumberUnitValue(String(Number.MAX_SAFE_INTEGER + 1)),
		).toBeNull();
		expect(numberUnitValueIsValid("1.5")).toBe(false);
		expect(numberUnitValueIsValid("1")).toBe(true);
	});

	it("converts drafts to base units with safe-integer overflow protection", () => {
		expect(convertNumberUnitValueToBaseUnit("", unit)).toBe(0);
		expect(convertNumberUnitValueToBaseUnit("4", unit)).toBe(4096);
		expect(
			convertNumberUnitValueToBaseUnit(String(Number.MAX_SAFE_INTEGER), unit),
		).toBeNull();
	});

	it("rejects invalid unit multipliers", () => {
		expect(
			convertNumberUnitValueToBaseUnit("4", {
				...unit,
				multiplier: 0,
			}),
		).toBeNull();
		expect(
			convertNumberUnitValueToBaseUnit("4", {
				...unit,
				multiplier: -1,
			}),
		).toBeNull();
	});
});
