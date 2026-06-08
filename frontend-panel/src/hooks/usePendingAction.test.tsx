import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { usePendingAction } from "@/hooks/usePendingAction";

describe("usePendingAction", () => {
	it("guards duplicate entry synchronously", async () => {
		const { result } = renderHook(() => usePendingAction());
		let releaseAction: (() => void) | undefined;
		const action = vi.fn(
			() =>
				new Promise<string>((resolve) => {
					releaseAction = () => resolve("done");
				}),
		);

		const runs: Array<
			Promise<
				{ entered: true; value: string } | { entered: false; value: undefined }
			>
		> = [];

		act(() => {
			runs.push(result.current.runWithPending(action));
			runs.push(result.current.runWithPending(action));
		});
		const [first, second] = runs;
		if (!first || !second) {
			throw new Error("pending action runs were not captured");
		}

		await expect(second).resolves.toEqual({
			entered: false,
			value: undefined,
		});
		expect(action).toHaveBeenCalledTimes(1);
		expect(result.current.pending).toBe(true);

		await act(async () => {
			releaseAction?.();
			await first;
		});

		await expect(first).resolves.toEqual({
			entered: true,
			value: "done",
		});
		expect(result.current.pending).toBe(false);
	});

	it("releases pending when the action rejects", async () => {
		const { result } = renderHook(() => usePendingAction());

		await expect(
			act(async () => {
				await result.current.runWithPending(async () => {
					throw new Error("failed");
				});
			}),
		).rejects.toThrow("failed");

		expect(result.current.pending).toBe(false);
	});

	it("can clear pending manually", () => {
		const { result } = renderHook(() => usePendingAction());

		act(() => {
			void result.current.runWithPending(
				() => new Promise<void>(() => undefined),
			);
		});
		expect(result.current.pending).toBe(true);

		act(() => {
			result.current.clearPending();
		});

		expect(result.current.pending).toBe(false);
	});
});
