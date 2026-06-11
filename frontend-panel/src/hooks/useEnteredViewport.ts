import { useCallback, useEffect, useState } from "react";

interface UseEnteredViewportOptions {
	enabled?: boolean;
	rootMargin?: string;
	threshold?: number | number[];
	trackVisibility?: boolean;
}

interface UseEnteredViewportResult<T extends Element> {
	ref: (nextNode: T | null) => void;
	hasEnteredViewport: boolean;
	/**
	 * Current visibility while tracking is active. When trackVisibility is false,
	 * this becomes null after the element has entered once and the observer is
	 * disconnected; use hasEnteredViewport for lazy-load decisions in that mode.
	 */
	isInViewport: boolean | null;
}

function findObserverRoot(node: Element) {
	const scrollAreaViewport = node.closest("[data-slot='scroll-area-viewport']");
	if (scrollAreaViewport instanceof Element) {
		return scrollAreaViewport;
	}

	let current = node.parentElement;
	while (current) {
		const style = window.getComputedStyle(current);
		const overflowY = style.overflowY;
		const overflowX = style.overflowX;
		const canScrollY =
			/(auto|scroll|overlay)/.test(overflowY) &&
			current.scrollHeight > current.clientHeight;
		const canScrollX =
			/(auto|scroll|overlay)/.test(overflowX) &&
			current.scrollWidth > current.clientWidth;

		if (canScrollY || canScrollX) {
			return current;
		}

		current = current.parentElement;
	}

	return null;
}

export function useEnteredViewport<T extends Element = HTMLDivElement>({
	enabled = true,
	rootMargin = "0px",
	threshold = 0,
	trackVisibility = false,
}: UseEnteredViewportOptions = {}): UseEnteredViewportResult<T> {
	const [node, setNode] = useState<T | null>(null);
	const [hasEnteredViewport, setHasEnteredViewport] = useState(false);
	const [isInViewport, setIsInViewport] = useState<boolean | null>(false);

	useEffect(() => {
		if (!enabled) {
			setHasEnteredViewport(false);
			setIsInViewport(false);
			return;
		}

		if (!node) {
			setIsInViewport((current) =>
				!trackVisibility && hasEnteredViewport && current === null
					? null
					: false,
			);
			return;
		}

		if (!trackVisibility && hasEnteredViewport) {
			return;
		}

		if (
			typeof window === "undefined" ||
			typeof window.IntersectionObserver === "undefined"
		) {
			setHasEnteredViewport(true);
			setIsInViewport(true);
			return;
		}

		const observer = new window.IntersectionObserver(
			(entries) => {
				const visible = entries.some((entry) => entry.isIntersecting);
				setIsInViewport(visible);
				if (!visible) {
					return;
				}

				setHasEnteredViewport(true);
				if (!trackVisibility) {
					setIsInViewport(null);
					observer.disconnect();
				}
			},
			{
				root: findObserverRoot(node),
				rootMargin,
				threshold,
			},
		);

		observer.observe(node);

		return () => observer.disconnect();
	}, [
		enabled,
		hasEnteredViewport,
		node,
		rootMargin,
		threshold,
		trackVisibility,
	]);

	const ref = useCallback((nextNode: T | null) => {
		setNode(nextNode);
	}, []);

	return {
		ref,
		hasEnteredViewport,
		isInViewport,
	};
}
