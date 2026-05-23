import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useDesktopInfoPanelMount } from "./useDesktopInfoPanelMount";

describe("useDesktopInfoPanelMount", () => {
	it("syncs immediately outside desktop mode", () => {
		const { rerender, result } = renderHook(
			(props: { isDesktop: boolean; open: boolean }) =>
				useDesktopInfoPanelMount(props.open, props.isDesktop),
			{ initialProps: { isDesktop: false, open: true } },
		);

		expect(result.current).toEqual({
			desktopMounted: true,
			desktopVisible: true,
		});

		rerender({ isDesktop: false, open: false });

		expect(result.current).toEqual({
			desktopMounted: false,
			desktopVisible: false,
		});
	});

	it("delays desktop enter and exit states for animation", () => {
		vi.useFakeTimers();
		const { rerender, result } = renderHook(
			(props: { open: boolean }) => useDesktopInfoPanelMount(props.open, true),
			{ initialProps: { open: false } },
		);

		expect(result.current).toEqual({
			desktopMounted: false,
			desktopVisible: false,
		});

		rerender({ open: true });
		expect(result.current.desktopMounted).toBe(true);
		expect(result.current.desktopVisible).toBe(false);

		act(() => {
			vi.advanceTimersByTime(0);
		});
		expect(result.current).toEqual({
			desktopMounted: true,
			desktopVisible: true,
		});

		rerender({ open: false });
		expect(result.current).toEqual({
			desktopMounted: true,
			desktopVisible: false,
		});

		act(() => {
			vi.advanceTimersByTime(219);
		});
		expect(result.current.desktopMounted).toBe(true);

		act(() => {
			vi.advanceTimersByTime(1);
		});
		expect(result.current).toEqual({
			desktopMounted: false,
			desktopVisible: false,
		});
	});

	it("clears stale desktop animation timers when open state changes quickly", () => {
		vi.useFakeTimers();
		const { rerender, result } = renderHook(
			(props: { open: boolean }) => useDesktopInfoPanelMount(props.open, true),
			{ initialProps: { open: false } },
		);

		rerender({ open: true });
		rerender({ open: false });
		rerender({ open: true });

		act(() => {
			vi.advanceTimersByTime(220);
		});

		expect(result.current).toEqual({
			desktopMounted: true,
			desktopVisible: true,
		});
	});
});
