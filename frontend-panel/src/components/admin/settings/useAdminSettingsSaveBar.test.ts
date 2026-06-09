import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAdminSettingsSaveBar } from "@/components/admin/settings/useAdminSettingsSaveBar";

function createProps(
	overrides: Partial<Parameters<typeof useAdminSettingsSaveBar>[0]> = {},
) {
	return {
		desktopMinReservedHeight: 64,
		enterDurationMs: 180,
		exitDurationMs: 140,
		hasUnsavedChanges: false,
		mobileBreakpoint: 768,
		mobileMinReservedHeight: 96,
		viewportWidth: 1024,
		...overrides,
	};
}

describe("useAdminSettingsSaveBar", () => {
	beforeEach(() => {
		vi.useFakeTimers();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("enters and reserves the mobile fallback height before the bar is measured", () => {
		const { result, rerender } = renderHook(useAdminSettingsSaveBar, {
			initialProps: createProps({
				viewportWidth: 520,
			}),
		});

		act(() => {
			rerender(
				createProps({
					hasUnsavedChanges: true,
					viewportWidth: 520,
				}),
			);
		});

		expect(result.current.phase).toBe("entering");
		expect(result.current.reservedHeight).toBe(96);

		act(() => {
			vi.advanceTimersByTime(180);
		});

		expect(result.current.phase).toBe("visible");
	});

	it("measures the mounted height and keeps the bar mounted through the exit animation", () => {
		const { result, rerender } = renderHook(useAdminSettingsSaveBar, {
			initialProps: createProps(),
		});

		const measureNode = document.createElement("div");
		Object.defineProperty(measureNode, "getBoundingClientRect", {
			value: () =>
				({
					bottom: 72,
					height: 72,
					left: 0,
					right: 0,
					toJSON: () => ({}),
					top: 0,
					width: 320,
					x: 0,
					y: 0,
				}) satisfies DOMRect,
		});

		act(() => {
			(
				result.current.measureRef as {
					current: HTMLDivElement | null;
				}
			).current = measureNode;
		});

		act(() => {
			rerender(
				createProps({
					hasUnsavedChanges: true,
				}),
			);
		});

		expect(result.current.phase).toBe("entering");
		expect(result.current.reservedHeight).toBe(72);

		act(() => {
			rerender(
				createProps({
					hasUnsavedChanges: false,
				}),
			);
		});

		expect(result.current.phase).toBe("entering");
		expect(result.current.reservedHeight).toBe(72);

		act(() => {
			vi.advanceTimersByTime(16);
		});

		expect(result.current.phase).toBe("visible");
		expect(result.current.reservedHeight).toBe(72);

		act(() => {
			vi.advanceTimersByTime(16);
		});

		expect(result.current.phase).toBe("exiting");
		expect(result.current.reservedHeight).toBe(0);

		act(() => {
			vi.advanceTimersByTime(140);
		});

		expect(result.current.phase).toBe("exiting");
		expect(result.current.reservedHeight).toBe(0);

		act(() => {
			vi.advanceTimersByTime(50);
		});

		expect(result.current.phase).toBe("hidden");
		expect(result.current.reservedHeight).toBe(0);
	});
});
