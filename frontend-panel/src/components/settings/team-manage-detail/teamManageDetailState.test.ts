import { describe, expect, it } from "vitest";
import {
	getTeamManagePanelAnimationClass,
	getTeamManageTabDirection,
	isTeamManageTab,
	isTeamManageTabAllowed,
} from "@/components/settings/team-manage-detail/teamManageDetailState";

describe("teamManageDetailState", () => {
	it("validates tabs and role-dependent tab access", () => {
		expect(isTeamManageTab("overview")).toBe(true);
		expect(isTeamManageTab("members")).toBe(true);
		expect(isTeamManageTab("webdav")).toBe(true);
		expect(isTeamManageTab("audit")).toBe(true);
		expect(isTeamManageTab("danger")).toBe(true);
		expect(isTeamManageTab("missing")).toBe(false);

		expect(isTeamManageTabAllowed("overview", false, false)).toBe(true);
		expect(isTeamManageTabAllowed("members", false, false)).toBe(true);
		expect(isTeamManageTabAllowed("webdav", false, false)).toBe(true);
		expect(isTeamManageTabAllowed("audit", false, true)).toBe(false);
		expect(isTeamManageTabAllowed("audit", true, false)).toBe(true);
		expect(isTeamManageTabAllowed("danger", true, false)).toBe(false);
		expect(isTeamManageTabAllowed("danger", false, true)).toBe(true);
	});

	it("maps tab movement to panel animation direction", () => {
		expect(getTeamManageTabDirection("danger", "overview")).toBe("forward");
		expect(getTeamManageTabDirection("webdav", "audit")).toBe("backward");
		expect(getTeamManagePanelAnimationClass("forward")).toContain(
			"slide-in-from-right-4",
		);
		expect(getTeamManagePanelAnimationClass("backward")).toContain(
			"slide-in-from-left-4",
		);
	});
});
