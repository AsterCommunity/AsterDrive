import { describe, expect, it } from "vitest";
import { buildPolicyGroupOptions } from "@/components/admin/user-detail-dialog/types";

const group = (overrides: Record<string, unknown> = {}) => ({
	id: 1,
	name: "Primary",
	is_enabled: true,
	rules: [{ id: 10 }],
	...overrides,
});

describe("buildPolicyGroupOptions", () => {
	it("filters disabled and empty groups while preserving enabled groups", () => {
		expect(
			buildPolicyGroupOptions(
				[
					group(),
					group({ id: 2, name: "Disabled", is_enabled: false }),
					group({ id: 3, name: "Empty", rules: [] }),
				] as never,
				null,
			),
		).toEqual([{ label: "Primary", value: "1" }]);
	});

	it("adds an unavailable selected group as a disabled option", () => {
		expect(buildPolicyGroupOptions([group()] as never, 99)).toEqual([
			{ label: "#99", value: "99", disabled: true },
			{ label: "Primary", value: "1" },
		]);
		expect(
			buildPolicyGroupOptions([group({ id: 99, name: "Legacy" })] as never, 99),
		).toEqual([{ label: "Legacy", value: "99" }]);
	});
});
