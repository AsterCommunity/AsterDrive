import { describe, expect, it } from "vitest";
import { normalizeObjectStorageConnectionFields } from "@/lib/objectStorageConnectionFields";

describe("normalizeObjectStorageConnectionFields", () => {
	it("trims endpoint and bucket without provider-specific rewriting", () => {
		expect(
			normalizeObjectStorageConnectionFields(
				" https://s3.example.test/custom/path ",
				" archive ",
			),
		).toEqual({
			endpoint: "https://s3.example.test/custom/path",
			bucket: "archive",
		});
	});

	it("preserves custom endpoint paths", () => {
		expect(
			normalizeObjectStorageConnectionFields(
				"https://s3.example.com/custom/path",
				"archive",
			),
		).toEqual({
			endpoint: "https://s3.example.com/custom/path",
			bucket: "archive",
		});
	});
});
