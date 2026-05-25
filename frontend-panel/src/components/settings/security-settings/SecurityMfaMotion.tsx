import { type ReactNode, useEffect, useLayoutEffect, useRef } from "react";
import { cn } from "@/lib/utils";
import {
	MFA_MOTION_DURATION_MS,
	MFA_MOTION_EASING,
	shouldReduceMotion,
} from "./securityMfaMotionConfig";

export function SecurityMfaMeasuredMotion({
	children,
	className,
	contentClassName,
}: {
	children: ReactNode;
	className?: string;
	contentClassName?: string;
}) {
	const containerRef = useRef<HTMLDivElement | null>(null);
	const contentRef = useRef<HTMLDivElement | null>(null);
	const previousHeightRef = useRef<number | null>(null);
	const animationIdRef = useRef(0);

	useLayoutEffect(() => {
		if (typeof window === "undefined") {
			return;
		}

		const container = containerRef.current;
		const content = contentRef.current;
		if (!container || !content) {
			return;
		}

		const nextHeight = Math.ceil(content.getBoundingClientRect().height);
		const previousHeight = previousHeightRef.current;
		previousHeightRef.current = nextHeight;

		if (
			previousHeight === null ||
			Math.abs(previousHeight - nextHeight) < 1 ||
			shouldReduceMotion()
		) {
			container.style.height = "";
			container.style.overflow = "";
			container.style.transitionProperty = "";
			container.style.transitionDuration = "";
			container.style.transitionTimingFunction = "";
			return;
		}

		const animationId = animationIdRef.current + 1;
		animationIdRef.current = animationId;
		let frame: number | null = null;
		let timer: number | null = null;

		container.style.height = `${previousHeight}px`;
		container.style.overflow = "hidden";
		container.style.transitionProperty = "height";
		container.style.transitionDuration = `${MFA_MOTION_DURATION_MS}ms`;
		container.style.transitionTimingFunction = MFA_MOTION_EASING;
		container.getBoundingClientRect();

		frame = window.requestAnimationFrame(() => {
			if (animationIdRef.current !== animationId) {
				return;
			}
			container.style.height = `${nextHeight}px`;
		});

		timer = window.setTimeout(() => {
			if (animationIdRef.current !== animationId) {
				return;
			}
			previousHeightRef.current = Math.ceil(
				content.getBoundingClientRect().height,
			);
			container.style.height = "";
			container.style.overflow = "";
			container.style.transitionProperty = "";
			container.style.transitionDuration = "";
			container.style.transitionTimingFunction = "";
		}, MFA_MOTION_DURATION_MS);

		return () => {
			if (frame !== null) {
				window.cancelAnimationFrame(frame);
			}
			if (timer !== null) {
				window.clearTimeout(timer);
			}
		};
	});

	useEffect(() => {
		if (typeof ResizeObserver === "undefined") {
			return;
		}

		const content = contentRef.current;
		if (!content) {
			return;
		}

		const observer = new ResizeObserver(() => {
			const container = containerRef.current;
			if (container?.style.height) {
				return;
			}
			previousHeightRef.current = Math.ceil(
				content.getBoundingClientRect().height,
			);
		});
		observer.observe(content);
		return () => observer.disconnect();
	}, []);

	return (
		<div ref={containerRef} className={cn("flow-root", className)}>
			<div ref={contentRef} className={contentClassName}>
				{children}
			</div>
		</div>
	);
}
