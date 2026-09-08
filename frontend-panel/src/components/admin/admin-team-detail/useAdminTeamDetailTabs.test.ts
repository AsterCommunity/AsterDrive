import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useAdminTeamDetailTabs } from "@/components/admin/admin-team-detail/useAdminTeamDetailTabs";

describe("useAdminTeamDetailTabs", () => {
	it("ignores invalid tab values", () => {
		const onPageTabChange = vi.fn();
		const { result } = renderHook(() =>
			useAdminTeamDetailTabs({ onPageTabChange, pageTab: "overview" }),
		);

		expect(result.current.currentTab).toBe("overview");

		act(() => {
			result.current.handleTabChange("members");
		});

		expect(onPageTabChange).toHaveBeenCalledWith("members");

		act(() => {
			result.current.handleTabChange("not-a-tab");
		});

		expect(onPageTabChange).toHaveBeenCalledTimes(1);
	});

	it("syncs page tabs and reports changes with directional animation", () => {
		const onPageTabChange = vi.fn();
		const { result, rerender } = renderHook(
			({ pageTab }: { pageTab: "overview" | "members" | "audit" | "danger" }) =>
				useAdminTeamDetailTabs({
					onPageTabChange,
					pageTab,
				}),
			{ initialProps: { pageTab: "overview" as const } },
		);

		act(() => {
			result.current.handleTabChange("audit");
		});

		expect(onPageTabChange).toHaveBeenCalledWith("audit");
		rerender({ pageTab: "audit" });
		expect(result.current.currentTab).toBe("audit");
		expect(result.current.panelAnimationClass).toContain(
			"slide-in-from-right-4",
		);

		act(() => {
			result.current.handleTabChange("audit");
		});

		expect(onPageTabChange).toHaveBeenCalledTimes(1);

		rerender({ pageTab: "overview" });

		expect(result.current.currentTab).toBe("overview");
		expect(result.current.panelAnimationClass).toContain(
			"slide-in-from-left-4",
		);
	});
});
