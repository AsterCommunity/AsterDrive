import {
	buildWorkspacePath,
	type Workspace,
	workspaceKey,
} from "@/lib/workspace";
import { api, downloadFile } from "@/services/http";
import type { WorkspaceActivityPage } from "@/types/api";

export type ActivityTarget =
	| {
			kind: "resource";
			workspace: Workspace;
			entityType: "file" | "folder";
			entityId: number;
	  }
	| { kind: "team"; teamId: number; admin?: boolean };

export function createWorkspaceActivityResource(target: ActivityTarget) {
	const path =
		target.kind === "team"
			? `${target.admin ? "/admin" : ""}/teams/${target.teamId}/activities`
			: buildWorkspacePath(
					target.workspace,
					`/${target.entityType === "file" ? "files" : "folders"}/${target.entityId}/activities`,
				);
	return {
		key:
			target.kind === "team"
				? `${target.admin ? "admin" : "team"}:${target.teamId}`
				: `${workspaceKey(target.workspace)}:${target.entityType}:${target.entityId}`,
		list: (beforeId?: number | null, signal?: AbortSignal) =>
			api.get<WorkspaceActivityPage>(path, {
				params: { limit: 50, before_id: beforeId ?? undefined },
				signal,
			}),
		export:
			target.kind === "team"
				? () =>
						downloadFile(
							`${path}/export`,
							undefined,
							`asterdrive-team-${target.teamId}-activities.csv`,
						)
				: undefined,
	};
}
