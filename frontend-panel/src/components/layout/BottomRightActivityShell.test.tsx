import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
	BottomRightActivityPortal,
	BottomRightActivityShell,
} from "@/components/layout/BottomRightActivityShell";
import { BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY } from "@/lib/constants";

describe("BottomRightActivityShell", () => {
	const originalResizeObserver = window.ResizeObserver;

	afterEach(() => {
		vi.restoreAllMocks();
		window.ResizeObserver = originalResizeObserver;
		document.documentElement.style.removeProperty(
			BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY,
		);
	});

	it("stacks activity surfaces inside one fixed bottom-right shell", async () => {
		vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
			bottom: 164,
			height: 164,
			left: 0,
			right: 448,
			top: 0,
			width: 448,
			x: 0,
			y: 0,
			toJSON: () => ({}),
		});
		const view = render(
			<BottomRightActivityShell>
				<BottomRightActivityPortal>
					<div>upload-surface</div>
				</BottomRightActivityPortal>
				<BottomRightActivityPortal>
					<div>download-surface</div>
				</BottomRightActivityPortal>
			</BottomRightActivityShell>,
		);

		const shell = screen.getByTestId("bottom-right-activity-shell");
		expect(shell).toHaveClass(
			"fixed",
			"right-4",
			"bottom-4",
			"flex-col",
			"w-[28rem]",
			"overflow-hidden",
			"rounded-lg",
		);
		expect((await screen.findByText("upload-surface")).parentElement).toBe(
			shell,
		);
		expect(screen.getByText("download-surface").parentElement).toBe(shell);
		expect(
			document.documentElement.style.getPropertyValue(
				BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY,
			),
		).toBe("164px");

		view.unmount();
		expect(
			document.documentElement.style.getPropertyValue(
				BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY,
			),
		).toBe("");
	});

	it("updates the CSS variable from ResizeObserver changes and disconnects", () => {
		let height = 10.2;
		const observe = vi.fn();
		const disconnect = vi.fn();
		let resizeCallback: ResizeObserverCallback | null = null;
		class MockResizeObserver {
			constructor(callback: ResizeObserverCallback) {
				resizeCallback = callback;
			}
			observe = observe;
			disconnect = disconnect;
			unobserve = vi.fn();
		}
		window.ResizeObserver =
			MockResizeObserver as unknown as typeof ResizeObserver;
		vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
			() =>
				({
					bottom: height,
					height,
					left: 0,
					right: 448,
					top: 0,
					width: 448,
					x: 0,
					y: 0,
					toJSON: () => ({}),
				}) as DOMRect,
		);

		const view = render(
			<BottomRightActivityShell>content</BottomRightActivityShell>,
		);
		const shell = screen.getByTestId("bottom-right-activity-shell");
		expect(observe).toHaveBeenCalledWith(shell);
		expect(
			document.documentElement.style.getPropertyValue(
				BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY,
			),
		).toBe("11px");

		height = 49.01;
		resizeCallback?.([], {} as ResizeObserver);
		expect(
			document.documentElement.style.getPropertyValue(
				BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY,
			),
		).toBe("50px");

		view.unmount();
		expect(disconnect).toHaveBeenCalledTimes(1);
	});

	it("falls back to window resize events when ResizeObserver is absent", () => {
		window.ResizeObserver = undefined as unknown as typeof ResizeObserver;
		let height = 20;
		vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
			() => ({ height }) as DOMRect,
		);
		const removeEventListener = vi.spyOn(window, "removeEventListener");

		const view = render(
			<BottomRightActivityShell>content</BottomRightActivityShell>,
		);
		height = 72;
		window.dispatchEvent(new Event("resize"));
		expect(
			document.documentElement.style.getPropertyValue(
				BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY,
			),
		).toBe("72px");

		view.unmount();
		expect(removeEventListener).toHaveBeenCalledWith(
			"resize",
			expect.any(Function),
		);
	});
});
