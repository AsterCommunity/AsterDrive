import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { WorkspaceActivity, WorkspaceActivityPage } from "@/types/api";
import { useWorkspaceActivities } from "./useWorkspaceActivities";

const event = (id: number): WorkspaceActivity => ({
	id,
	event_id: `event-${id}`,
	operation_id: `operation-${id}`,
	action: "file_rename",
	actor: { user_id: 1, name: "Alice" },
	entity_type: "file",
	entity_id: 7,
	entity_name: "report.txt",
	source: "rest",
	created_at: "2026-10-30T00:00:00Z",
	details: {},
});
function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((done) => {
		resolve = done;
	});
	return { promise, resolve };
}
describe("useWorkspaceActivities", () => {
	it("aborts old requests and ignores stale completions after switching resources", async () => {
		const a = deferred<WorkspaceActivityPage>();
		const b = deferred<WorkspaceActivityPage>();
		const first = {
			key: "team:1:file:7",
			list: vi.fn(() => a.promise),
			export: undefined,
		};
		const second = {
			key: "team:2:file:7",
			list: vi.fn(() => b.promise),
			export: undefined,
		};
		const { result, rerender } = renderHook(
			({ resource }) => useWorkspaceActivities(resource, true),
			{ initialProps: { resource: first } },
		);
		const signal = first.list.mock.calls[0]?.[1] as AbortSignal;
		rerender({ resource: second });
		expect(signal.aborted).toBe(true);
		await act(async () =>
			a.resolve({ items: [event(1)], next_before_id: null }),
		);
		expect(result.current.items).toEqual([]);
		await act(async () =>
			b.resolve({ items: [event(2)], next_before_id: null }),
		);
		expect(result.current.items.map((e) => e.id)).toEqual([2]);
	});
	it("does not request while closed and aborts on close", async () => {
		const pending = deferred<WorkspaceActivityPage>();
		const list = vi.fn(
			(_cursor?: number | null, _signal?: AbortSignal) => pending.promise,
		);
		const resource = { key: "personal:file:7", list, export: undefined };
		const { result, rerender } = renderHook(
			({ enabled }) => useWorkspaceActivities(resource, enabled),
			{ initialProps: { enabled: false } },
		);
		expect(list).not.toHaveBeenCalled();
		rerender({ enabled: true });
		expect(list).toHaveBeenCalledTimes(1);
		rerender({ enabled: false });
		expect(list.mock.calls[0]?.[1]?.aborted).toBe(true);
		await act(async () =>
			pending.resolve({ items: [event(1)], next_before_id: null }),
		);
		expect(result.current.items).toEqual([]);
		expect(result.current.failed).toBe(false);
	});
	it("preserves the current page on failure and retries the same cursor", async () => {
		const list = vi
			.fn()
			.mockResolvedValueOnce({ items: [event(3)], next_before_id: 3 })
			.mockRejectedValueOnce(new Error("offline"))
			.mockResolvedValueOnce({ items: [event(2)], next_before_id: null });
		const resource = { key: "personal:file:7", list, export: undefined };
		const { result } = renderHook(() => useWorkspaceActivities(resource, true));
		await waitFor(() => expect(result.current.loading).toBe(false));
		await act(async () => {
			await result.current.nextPage();
		});
		expect(result.current.items.map((e) => e.id)).toEqual([3]);
		expect(result.current.failed).toBe(true);
		await act(async () => {
			await result.current.retry();
		});
		expect(list.mock.calls[1]?.[0]).toBe(3);
		expect(list.mock.calls[2]?.[0]).toBe(3);
		expect(result.current.items.map((e) => e.id)).toEqual([2]);
	});
	it("deduplicates rapid load-more clicks and ignores responses after unmount", async () => {
		const next = deferred<WorkspaceActivityPage>();
		const list = vi
			.fn()
			.mockResolvedValueOnce({ items: [event(3)], next_before_id: 3 })
			.mockImplementation(() => next.promise);
		const resource = { key: "personal:file:7", list, export: undefined };
		const { result, unmount } = renderHook(() =>
			useWorkspaceActivities(resource, true),
		);
		await waitFor(() => expect(result.current.items).toHaveLength(1));
		let request: Promise<unknown> | undefined;
		act(() => {
			request = result.current.nextPage();
			void result.current.nextPage();
		});
		expect(list).toHaveBeenCalledTimes(2);
		const signal = list.mock.calls[1]?.[1] as AbortSignal;
		unmount();
		expect(signal.aborted).toBe(true);
		await act(async () => {
			next.resolve({ items: [event(2)], next_before_id: null });
			await request;
		});
	});
	it("replaces pages instead of accumulating history and navigates back by cursor", async () => {
		const list = vi
			.fn()
			.mockResolvedValueOnce({ items: [event(4), event(3)], next_before_id: 3 })
			.mockResolvedValueOnce({
				items: [event(2), event(1)],
				next_before_id: null,
			})
			.mockResolvedValueOnce({
				items: [event(4), event(3)],
				next_before_id: 3,
			});
		const resource = { key: "personal:file:7", list, export: undefined };
		const { result } = renderHook(() => useWorkspaceActivities(resource, true));
		await waitFor(() => expect(result.current.items).toHaveLength(2));
		await act(async () => {
			await result.current.nextPage();
		});
		expect(result.current.items.map((e) => e.id)).toEqual([2, 1]);
		expect(result.current.page).toBe(2);
		await act(async () => {
			await result.current.previousPage();
		});
		expect(list.mock.calls[2]?.[0]).toBe(null);
		expect(result.current.items.map((e) => e.id)).toEqual([4, 3]);
		expect(result.current.page).toBe(1);
	});
});
