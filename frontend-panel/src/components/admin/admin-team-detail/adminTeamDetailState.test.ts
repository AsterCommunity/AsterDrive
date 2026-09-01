import { describe, expect, it } from "vitest";
import {
	buildPolicyGroupOptions,
	getAdminTeamDetailPanelAnimationClass,
	getAdminTeamDetailTabDirection,
	isAdminTeamDetailTab,
} from "@/components/admin/admin-team-detail/adminTeamDetailState";
import type { StoragePolicyGroup } from "@/types/api";

const group = (
	overrides: Partial<StoragePolicyGroup> = {},
): StoragePolicyGroup => ({
	created_at: "2026-05-01T00:00:00Z",
	description: "",
	id: 1,
	is_default: false,
	is_enabled: true,
	rules: [
		{
			id: 11,
			name: "Rule 1",
			description: "",
			priority: 1,
			is_enabled: true,
			matcher: {
				min_file_size: 0,
				max_file_size: 0,
				extensions: [],
				compound_extensions: [],
				extensionless: null,
				categories: [],
			},
			selection_mode: "first_available",
			unavailable_behavior: "next_rule",
			targets: [],
		},
	],
	name: "Primary",
	updated_at: "2026-05-01T00:00:00Z",
	...overrides,
});

describe("adminTeamDetailState", () => {
	it("validates team detail tabs and directions", () => {
		expect(isAdminTeamDetailTab("overview")).toBe(true);
		expect(isAdminTeamDetailTab("members")).toBe(true);
		expect(isAdminTeamDetailTab("audit")).toBe(true);
		expect(isAdminTeamDetailTab("danger")).toBe(true);
		expect(isAdminTeamDetailTab("missing")).toBe(false);
		expect(getAdminTeamDetailTabDirection("members", "overview")).toBe(
			"forward",
		);
		expect(getAdminTeamDetailTabDirection("overview", "danger")).toBe(
			"backward",
		);
	});

	it("returns directional panel animation classes", () => {
		expect(getAdminTeamDetailPanelAnimationClass("forward")).toContain(
			"slide-in-from-right-4",
		);
		expect(getAdminTeamDetailPanelAnimationClass("backward")).toContain(
			"slide-in-from-left-4",
		);
	});

	it("builds enabled policy group options and preserves invalid selections", () => {
		expect(
			buildPolicyGroupOptions(
				[
					group({ id: 1, name: "Primary" }),
					group({ id: 2, is_enabled: false, name: "Disabled" }),
					group({ id: 3, rules: [], name: "Empty" }),
				],
				2,
			),
		).toEqual([
			{ disabled: true, label: "Disabled", value: "2" },
			{ label: "Primary", value: "1" },
		]);

		expect(buildPolicyGroupOptions([group({ id: 1 })], 99)).toEqual([
			{ disabled: true, label: "#99", value: "99" },
			{ label: "Primary", value: "1" },
		]);
	});
});
