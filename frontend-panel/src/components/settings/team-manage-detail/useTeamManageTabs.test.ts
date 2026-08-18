import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useTeamManageTabs } from "@/components/settings/team-manage-detail/useTeamManageTabs";

function renderTabs(
	initialProps: Parameters<typeof useTeamManageTabs>[0] = {
		canArchiveTeam: true,
		canManageTeam: true,
		detailLoading: false,
		detailRequestStarted: true,
		onPageTabChange: vi.fn(),
		pageTab: "overview",
	},
) {
	return renderHook(
		(props: Parameters<typeof useTeamManageTabs>[0]) =>
			useTeamManageTabs(props),
		{
			initialProps,
		},
	);
}

describe("useTeamManageTabs", () => {
	it("syncs page tabs and redirects disallowed page tabs after detail loads", () => {
		const onPageTabChange = vi.fn();
		const hook = renderTabs({
			canArchiveTeam: true,
			canManageTeam: true,
			detailLoading: false,
			detailRequestStarted: true,
			onPageTabChange,
			pageTab: "overview",
		});

		act(() => {
			hook.result.current.handleTabChange("danger");
		});

		expect(onPageTabChange).toHaveBeenCalledWith("danger");
		hook.rerender({
			canArchiveTeam: true,
			canManageTeam: true,
			detailLoading: false,
			detailRequestStarted: true,
			onPageTabChange,
			pageTab: "danger",
		});

		expect(hook.result.current.currentTab).toBe("danger");
		expect(hook.result.current.panelAnimationClass).toContain(
			"slide-in-from-right-4",
		);

		hook.rerender({
			canArchiveTeam: false,
			canManageTeam: false,
			detailLoading: false,
			detailRequestStarted: true,
			onPageTabChange,
			pageTab: "danger",
		});

		expect(onPageTabChange).toHaveBeenCalledWith("overview", {
			replace: true,
		});

		hook.rerender({
			canArchiveTeam: false,
			canManageTeam: false,
			detailLoading: false,
			detailRequestStarted: true,
			onPageTabChange,
			pageTab: "overview",
		});

		expect(hook.result.current.currentTab).toBe("overview");
		expect(hook.result.current.panelAnimationClass).toContain(
			"slide-in-from-left-4",
		);
	});

	it("ignores disallowed or repeated tab changes", () => {
		const onPageTabChange = vi.fn();
		const hook = renderTabs({
			canArchiveTeam: false,
			canManageTeam: false,
			detailLoading: false,
			detailRequestStarted: true,
			onPageTabChange,
			pageTab: "overview",
		});

		act(() => {
			hook.result.current.handleTabChange("danger");
		});

		expect(hook.result.current.currentTab).toBe("overview");
		expect(onPageTabChange).not.toHaveBeenCalled();

		act(() => {
			hook.result.current.handleTabChange("members");
		});

		expect(onPageTabChange).toHaveBeenCalledWith("members");

		// 模拟路由同步：pageTab 跟上已切换的 currentTab
		hook.rerender({
			canArchiveTeam: false,
			canManageTeam: false,
			detailLoading: false,
			detailRequestStarted: true,
			onPageTabChange,
			pageTab: "members",
		});

		act(() => {
			hook.result.current.handleTabChange("members");
		});

		expect(onPageTabChange).toHaveBeenCalledTimes(1);
	});
});
