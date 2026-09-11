import { afterEach, describe, expect, it, vi } from "vitest";
import { runWhenIdle } from "@/lib/idleTask";

describe("runWhenIdle", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	it("uses requestIdleCallback when available and cancels it", () => {
		const task = vi.fn();
		const cancelIdleCallback = vi.fn();
		const requestIdleCallback = vi.fn().mockReturnValue(42);
		vi.stubGlobal("requestIdleCallback", requestIdleCallback);
		vi.stubGlobal("cancelIdleCallback", cancelIdleCallback);

		const cancel = runWhenIdle(task, { timeoutMs: 500 });
		cancel();

		expect(requestIdleCallback).toHaveBeenCalledWith(task, { timeout: 500 });
		expect(cancelIdleCallback).toHaveBeenCalledWith(42);
		expect(task).not.toHaveBeenCalled();
	});

	it("falls back to setTimeout and clears the timeout", () => {
		vi.useFakeTimers();
		vi.stubGlobal("requestIdleCallback", undefined);
		vi.stubGlobal("cancelIdleCallback", undefined);
		const task = vi.fn();

		const cancel = runWhenIdle(task, { fallbackDelayMs: 25 });
		vi.advanceTimersByTime(24);
		expect(task).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		expect(task).toHaveBeenCalledTimes(1);

		const secondTask = vi.fn();
		const secondCancel = runWhenIdle(secondTask, { fallbackDelayMs: 25 });
		secondCancel();
		vi.advanceTimersByTime(25);
		expect(secondTask).not.toHaveBeenCalled();
		cancel();
	});
});
