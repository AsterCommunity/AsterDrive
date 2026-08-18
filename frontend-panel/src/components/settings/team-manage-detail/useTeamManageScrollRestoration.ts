import { useCallback, useLayoutEffect, useRef } from "react";
import {
	teamManageContentScrollPositions,
	teamManageSidebarScrollPositions,
} from "./teamManageDetailState";
import type { TeamManageTab } from "./types";

interface UseTeamManageScrollRestorationArgs {
	pageTab: TeamManageTab;
	teamId: number;
}

export function useTeamManageScrollRestoration({
	pageTab,
	teamId,
}: UseTeamManageScrollRestorationArgs) {
	const contentRef = useRef<HTMLDivElement | null>(null);
	const sidebarRef = useRef<HTMLElement | null>(null);

	// biome-ignore lint/correctness/useExhaustiveDependencies: pageTab re-runs position save/restore on every tab switch even though the effect body only reads refs
	useLayoutEffect(() => {
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
	}, [pageTab, teamId]);

	const handleContentScroll = useCallback(() => {
		if (contentRef.current == null) {
			return;
		}

		teamManageContentScrollPositions.set(teamId, contentRef.current.scrollTop);
	}, [teamId]);

	const handleSidebarScroll = useCallback(() => {
		if (sidebarRef.current == null) {
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
