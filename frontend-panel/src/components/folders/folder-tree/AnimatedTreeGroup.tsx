import {
	type ReactNode,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { cn } from "@/lib/utils";

const TREE_GROUP_EXPAND_DURATION_MS = 180;
const TREE_GROUP_COLLAPSE_DURATION_MS = 140;
const TREE_GROUP_EXPAND_EASING = "cubic-bezier(0.22, 1, 0.36, 1)";
const TREE_GROUP_COLLAPSE_EASING = "cubic-bezier(0.4, 0, 1, 1)";

function setTreeGroupStyle(
	element: HTMLDivElement,
	values: {
		maxHeight: string;
		transitionDuration?: string;
		transitionTimingFunction?: string;
	},
) {
	element.style.cssText = [
		"overflow: hidden",
		"transition-property: max-height",
		`transition-duration: ${
			values.transitionDuration ?? element.style.transitionDuration
		}`,
		`transition-timing-function: ${
			values.transitionTimingFunction ?? element.style.transitionTimingFunction
		}`,
		`max-height: ${values.maxHeight}`,
	].join(";");
}

function setTreeGroupContentStyle(
	element: HTMLDivElement,
	values: {
		scaleY: number;
		transitionDuration?: string;
		transitionTimingFunction?: string;
	},
) {
	element.style.cssText = [
		"min-height: 0",
		"transform-origin: top",
		"will-change: transform",
		"transition-property: transform",
		`transition-duration: ${
			values.transitionDuration ?? element.style.transitionDuration
		}`,
		`transition-timing-function: ${
			values.transitionTimingFunction ?? element.style.transitionTimingFunction
		}`,
		`transform: scaleY(${values.scaleY})`,
	].join(";");
}

function calcScale(currentHeight: number, fullHeight: number) {
	if (fullHeight <= 0) {
		return 1;
	}
	return Math.min(1, Math.max(0, currentHeight / fullHeight));
}

export function AnimatedTreeGroup({
	children,
	className,
	open,
}: {
	children: ReactNode;
	className?: string;
	open: boolean;
}) {
	const containerRef = useRef<HTMLDivElement | null>(null);
	const contentRef = useRef<HTMLDivElement | null>(null);
	const initialOpenRef = useRef(open);
	const hasRenderedDomRef = useRef(false);
	const fullyClosedRef = useRef(!open);
	const [mounted, setMounted] = useState(open);

	const shouldRender = open || mounted;

	useEffect(() => {
		if (typeof window === "undefined") {
			setMounted(open);
			return;
		}

		if (open) {
			setMounted(true);
		}
	}, [open]);

	useLayoutEffect(() => {
		if (typeof window === "undefined" || !shouldRender) {
			return;
		}

		const container = containerRef.current;
		const content = contentRef.current;
		if (!container || !content) {
			return;
		}

		const prefersReducedMotion =
			typeof window.matchMedia === "function" &&
			window.matchMedia("(prefers-reduced-motion: reduce)").matches;
		const duration = prefersReducedMotion
			? 0
			: open
				? TREE_GROUP_EXPAND_DURATION_MS
				: TREE_GROUP_COLLAPSE_DURATION_MS;
		let frameA: number | null = null;
		let frameB: number | null = null;
		let timer: number | null = null;
		const fullHeight = `${content.scrollHeight}px`;
		const firstDomRender = !hasRenderedDomRef.current;
		const shouldSkipInitialOpen =
			open && firstDomRender && initialOpenRef.current;

		hasRenderedDomRef.current = true;

		if (shouldSkipInitialOpen || duration === 0) {
			setTreeGroupStyle(container, {
				maxHeight: open ? "none" : "0px",
				transitionDuration: "0ms",
				transitionTimingFunction: open
					? TREE_GROUP_EXPAND_EASING
					: TREE_GROUP_COLLAPSE_EASING,
			});
			setTreeGroupContentStyle(content, {
				scaleY: open ? 1 : 0,
				transitionDuration: "0ms",
				transitionTimingFunction: open
					? TREE_GROUP_EXPAND_EASING
					: TREE_GROUP_COLLAPSE_EASING,
			});
			fullyClosedRef.current = !open;
			if (!open) {
				setMounted(false);
			}
			return;
		}

		if (open) {
			const currentHeight = fullyClosedRef.current
				? 0
				: container.getBoundingClientRect().height;
			fullyClosedRef.current = false;
			setTreeGroupStyle(container, {
				maxHeight: `${currentHeight}px`,
				transitionDuration: `${duration}ms`,
				transitionTimingFunction: TREE_GROUP_EXPAND_EASING,
			});
			setTreeGroupContentStyle(content, {
				scaleY: calcScale(currentHeight, content.scrollHeight),
				transitionDuration: `${duration}ms`,
				transitionTimingFunction: TREE_GROUP_EXPAND_EASING,
			});
			frameA = window.requestAnimationFrame(() => {
				frameB = window.requestAnimationFrame(() => {
					setTreeGroupStyle(container, {
						maxHeight: fullHeight,
					});
					setTreeGroupContentStyle(content, {
						scaleY: 1,
					});
				});
			});
			timer = window.setTimeout(() => {
				setTreeGroupStyle(container, {
					maxHeight: "none",
				});
				setTreeGroupContentStyle(content, {
					scaleY: 1,
				});
			}, duration);
		} else {
			const currentHeight = container.getBoundingClientRect().height;
			setTreeGroupStyle(container, {
				maxHeight: `${currentHeight || content.scrollHeight}px`,
				transitionDuration: `${duration}ms`,
				transitionTimingFunction: TREE_GROUP_COLLAPSE_EASING,
			});
			setTreeGroupContentStyle(content, {
				scaleY: calcScale(
					currentHeight || content.scrollHeight,
					content.scrollHeight,
				),
				transitionDuration: `${duration}ms`,
				transitionTimingFunction: TREE_GROUP_COLLAPSE_EASING,
			});
			frameA = window.requestAnimationFrame(() => {
				setTreeGroupStyle(container, {
					maxHeight: "0px",
				});
				setTreeGroupContentStyle(content, {
					scaleY: 0,
				});
			});
			timer = window.setTimeout(() => {
				fullyClosedRef.current = true;
				setMounted(false);
			}, duration);
		}

		return () => {
			if (frameA !== null) {
				window.cancelAnimationFrame(frameA);
			}
			if (frameB !== null) {
				window.cancelAnimationFrame(frameB);
			}
			if (timer !== null) {
				window.clearTimeout(timer);
			}
		};
	}, [open, shouldRender]);

	if (!shouldRender) {
		return null;
	}

	return (
		<div
			ref={containerRef}
			aria-hidden={!open}
			className={cn("overflow-hidden", className)}
		>
			<div ref={contentRef} className="min-h-0">
				{children}
			</div>
		</div>
	);
}
