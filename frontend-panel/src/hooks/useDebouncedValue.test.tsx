import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";

describe("useDebouncedValue", () => {
	it("restarts the delay for the latest value and cancels on delay changes and unmount", () => {
		vi.useFakeTimers();
		const { result, rerender, unmount } = renderHook(
			({ value, delay }) => useDebouncedValue(value, delay),
			{ initialProps: { value: "initial", delay: 350 } },
		);
		rerender({ value: "first", delay: 350 });
		act(() => vi.advanceTimersByTime(200));
		rerender({ value: "latest", delay: 350 });
		act(() => vi.advanceTimersByTime(349));
		expect(result.current).toBe("initial");
		act(() => vi.advanceTimersByTime(1));
		expect(result.current).toBe("latest");
		rerender({ value: "pending", delay: 350 });
		act(() => vi.advanceTimersByTime(200));
		rerender({ value: "pending", delay: 500 });
		act(() => vi.advanceTimersByTime(499));
		expect(result.current).toBe("latest");
		act(() => vi.advanceTimersByTime(1));
		expect(result.current).toBe("pending");
		rerender({ value: "disposed", delay: 500 });
		unmount();
		expect(vi.getTimerCount()).toBe(0);
		act(() => vi.advanceTimersByTime(500));
		expect(result.current).toBe("pending");
	});
});
