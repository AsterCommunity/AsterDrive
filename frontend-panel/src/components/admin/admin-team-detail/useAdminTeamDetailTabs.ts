import { useEffect, useState } from "react";
import {
	getAdminTeamDetailPanelAnimationClass,
	getAdminTeamDetailTabDirection,
	isAdminTeamDetailTab,
} from "./adminTeamDetailState";
import type { AdminTeamDetailTab } from "./types";

interface UseAdminTeamDetailTabsArgs {
	onPageTabChange: (
		tab: AdminTeamDetailTab,
		options?: { replace?: boolean },
	) => void;
	pageTab: AdminTeamDetailTab;
}

export function useAdminTeamDetailTabs({
	onPageTabChange,
	pageTab,
}: UseAdminTeamDetailTabsArgs) {
	const [pageLayoutTab, setPageLayoutTab] =
		useState<AdminTeamDetailTab>(pageTab);
	const [tabDirection, setTabDirection] = useState<"forward" | "backward">(
		"forward",
	);
	const currentTab = pageLayoutTab;
	const panelAnimationClass =
		getAdminTeamDetailPanelAnimationClass(tabDirection);

	useEffect(() => {
		if (pageLayoutTab === pageTab) {
			return;
		}

		setTabDirection(getAdminTeamDetailTabDirection(pageTab, pageLayoutTab));
		setPageLayoutTab(pageTab);
	}, [pageLayoutTab, pageTab]);

	const handleTabChange = (value: string) => {
		if (!isAdminTeamDetailTab(value)) {
			return;
		}

		if (value === currentTab) {
			return;
		}

		setTabDirection(getAdminTeamDetailTabDirection(value, currentTab));
		setPageLayoutTab(value);
		onPageTabChange(value);
	};

	return {
		currentTab,
		handleTabChange,
		panelAnimationClass,
	};
}
