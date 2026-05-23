import { useCallback, useLayoutEffect, useRef } from "react";
import {
	teamManageContentScrollPositions,
	teamManageSidebarScrollPositions,
} from "./teamManageDialogState";
import type { TeamManageTab } from "./types";

interface UseTeamManageScrollRestorationArgs {
	isPageLayout: boolean;
	pageTab?: TeamManageTab;
	teamId: number | null;
}

export function useTeamManageScrollRestoration({
	isPageLayout,
	pageTab,
	teamId,
}: UseTeamManageScrollRestorationArgs) {
	const contentRef = useRef<HTMLDivElement | null>(null);
	const sidebarRef = useRef<HTMLElement | null>(null);

	useLayoutEffect(() => {
		if (!isPageLayout || teamId == null || pageTab == null) {
			return;
		}

		const content = contentRef.current;
		if (content != null) {
			content.scrollTop = teamManageContentScrollPositions.get(teamId) ?? 0;
		}

		const sidebar = sidebarRef.current;
		if (sidebar == null) {
			return () => {
				if (content == null) {
					return;
				}

				teamManageContentScrollPositions.set(teamId, content.scrollTop);
			};
		}

		sidebar.scrollTop = teamManageSidebarScrollPositions.get(teamId) ?? 0;

		return () => {
			if (content != null) {
				teamManageContentScrollPositions.set(teamId, content.scrollTop);
			}

			if (sidebar != null) {
				teamManageSidebarScrollPositions.set(teamId, sidebar.scrollTop);
			}
		};
	}, [isPageLayout, pageTab, teamId]);

	const handleContentScroll = useCallback(() => {
		if (teamId == null || contentRef.current == null) {
			return;
		}

		teamManageContentScrollPositions.set(teamId, contentRef.current.scrollTop);
	}, [teamId]);

	const handleSidebarScroll = useCallback(() => {
		if (teamId == null || sidebarRef.current == null) {
			return;
		}

		teamManageSidebarScrollPositions.set(teamId, sidebarRef.current.scrollTop);
	}, [teamId]);

	return {
		contentRef,
		handleContentScroll,
		handleSidebarScroll,
		sidebarRef,
	};
}
