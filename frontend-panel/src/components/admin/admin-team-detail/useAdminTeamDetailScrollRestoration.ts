import { type RefObject, useLayoutEffect } from "react";
import {
	adminTeamDetailContentScrollPositions,
	adminTeamDetailSidebarScrollPositions,
} from "./adminTeamDetailState";
import type { AdminTeamDetailTab } from "./types";

interface UseAdminTeamDetailScrollRestorationArgs {
	contentRef: RefObject<HTMLDivElement | null>;
	pageTab: AdminTeamDetailTab;
	sidebarRef: RefObject<HTMLElement | null>;
	teamId: number;
}

export function useAdminTeamDetailScrollRestoration({
	contentRef,
	pageTab,
	sidebarRef,
	teamId,
}: UseAdminTeamDetailScrollRestorationArgs) {
	// biome-ignore lint/correctness/useExhaustiveDependencies: each URL tab restores the team's shared detail scroll position
	useLayoutEffect(() => {
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
	}, [contentRef, pageTab, sidebarRef, teamId]);
}
