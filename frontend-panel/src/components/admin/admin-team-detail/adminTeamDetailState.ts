import type { PolicyGroupOption } from "@/components/admin/admin-team-detail/AdminTeamDetailSections";
import type { StoragePolicyGroup } from "@/types/api";
import type { AdminTeamDetailTab } from "./types";

export const ADMIN_TEAM_DETAIL_MEMBER_PAGE_SIZE = 10;
export const ADMIN_TEAM_DETAIL_AUDIT_PAGE_SIZE = 10;

export const ADMIN_TEAM_DETAIL_TAB_INDEX: Record<AdminTeamDetailTab, number> = {
	overview: 0,
	members: 1,
	audit: 2,
	danger: 3,
};

export const adminTeamDetailContentScrollPositions = new Map<number, number>();
export const adminTeamDetailSidebarScrollPositions = new Map<number, number>();

export function isAdminTeamDetailTab(
	value: string,
): value is AdminTeamDetailTab {
	return (
		value === "overview" ||
		value === "members" ||
		value === "audit" ||
		value === "danger"
	);
}

export function getAdminTeamDetailTabDirection(
	nextTab: AdminTeamDetailTab,
	currentTab: AdminTeamDetailTab,
) {
	return ADMIN_TEAM_DETAIL_TAB_INDEX[nextTab] >=
		ADMIN_TEAM_DETAIL_TAB_INDEX[currentTab]
		? "forward"
		: "backward";
}

export function getAdminTeamDetailPanelAnimationClass(
	tabDirection: "forward" | "backward",
) {
	return tabDirection === "forward"
		? "animate-in fade-in duration-300 slide-in-from-right-4 motion-reduce:animate-none"
		: "animate-in fade-in duration-300 slide-in-from-left-4 motion-reduce:animate-none";
}

export function buildPolicyGroupOptions(
	policyGroups: StoragePolicyGroup[],
	selectedPolicyGroupId: number | null,
): PolicyGroupOption[] {
	const options: PolicyGroupOption[] = [];
	for (const group of policyGroups) {
		if (!group.is_enabled || (group.rules ?? []).length === 0) {
			continue;
		}
		options.push({
			label: group.name,
			value: String(group.id),
		});
	}

	if (
		selectedPolicyGroupId != null &&
		!options.some((option) => option.value === String(selectedPolicyGroupId))
	) {
		const selectedGroup = policyGroups.find(
			(group) => group.id === selectedPolicyGroupId,
		);
		options.unshift({
			label: selectedGroup?.name ?? `#${selectedPolicyGroupId}`,
			value: String(selectedPolicyGroupId),
			disabled: true,
		});
	}

	return options;
}
