import { fireEvent, render, screen } from "@testing-library/react";
import { useRef, useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useImagePreviewTransform } from "./useImagePreviewTransform";

function TransformHarness({
	gesturesEnabled = true,
	renderViewport = true,
}: {
	gesturesEnabled?: boolean;
	renderViewport?: boolean;
}) {
	const imageRef = useRef<HTMLImageElement | null>(null);
	const viewportRef = useRef<HTMLDivElement | null>(null);
	const { zoomPercent } = useImagePreviewTransform({
		gesturesEnabled,
		imageRef,
		viewportRef,
	});

	return (
		<div>
			<div data-testid="zoom">{zoomPercent}</div>
			<img alt="" ref={imageRef} />
			{renderViewport ? <div data-testid="viewport" ref={viewportRef} /> : null}
		</div>
	);
}

function PointerCleanupHarness() {
	const [gesturesEnabled, setGesturesEnabled] = useState(true);
	return (
		<div>
			<TransformHarness gesturesEnabled={gesturesEnabled} />
			<button type="button" onClick={() => setGesturesEnabled(false)}>
				disable-gestures
			</button>
			<button type="button" onClick={() => setGesturesEnabled(true)}>
				enable-gestures
			</button>
		</div>
	);
}

describe("useImagePreviewTransform", () => {
	beforeEach(() => {
		Object.defineProperty(HTMLElement.prototype, "setPointerCapture", {
			configurable: true,
			value: vi.fn(),
		});
		Object.defineProperty(HTMLElement.prototype, "hasPointerCapture", {
			configurable: true,
			value: vi.fn(() => true),
		});
		Object.defineProperty(HTMLElement.prototype, "releasePointerCapture", {
			configurable: true,
			value: vi.fn(),
		});
	});

	it("uses the viewport as the wheel target when no gesture surface is provided", () => {
		render(<TransformHarness />);

		fireEvent.wheel(screen.getByTestId("viewport"), {
			clientX: 100,
			clientY: 100,
			ctrlKey: true,
			deltaY: -100,
		});

		expect(screen.getByTestId("zoom")).toHaveTextContent("125");
	});

	it("skips wheel listener setup when no wheel target is mounted", () => {
		render(<TransformHarness renderViewport={false} />);

		expect(screen.getByTestId("zoom")).toHaveTextContent("100");
	});

	it("clears active pointers when gestures are disabled", () => {
		render(<PointerCleanupHarness />);
		const viewport = screen.getByTestId("viewport");

		fireEvent.pointerDown(viewport, {
			clientX: 20,
			clientY: 20,
			pointerId: 1,
		});
		fireEvent.click(screen.getByRole("button", { name: "disable-gestures" }));
		fireEvent.click(screen.getByRole("button", { name: "enable-gestures" }));
		fireEvent.pointerDown(viewport, {
			clientX: 40,
			clientY: 20,
			pointerId: 2,
		});
		fireEvent.pointerMove(viewport, {
			clientX: 120,
			clientY: 20,
			pointerId: 2,
		});

		expect(screen.getByTestId("zoom")).toHaveTextContent("100");
	});
});
