import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useEnteredViewport } from "@/hooks/useEnteredViewport";

class MockIntersectionObserver {
	static instances: MockIntersectionObserver[] = [];

	disconnect = vi.fn();
	observe = vi.fn();
	root: Element | null;
	rootMargin: string;
	thresholds: number[];
	unobserve = vi.fn();

	private readonly callback: IntersectionObserverCallback;

	constructor(
		callback: IntersectionObserverCallback,
		options: IntersectionObserverInit = {},
	) {
		this.callback = callback;
		this.root =
			options.root instanceof Element ? options.root : (options.root ?? null);
		this.rootMargin = options.rootMargin ?? "";
		this.thresholds = Array.isArray(options.threshold)
			? options.threshold
			: options.threshold !== undefined
				? [options.threshold]
				: [];

		MockIntersectionObserver.instances.push(this);
	}

	takeRecords() {
		return [];
	}

	trigger(entry: Partial<IntersectionObserverEntry>) {
		this.callback(
			[
				{
					boundingClientRect: entry.boundingClientRect ?? DOMRect.fromRect(),
					intersectionRatio: entry.intersectionRatio ?? 1,
					intersectionRect: entry.intersectionRect ?? DOMRect.fromRect(),
					isIntersecting: entry.isIntersecting ?? true,
					rootBounds: entry.rootBounds ?? null,
					target: entry.target ?? document.body,
					time: entry.time ?? 0,
				} as IntersectionObserverEntry,
			],
			this as unknown as IntersectionObserver,
		);
	}
}

function TestComponent({
	enabled = true,
	rootMargin,
	trackVisibility,
}: {
	enabled?: boolean;
	rootMargin?: string;
	trackVisibility?: boolean;
}) {
	const { ref, hasEnteredViewport, isInViewport } =
		useEnteredViewport<HTMLDivElement>({
			enabled,
			rootMargin,
			trackVisibility,
		});

	return (
		<div
			ref={ref}
			data-testid="target"
			data-entered={String(hasEnteredViewport)}
			data-visible={String(isInViewport)}
		/>
	);
}

describe("useEnteredViewport", () => {
	let originalIntersectionObserver = window.IntersectionObserver;

	beforeEach(() => {
		MockIntersectionObserver.instances = [];
		originalIntersectionObserver = window.IntersectionObserver;
		Object.defineProperty(window, "IntersectionObserver", {
			writable: true,
			value: MockIntersectionObserver,
		});
	});

	afterEach(() => {
		Object.defineProperty(window, "IntersectionObserver", {
			writable: true,
			value: originalIntersectionObserver,
		});
	});

	it("marks the element entered after it intersects the current scroll area viewport", async () => {
		render(
			<div data-slot="scroll-area-viewport" data-testid="viewport">
				<TestComponent rootMargin="24px" />
			</div>,
		);

		expect(screen.getByTestId("target")).toHaveAttribute(
			"data-entered",
			"false",
		);
		expect(MockIntersectionObserver.instances).toHaveLength(1);
		const observer = MockIntersectionObserver.instances[0];
		expect(observer?.root).toBe(screen.getByTestId("viewport"));
		expect(observer?.rootMargin).toBe("24px");

		act(() => {
			const target = screen.getByTestId("target");
			observer?.trigger({ isIntersecting: true, target });
		});

		await waitFor(() =>
			expect(screen.getByTestId("target")).toHaveAttribute(
				"data-entered",
				"true",
			),
		);
		expect(screen.getByTestId("target")).toHaveAttribute(
			"data-visible",
			"null",
		);
		expect(observer?.disconnect).toHaveBeenCalled();
	});

	it("disconnects after entering when visibility tracking is disabled", async () => {
		render(<TestComponent trackVisibility={false} />);

		const observer = MockIntersectionObserver.instances[0];
		const target = screen.getByTestId("target");

		act(() => {
			observer?.trigger({ isIntersecting: true, target });
		});

		await waitFor(() => expect(target).toHaveAttribute("data-entered", "true"));
		expect(target).toHaveAttribute("data-visible", "null");
		expect(observer?.disconnect).toHaveBeenCalled();
	});

	it("keeps observing after entering when visibility tracking is enabled", async () => {
		render(<TestComponent trackVisibility />);

		const observer = MockIntersectionObserver.instances[0];
		const target = screen.getByTestId("target");

		act(() => {
			observer?.trigger({ isIntersecting: true, target });
		});

		await waitFor(() => expect(target).toHaveAttribute("data-entered", "true"));
		expect(target).toHaveAttribute("data-visible", "true");
		const activeObserver =
			MockIntersectionObserver.instances[
				MockIntersectionObserver.instances.length - 1
			];
		expect(activeObserver?.disconnect).not.toHaveBeenCalled();
	});

	it("tracks when the element leaves the viewport when visibility tracking is enabled", async () => {
		render(<TestComponent trackVisibility />);

		const observer = MockIntersectionObserver.instances[0];
		const target = screen.getByTestId("target");

		act(() => {
			observer?.trigger({ isIntersecting: true, target });
		});

		await waitFor(() => expect(target).toHaveAttribute("data-entered", "true"));
		expect(target).toHaveAttribute("data-visible", "true");

		const activeObserver =
			MockIntersectionObserver.instances[
				MockIntersectionObserver.instances.length - 1
			];

		act(() => {
			activeObserver?.trigger({ isIntersecting: false, target });
		});

		await waitFor(() =>
			expect(target).toHaveAttribute("data-visible", "false"),
		);
		expect(target).toHaveAttribute("data-entered", "true");
	});

	it("does not observe while disabled", () => {
		render(<TestComponent enabled={false} />);

		expect(screen.getByTestId("target")).toHaveAttribute(
			"data-entered",
			"false",
		);
		expect(screen.getByTestId("target")).toHaveAttribute(
			"data-visible",
			"false",
		);
		expect(MockIntersectionObserver.instances).toHaveLength(0);
	});

	it("falls back to entered when IntersectionObserver is unavailable", async () => {
		Object.defineProperty(window, "IntersectionObserver", {
			writable: true,
			value: undefined,
		});

		render(<TestComponent />);

		await waitFor(() =>
			expect(screen.getByTestId("target")).toHaveAttribute(
				"data-entered",
				"true",
			),
		);
	});
});
