import { useEffect, useState } from "react";
import {
	getTeamManagePanelAnimationClass,
	getTeamManageTabDirection,
	isTeamManageTab,
	isTeamManageTabAllowed,
} from "./teamManageDetailState";
import type { TeamManageTab } from "./types";

interface UseTeamManageTabsArgs {
	canArchiveTeam: boolean;
	canManageTeam: boolean;
	detailLoading: boolean;
	detailRequestStarted: boolean;
	onPageTabChange: (
		tab: TeamManageTab,
		options?: { replace?: boolean },
	) => void;
	pageTab: TeamManageTab;
}

export function useTeamManageTabs({
	canArchiveTeam,
	canManageTeam,
	detailLoading,
	detailRequestStarted,
	onPageTabChange,
	pageTab,
}: UseTeamManageTabsArgs) {
	const [currentTab, setCurrentTab] = useState<TeamManageTab>(pageTab);
	const [tabDirection, setTabDirection] = useState<"forward" | "backward">(
		"forward",
	);
	const panelAnimationClass = getTeamManagePanelAnimationClass(tabDirection);

	useEffect(() => {
		if (currentTab === pageTab) {
			return;
		}

		setTabDirection(getTeamManageTabDirection(pageTab, currentTab));
		setCurrentTab(pageTab);
	}, [currentTab, pageTab]);

	useEffect(() => {
		if (
			detailLoading ||
			!detailRequestStarted ||
			isTeamManageTabAllowed(pageTab, canManageTeam, canArchiveTeam)
		) {
			return;
		}

		onPageTabChange("overview", { replace: true });
	}, [
		canArchiveTeam,
		canManageTeam,
		detailLoading,
		detailRequestStarted,
		onPageTabChange,
		pageTab,
	]);

	const handleTabChange = (value: string) => {
		if (
			!isTeamManageTab(value) ||
			!isTeamManageTabAllowed(value, canManageTeam, canArchiveTeam) ||
			value === currentTab
		) {
			return;
		}

		setTabDirection(getTeamManageTabDirection(value, currentTab));
		setCurrentTab(value);
		onPageTabChange(value);
	};

	return {
		currentTab,
		handleTabChange,
		panelAnimationClass,
	};
}
