import { type RefObject, useLayoutEffect } from "react";
import {
	adminTeamDetailContentScrollPositions,
	adminTeamDetailSidebarScrollPositions,
} from "./adminTeamDetailDialogState";
import type { AdminTeamDetailTab } from "./types";

interface UseAdminTeamDetailScrollRestorationArgs {
	contentRef: RefObject<HTMLDivElement | null>;
	isPageLayout: boolean;
	pageTab?: AdminTeamDetailTab;
	sidebarRef: RefObject<HTMLElement | null>;
	teamId: number | null;
}

export function useAdminTeamDetailScrollRestoration({
	contentRef,
	isPageLayout,
	pageTab,
	sidebarRef,
	teamId,
}: UseAdminTeamDetailScrollRestorationArgs) {
	useLayoutEffect(() => {
		if (!isPageLayout || teamId == null || pageTab == null) {
			return;
		}

		const content = contentRef.current;
		if (content != null) {
			content.scrollTop =
				adminTeamDetailContentScrollPositions.get(teamId) ?? 0;
		}

		const sidebar = sidebarRef.current;
		if (sidebar == null) {
			return () => {
				if (content == null) {
					return;
				}

				adminTeamDetailContentScrollPositions.set(teamId, content.scrollTop);
			};
		}

		sidebar.scrollTop = adminTeamDetailSidebarScrollPositions.get(teamId) ?? 0;

		return () => {
			if (content != null) {
				adminTeamDetailContentScrollPositions.set(teamId, content.scrollTop);
			}

			if (sidebar != null) {
				adminTeamDetailSidebarScrollPositions.set(teamId, sidebar.scrollTop);
			}
		};
	}, [contentRef, isPageLayout, pageTab, sidebarRef, teamId]);
}
