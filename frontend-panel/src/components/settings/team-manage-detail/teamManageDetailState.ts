import type { TeamManageTab } from "./types";

export const TEAM_MANAGE_MEMBER_PAGE_SIZE = 10;
export const TEAM_MANAGE_AUDIT_PAGE_SIZE = 10;

export const TEAM_MANAGE_TAB_INDEX: Record<TeamManageTab, number> = {
	overview: 0,
	members: 1,
	webdav: 2,
	audit: 3,
	danger: 4,
};

export const teamManageContentScrollPositions = new Map<number, number>();
export const teamManageSidebarScrollPositions = new Map<number, number>();

export function isTeamManageTab(value: string): value is TeamManageTab {
	return (
		value === "overview" ||
		value === "members" ||
		value === "webdav" ||
		value === "audit" ||
		value === "danger"
	);
}

export function isTeamManageTabAllowed(
	tab: TeamManageTab,
	canManageTeam: boolean,
	canArchiveTeam: boolean,
) {
	return (
		tab === "overview" ||
		tab === "members" ||
		tab === "webdav" ||
		(tab === "audit" && canManageTeam) ||
		(tab === "danger" && canArchiveTeam)
	);
}

export function getTeamManageTabDirection(
	nextTab: TeamManageTab,
	currentTab: TeamManageTab,
) {
	return TEAM_MANAGE_TAB_INDEX[nextTab] >= TEAM_MANAGE_TAB_INDEX[currentTab]
		? "forward"
		: "backward";
}

export function getTeamManagePanelAnimationClass(
	tabDirection: "forward" | "backward",
) {
	return tabDirection === "forward"
		? "animate-in fade-in duration-300 slide-in-from-right-4 motion-reduce:animate-none"
		: "animate-in fade-in duration-300 slide-in-from-left-4 motion-reduce:animate-none";
}
