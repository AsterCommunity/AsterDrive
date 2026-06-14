import { describe, expect, it } from "vitest";
import {
	getPolicyDriverBadgeClass,
	getPolicyDriverLabelKey,
} from "./policyPresentation";

describe("policyPresentation", () => {
	it("returns distinct badge classes for every storage driver", () => {
		expect(getPolicyDriverBadgeClass("local")).toContain("text-emerald-600");
		expect(getPolicyDriverBadgeClass("s3")).toContain("text-blue-600");
		expect(getPolicyDriverBadgeClass("tencent_cos")).toContain("text-cyan-700");
		expect(getPolicyDriverBadgeClass("azure_blob")).toContain("text-sky-700");
		expect(getPolicyDriverBadgeClass("remote")).toContain("text-amber-600");
	});

	it("returns label keys for every storage driver", () => {
		expect(getPolicyDriverLabelKey("local")).toBe("driver_type_local");
		expect(getPolicyDriverLabelKey("remote")).toBe("driver_type_remote");
		expect(getPolicyDriverLabelKey("tencent_cos")).toBe(
			"driver_type_tencent_cos",
		);
		expect(getPolicyDriverLabelKey("azure_blob")).toBe(
			"driver_type_azure_blob",
		);
		expect(getPolicyDriverLabelKey("s3")).toBe("driver_type_s3");
	});

	it("throws for unsupported storage driver label keys", () => {
		expect(() => getPolicyDriverLabelKey("unknown" as never)).toThrow(
			"Unhandled storage policy driver type: unknown",
		);
	});
});
