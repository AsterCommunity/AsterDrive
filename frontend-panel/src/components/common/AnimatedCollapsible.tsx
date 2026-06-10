import { type ReactNode, useLayoutEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

const EXPAND_DURATION_MS = 220;
const COLLAPSE_DURATION_MS = 160;

function setContainerStyle(
	element: HTMLDivElement,
	values: {
		maxHeight: string;
		opacity: string;
		transform: string;
		transitionDuration?: string;
		transitionTimingFunction?: string;
	},
) {
	element.style.cssText = [
		"overflow: hidden",
		"transition-property: max-height, opacity, transform",
		`transition-duration: ${
			values.transitionDuration ?? element.style.transitionDuration
		}`,
		`transition-timing-function: ${
			values.transitionTimingFunction ?? element.style.transitionTimingFunction
		}`,
		`max-height: ${values.maxHeight}`,
		`opacity: ${values.opacity}`,
		`transform: ${values.transform}`,
	].join(";");
}

export function AnimatedCollapsible({
	children,
	className,
	contentClassName,
	open,
}: {
	children: ReactNode;
	className?: string;
	contentClassName?: string;
	open: boolean;
}) {
	const containerRef = useRef<HTMLDivElement | null>(null);
	const contentRef = useRef<HTMLDivElement | null>(null);
	const [keepMounted, setKeepMounted] = useState(false);

	const shouldRender = open || keepMounted;

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
				? EXPAND_DURATION_MS
				: COLLAPSE_DURATION_MS;
		let frameA: number | null = null;
		let frameB: number | null = null;
		let timer: number | null = null;
		const fullHeight = `${content.scrollHeight}px`;

		const baseStyle = {
			transitionDuration: `${duration}ms`,
			transitionTimingFunction: open
				? "cubic-bezier(0.22, 1, 0.36, 1)"
				: "cubic-bezier(0.4, 0, 1, 1)",
		};

		if (open) {
			setKeepMounted(true);
			setContainerStyle(container, {
				...baseStyle,
				maxHeight: "0px",
				opacity: "0",
				transform: "translateY(-4px)",
			});
			frameA = window.requestAnimationFrame(() => {
				frameB = window.requestAnimationFrame(() => {
					setContainerStyle(container, {
						maxHeight: fullHeight,
						opacity: "1",
						transform: "translateY(0)",
					});
				});
			});
			timer = window.setTimeout(() => {
				setContainerStyle(container, {
					maxHeight: "none",
					opacity: "1",
					transform: "translateY(0)",
				});
			}, duration);
		} else {
			setContainerStyle(container, {
				...baseStyle,
				maxHeight: fullHeight,
				opacity: "1",
				transform: "translateY(0)",
			});
			frameA = window.requestAnimationFrame(() => {
				setContainerStyle(container, {
					maxHeight: "0px",
					opacity: "0",
					transform: "translateY(-4px)",
				});
			});
			timer = window.setTimeout(() => {
				setKeepMounted(false);
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
			<div ref={contentRef} className={cn("min-h-0", contentClassName)}>
				{children}
			</div>
		</div>
	);
}
