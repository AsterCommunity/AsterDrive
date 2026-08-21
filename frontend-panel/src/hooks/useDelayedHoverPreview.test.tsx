import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDelayedHoverPreview } from "@/hooks/useDelayedHoverPreview";

function TestTarget({ delay }: { delay?: number }) {
	const { triggerRef, triggerEl, open } = useDelayedHoverPreview({
		...(delay === undefined ? {} : { delay }),
	});

	return (
		<div>
			<div ref={triggerRef} data-testid="trigger">
				target
			</div>
			<span data-testid="state">{open ? "open" : "closed"}</span>
			<span data-testid="anchor">{triggerEl ? "anchored" : "no-anchor"}</span>
		</div>
	);
}

describe("useDelayedHoverPreview", () => {
	beforeEach(() => {
		vi.useFakeTimers();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("opens after the delay once the mouse stays on the trigger", () => {
		render(<TestTarget />);
		const trigger = screen.getByTestId("trigger");

		fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
		act(() => {
			vi.advanceTimersByTime(699);
		});
		expect(screen.getByTestId("state")).toHaveTextContent("closed");

		act(() => {
			vi.advanceTimersByTime(1);
		});
		expect(screen.getByTestId("state")).toHaveTextContent("open");
		expect(screen.getByTestId("anchor")).toHaveTextContent("anchored");
	});

	it("cancels the pending preview when the pointer leaves before the delay", () => {
		render(<TestTarget />);
		const trigger = screen.getByTestId("trigger");

		fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
		act(() => {
			vi.advanceTimersByTime(600);
		});
		fireEvent.pointerLeave(trigger);
		act(() => {
			vi.advanceTimersByTime(200);
		});

		expect(screen.getByTestId("state")).toHaveTextContent("closed");
	});

	it("closes an open preview as soon as the pointer leaves", () => {
		render(<TestTarget />);
		const trigger = screen.getByTestId("trigger");

		fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
		act(() => {
			vi.advanceTimersByTime(700);
		});
		expect(screen.getByTestId("state")).toHaveTextContent("open");

		fireEvent.pointerLeave(trigger);
		expect(screen.getByTestId("state")).toHaveTextContent("closed");
	});

	it("restarts the delay when the pointer re-enters", () => {
		render(<TestTarget />);
		const trigger = screen.getByTestId("trigger");

		fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
		act(() => {
			vi.advanceTimersByTime(600);
		});
		fireEvent.pointerLeave(trigger);

		fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
		act(() => {
			vi.advanceTimersByTime(699);
		});
		expect(screen.getByTestId("state")).toHaveTextContent("closed");

		act(() => {
			vi.advanceTimersByTime(1);
		});
		expect(screen.getByTestId("state")).toHaveTextContent("open");
	});

	it("ignores non-mouse pointers such as touch", () => {
		render(<TestTarget />);
		const trigger = screen.getByTestId("trigger");

		fireEvent.pointerEnter(trigger, { pointerType: "touch" });
		act(() => {
			vi.advanceTimersByTime(5000);
		});

		expect(screen.getByTestId("state")).toHaveTextContent("closed");
	});

	it("cancels the pending preview and closes on drag start", () => {
		render(<TestTarget />);
		const trigger = screen.getByTestId("trigger");

		fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
		act(() => {
			vi.advanceTimersByTime(3000);
		});
		expect(screen.getByTestId("state")).toHaveTextContent("open");

		fireEvent.dragStart(trigger);
		expect(screen.getByTestId("state")).toHaveTextContent("closed");

		fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
		act(() => {
			vi.advanceTimersByTime(300);
		});
		fireEvent.dragStart(trigger);
		act(() => {
			vi.advanceTimersByTime(2000);
		});
		expect(screen.getByTestId("state")).toHaveTextContent("closed");
	});

	it("clears the pending timer on unmount", () => {
		const { unmount } = render(<TestTarget />);
		const trigger = screen.getByTestId("trigger");

		fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
		unmount();

		act(() => {
			vi.advanceTimersByTime(5000);
		});
		// 卸载后定时器不得再触发 setState（无 act 警告/崩溃即通过）
	});

	it("honours a custom delay", () => {
		render(<TestTarget delay={800} />);
		const trigger = screen.getByTestId("trigger");

		fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
		act(() => {
			vi.advanceTimersByTime(800);
		});

		expect(screen.getByTestId("state")).toHaveTextContent("open");
	});
});
