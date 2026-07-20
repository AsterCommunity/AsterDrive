import { useLayoutEffect, useMemo } from "react";
import { Outlet } from "react-router-dom";
import { DownloadCenter } from "@/components/files/DownloadCenter";
import { UploadAreaHost } from "@/components/files/UploadAreaHost";
import { BottomRightActivityShell } from "@/components/layout/BottomRightActivityShell";
import {
	PERSONAL_WORKSPACE,
	type Workspace,
	workspaceEquals,
} from "@/lib/workspace";
import { useFileStore } from "@/stores/fileStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";

export function WorkspaceOutlet({ workspace }: { workspace: Workspace }) {
	const workspaceKind = workspace.kind;
	const workspaceTeamId =
		workspace.kind === "team" ? workspace.teamId : undefined;
	const stableWorkspace = useMemo<Workspace>(
		() =>
			workspaceKind === "team" && workspaceTeamId !== undefined
				? { kind: "team", teamId: workspaceTeamId }
				: PERSONAL_WORKSPACE,
		[workspaceKind, workspaceTeamId],
	);

	useLayoutEffect(() => {
		if (
			workspaceEquals(useWorkspaceStore.getState().workspace, stableWorkspace)
		) {
			return;
		}
		useWorkspaceStore.getState().setWorkspace(stableWorkspace);
		useFileStore.getState().resetWorkspaceState();
	}, [stableWorkspace]);

	return (
		<BottomRightActivityShell>
			<UploadAreaHost workspace={stableWorkspace} />
			<Outlet />
			<DownloadCenter />
		</BottomRightActivityShell>
	);
}
