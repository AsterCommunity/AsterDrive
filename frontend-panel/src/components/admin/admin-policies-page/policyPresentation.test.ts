import { describe, expect, it } from "vitest";
import { getPolicyDriverBadgeClass } from "./policyPresentation";

describe("policyPresentation", () => {
	it("returns distinct badge classes for every storage driver", () => {
		expect(getPolicyDriverBadgeClass("local")).toContain("text-emerald-600");
		expect(getPolicyDriverBadgeClass("s3")).toContain("text-blue-600");
		expect(getPolicyDriverBadgeClass("tencent_cos")).toContain("text-cyan-700");
		expect(getPolicyDriverBadgeClass("azure_blob")).toContain("text-sky-700");
		expect(getPolicyDriverBadgeClass("remote")).toContain("text-amber-600");
		expect(getPolicyDriverBadgeClass("one_drive")).toContain("text-blue-700");
	});
});
