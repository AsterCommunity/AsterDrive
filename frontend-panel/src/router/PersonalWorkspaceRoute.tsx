import { lazy, Suspense } from "react";
import { PERSONAL_WORKSPACE } from "@/lib/workspace";
import { Loading } from "./Loading";

const WorkspaceOutlet = lazy(() =>
	import("./WorkspaceOutlet").then((module) => ({
		default: module.WorkspaceOutlet,
	})),
);

export function PersonalWorkspaceRoute() {
	return (
		<Suspense fallback={<Loading />}>
			<WorkspaceOutlet workspace={PERSONAL_WORKSPACE} />
		</Suspense>
	);
}
