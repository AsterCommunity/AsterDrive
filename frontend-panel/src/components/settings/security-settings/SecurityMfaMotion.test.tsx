import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SecurityMfaMeasuredMotion } from "./SecurityMfaMotion";
import { SecurityMfaPresence } from "./SecurityMfaPresence";
import { SecurityMfaStepMotion } from "./SecurityMfaStepMotion";

const originalGetBoundingClientRect =
	HTMLElement.prototype.getBoundingClientRect;

function createRect(height: number): DOMRect {
	return {
		bottom: height,
		height,
		left: 0,
		right: 320,
		toJSON: () => ({}),
		top: 0,
		width: 320,
		x: 0,
		y: 0,
	} satisfies DOMRect;
}

function measuredHeight(element: Element) {
	if (!(element instanceof HTMLElement)) {
		return 0;
	}

	const explicitHeight = element.dataset.height
		? Number(element.dataset.height)
		: null;
	if (explicitHeight !== null) {
		return explicitHeight;
	}

	let childrenHeight = 0;
	for (const child of Array.from(element.children)) {
		childrenHeight += measuredHeight(child);
	}
	return childrenHeight;
}

describe("SecurityMfaMotion", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
			return window.setTimeout(() => callback(performance.now()), 0);
		});
		vi.spyOn(window, "cancelAnimationFrame").mockImplementation((id) => {
			window.clearTimeout(id);
		});
		HTMLElement.prototype.getBoundingClientRect = function () {
			const measuredElement = this instanceof HTMLElement ? this : null;
			if (!measuredElement?.querySelector("[data-height]")) {
				return originalGetBoundingClientRect.call(this);
			}
			return createRect(measuredHeight(measuredElement));
		};
	});

	afterEach(() => {
		vi.restoreAllMocks();
		vi.useRealTimers();
		HTMLElement.prototype.getBoundingClientRect = originalGetBoundingClientRect;
	});

	it("animates measured height changes inside the MFA stack", () => {
		const view = render(
			<SecurityMfaMeasuredMotion>
				<div data-height="80">empty state</div>
			</SecurityMfaMeasuredMotion>,
		);
		const container = view.container.firstElementChild as HTMLDivElement;

		view.rerender(
			<SecurityMfaMeasuredMotion>
				<div data-height="144">setup panel</div>
			</SecurityMfaMeasuredMotion>,
		);

		expect(container.style.height).toBe("80px");
		expect(container.style.overflow).toBe("hidden");

		act(() => {
			vi.advanceTimersByTime(0);
		});

		expect(container.style.height).toBe("144px");

		act(() => {
			vi.advanceTimersByTime(240);
		});

		expect(container.style.height).toBe("");
		expect(container.style.overflow).toBe("");
	});

	it("keeps leaving content mounted until the presence exit finishes", () => {
		const view = render(
			<SecurityMfaPresence show={false}>
				<span>mfa-action-form</span>
			</SecurityMfaPresence>,
		);

		expect(screen.queryByText("mfa-action-form")).not.toBeInTheDocument();

		view.rerender(
			<SecurityMfaPresence show>
				<span>mfa-action-form</span>
			</SecurityMfaPresence>,
		);
		const container = view.container.firstElementChild as HTMLDivElement;

		expect(screen.getByText("mfa-action-form")).toBeInTheDocument();
		expect(container).toHaveAttribute("aria-hidden", "false");
		expect(container.style.gridTemplateRows).toBe("0fr");

		act(() => {
			vi.runOnlyPendingTimers();
			vi.runOnlyPendingTimers();
		});
		expect(container.style.gridTemplateRows).toBe("1fr");

		view.rerender(
			<SecurityMfaPresence show={false}>
				<span>mfa-action-form</span>
			</SecurityMfaPresence>,
		);

		expect(screen.getByText("mfa-action-form")).toBeInTheDocument();
		expect(container).toHaveAttribute("aria-hidden", "true");
		expect(container.style.gridTemplateRows).toBe("0fr");

		fireEvent.transitionEnd(screen.getByText("mfa-action-form"));

		expect(screen.getByText("mfa-action-form")).toBeInTheDocument();

		fireEvent.transitionEnd(container);

		expect(screen.queryByText("mfa-action-form")).not.toBeInTheDocument();
	});

	it("delays setup step swaps and uses the supplied direction", () => {
		const view = render(
			<SecurityMfaStepMotion activeKey="scan" direction="forward">
				<span>scan-step</span>
			</SecurityMfaStepMotion>,
		);
		const animated = view.container.querySelector("[aria-hidden]");
		if (!animated) throw new Error("animated step element not found");

		view.rerender(
			<SecurityMfaStepMotion activeKey="verify" direction="forward">
				<span>verify-step</span>
			</SecurityMfaStepMotion>,
		);

		expect(screen.getByText("scan-step")).toBeInTheDocument();
		expect(animated).toHaveAttribute("aria-hidden", "true");
		expect(animated).toHaveClass("translate-x-3");

		act(() => {
			vi.advanceTimersByTime(120);
			vi.runOnlyPendingTimers();
			vi.runOnlyPendingTimers();
		});

		expect(screen.getByText("verify-step")).toBeInTheDocument();
		expect(animated).toHaveAttribute("aria-hidden", "false");

		view.rerender(
			<SecurityMfaStepMotion activeKey="scan" direction="backward">
				<span>scan-step-again</span>
			</SecurityMfaStepMotion>,
		);

		expect(screen.getByText("verify-step")).toBeInTheDocument();
		expect(animated).toHaveAttribute("aria-hidden", "true");
		expect(animated).toHaveClass("-translate-x-3");

		act(() => {
			vi.advanceTimersByTime(120);
		});

		expect(screen.getByText("scan-step-again")).toBeInTheDocument();
		expect(animated).toHaveAttribute("aria-hidden", "false");
	});
});
