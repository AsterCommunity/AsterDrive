import { describe, expect, it } from "vitest";
import { formatJsonPreview, parseCsvPreview } from "./structuredPreview";

describe("structured preview budgets", () => {
	it("preserves JSON formatting and ignores escaped punctuation in strings", () => {
		const content = JSON.stringify({ text: '[{,}:"\\', value: [1, true] });
		expect(formatJsonPreview(content)).toEqual({
			text: JSON.stringify(JSON.parse(content), null, 2),
			limited: false,
		});
		expect(formatJsonPreview("{broken")).toBeNull();
	});

	it("avoids indentation expansion for deeply nested and wide JSON", () => {
		const nested = `${"[".repeat(3000)}0${"]".repeat(3000)}`;
		expect(formatJsonPreview(nested)).toEqual({ text: nested, limited: true });
		const wide = JSON.stringify(Array.from({ length: 20_000 }, () => 0));
		expect(formatJsonPreview(wide)?.limited).toBe(true);
	});

	it("bounds fallback output and formatted expansion before rendering", () => {
		const huge = JSON.stringify("x".repeat(2 * 1024 * 1024));
		const preview = formatJsonPreview(huge);
		expect(preview?.limited).toBe(true);
		expect(preview?.text.length).toBeLessThanOrEqual(256 * 1024);
		const expanded = `${"[".repeat(60)}${Array.from({ length: 3000 }, () => "0").join(",")}${"]".repeat(60)}`;
		expect(formatJsonPreview(expanded)?.limited).toBe(true);
	});

	it("bounds sparse CSV by columns and rendered cells including padding", () => {
		const content = [
			Array.from({ length: 4000 }, (_, index) => `h${index}`).join(","),
			...Array.from({ length: 500 }, () => "value"),
		].join("\n");
		const preview = parseCsvPreview(content, ",");
		expect(preview.invalid).toBe(false);
		expect(preview.limited).toBe(true);
		expect(preview.rows[0]).toHaveLength(50);
		expect(preview.rows.length * preview.rows[0].length).toBeLessThanOrEqual(
			10_000,
		);
		expect(preview.rows[1]).toEqual(["value"]);
	});

	it("preserves quoted multiline CSV and explicit delimiters", () => {
		expect(parseCsvPreview('name;note\nAster;"one\ntwo"', ";")).toEqual({
			rows: [
				["name", "note"],
				["Aster", "one\ntwo"],
			],
			limited: false,
			invalid: false,
		});
	});

	it("retains complete rows if an input limit cuts through a quoted field", () => {
		const preview = parseCsvPreview(
			`name,note\nAster,ok\nlast,"${"x".repeat(2 * 1024 * 1024)}"`,
			",",
		);
		expect(preview.rows).toEqual([
			["name", "note"],
			["Aster", "ok"],
		]);
		expect(preview.limited).toBe(true);
		expect(preview.invalid).toBe(false);
		expect(parseCsvPreview('name,note\nlast,"broken', ",").invalid).toBe(true);
	});
});
