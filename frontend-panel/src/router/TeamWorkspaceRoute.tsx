import { lazy, Suspense, useMemo } from "react";
import { Navigate, useParams } from "react-router-dom";
import type { Workspace } from "@/lib/workspace";
import { Loading } from "./Loading";

const WorkspaceOutlet = lazy(() =>
	import("./WorkspaceOutlet").then((module) => ({
		default: module.WorkspaceOutlet,
	})),
);

export function TeamWorkspaceRoute() {
	const { teamId } = useParams<{ teamId?: string }>();
	const parsedTeamId = Number(teamId);
	const workspace = useMemo<Workspace>(
		() => ({ kind: "team", teamId: parsedTeamId }),
		[parsedTeamId],
	);

	if (!Number.isSafeInteger(parsedTeamId) || parsedTeamId <= 0) {
		return <Navigate to="/" replace />;
	}

	return (
		<Suspense fallback={<Loading />}>
			<WorkspaceOutlet workspace={workspace} />
		</Suspense>
	);
}
