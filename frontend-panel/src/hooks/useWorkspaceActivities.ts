import { useCallback, useEffect, useRef, useState } from "react";
import type { createWorkspaceActivityResource } from "@/services/workspaceActivityService";
import type { WorkspaceActivity } from "@/types/api";

type Resource = ReturnType<typeof createWorkspaceActivityResource>;
interface ActivityState {
	key: string;
	items: WorkspaceActivity[];
	next: number | null;
	loading: boolean;
	failed: boolean;
}
const empty: ActivityState = {
	key: "",
	items: [],
	next: null,
	loading: false,
	failed: false,
};

export function useWorkspaceActivities(resource: Resource, enabled: boolean) {
	const [state, setState] = useState<ActivityState>(empty);
	const generation = useRef(0);
	const pending = useRef<AbortController | null>(null);
	const load = useCallback(
		async (before: number | null = null) => {
			pending.current?.abort();
			const controller = new AbortController();
			pending.current = controller;
			const request = ++generation.current;
			setState((previous) => ({
				...empty,
				...(before !== null && previous.key === resource.key ? previous : {}),
				key: resource.key,
				loading: true,
				failed: false,
			}));
			try {
				const page = await resource.list(before, controller.signal);
				if (controller.signal.aborted || generation.current !== request) return;
				setState((previous) => ({
					key: resource.key,
					items:
						before !== null ? [...previous.items, ...page.items] : page.items,
					next: page.next_before_id ?? null,
					loading: false,
					failed: false,
				}));
			} catch {
				if (!controller.signal.aborted && generation.current === request)
					setState((previous) => ({
						...previous,
						loading: false,
						failed: true,
					}));
			} finally {
				if (generation.current === request) pending.current = null;
			}
		},
		[resource],
	);
	useEffect(() => {
		if (enabled) void load();
		return () => {
			++generation.current;
			pending.current?.abort();
			pending.current = null;
		};
	}, [enabled, load]);
	const current = state.key === resource.key && enabled ? state : empty;
	return {
		...current,
		reload: () => load(),
		loadMore: () =>
			current.next !== null && !current.loading && pending.current === null
				? load(current.next)
				: Promise.resolve(),
	};
}
