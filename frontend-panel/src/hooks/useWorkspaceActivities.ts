import { useCallback, useEffect, useRef, useState } from "react";
import type { createWorkspaceActivityResource } from "@/services/workspaceActivityService";
import type { WorkspaceActivity } from "@/types/api";

type Resource = ReturnType<typeof createWorkspaceActivityResource>;
interface PageRequest {
	before: number | null;
	cursors: (number | null)[];
}
interface ActivityState {
	key: string;
	items: WorkspaceActivity[];
	next: number | null;
	cursors: (number | null)[];
	loading: boolean;
	failed: boolean;
}
const firstPage: PageRequest = { before: null, cursors: [null] };
const empty: ActivityState = {
	key: "",
	items: [],
	next: null,
	cursors: [null],
	loading: false,
	failed: false,
};

export function useWorkspaceActivities(resource: Resource, enabled: boolean) {
	const [state, setState] = useState<ActivityState>(empty);
	const generation = useRef(0);
	const pending = useRef<AbortController | null>(null);
	const attempted = useRef<PageRequest>(firstPage);
	const load = useCallback(
		async (page: PageRequest = firstPage) => {
			if (!enabled || pending.current) return;
			const controller = new AbortController();
			pending.current = controller;
			attempted.current = page;
			const request = ++generation.current;
			setState((previous) => ({
				...(previous.key === resource.key ? previous : empty),
				key: resource.key,
				loading: true,
				failed: false,
			}));
			try {
				const response = await resource.list(page.before, controller.signal);
				if (controller.signal.aborted || generation.current !== request) return;
				setState({
					key: resource.key,
					items: response.items,
					next: response.next_before_id ?? null,
					cursors: page.cursors,
					loading: false,
					failed: false,
				});
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
		[resource, enabled],
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
		page: current.cursors.length,
		reload: () => load(),
		retry: () => load(attempted.current),
		nextPage: () =>
			current.next !== null
				? load({
						before: current.next,
						cursors: [...current.cursors, current.next],
					})
				: Promise.resolve(),
		previousPage: () =>
			current.cursors.length > 1
				? load({
						before: current.cursors[current.cursors.length - 2],
						cursors: current.cursors.slice(0, -1),
					})
				: Promise.resolve(),
	};
}
