import { useEffect, useState } from "react";
import {
	getAdminTeamDetailPanelAnimationClass,
	getAdminTeamDetailTabDirection,
	isAdminTeamDetailTab,
} from "./adminTeamDetailState";
import type { AdminTeamDetailTab } from "./types";

interface UseAdminTeamDetailTabsArgs {
	onPageTabChange?: (
		tab: AdminTeamDetailTab,
		options?: { replace?: boolean },
	) => void;
	pageTab: AdminTeamDetailTab;
}

export function useAdminTeamDetailTabs({
	onPageTabChange,
	pageTab,
}: UseAdminTeamDetailTabsArgs) {
	const [currentTab, setCurrentTab] = useState<AdminTeamDetailTab>(pageTab);
	const [tabDirection, setTabDirection] = useState<"forward" | "backward">(
		"forward",
	);
	const panelAnimationClass =
		getAdminTeamDetailPanelAnimationClass(tabDirection);

	useEffect(() => {
		if (currentTab === pageTab) {
			return;
		}

		setTabDirection(getAdminTeamDetailTabDirection(pageTab, currentTab));
		setCurrentTab(pageTab);
	}, [currentTab, pageTab]);

	const handleTabChange = (value: string) => {
		if (!isAdminTeamDetailTab(value) || value === currentTab) {
			return;
		}

		setTabDirection(getAdminTeamDetailTabDirection(value, currentTab));
		setCurrentTab(value);
		onPageTabChange?.(value);
	};

	return {
		currentTab,
		handleTabChange,
		panelAnimationClass,
	};
}
