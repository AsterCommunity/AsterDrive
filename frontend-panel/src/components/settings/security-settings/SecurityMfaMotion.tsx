import {
	type ReactNode,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { cn } from "@/lib/utils";

const MFA_MOTION_DURATION_MS = 240;
const MFA_MOTION_EASING = "cubic-bezier(0.22, 1, 0.36, 1)";
const MFA_STEP_SWAP_DELAY_MS = 120;

function shouldReduceMotion() {
	return (
		typeof window !== "undefined" &&
		typeof window.matchMedia === "function" &&
		window.matchMedia("(prefers-reduced-motion: reduce)").matches
	);
}

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

export function SecurityMfaPresence({
	children,
	className,
	show,
}: {
	children: ReactNode;
	className?: string;
	show: boolean;
}) {
	const [mounted, setMounted] = useState(show);
	const [renderedChildren, setRenderedChildren] = useState(children);
	const [visible, setVisible] = useState(show);

	useEffect(() => {
		if (show) {
			setRenderedChildren(children);
		}
	}, [children, show]);

	useEffect(() => {
		if (shouldReduceMotion()) {
			setMounted(show);
			setVisible(show);
			return;
		}

		if (show) {
			setMounted(true);
			requestAnimationFrame(() => {
				requestAnimationFrame(() => setVisible(true));
			});
			return;
		}

		setVisible(false);
	}, [show]);

	const handleTransitionEnd = () => {
		if (!show) {
			setMounted(false);
		}
	};

	if (!mounted) {
		return null;
	}

	return (
		<div
			aria-hidden={!show && !visible}
			className={cn(
				"grid transition-[grid-template-rows,opacity,transform] duration-[240ms] ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none",
				visible ? "translate-y-0 opacity-100" : "-translate-y-1 opacity-0",
				className,
			)}
			style={{ gridTemplateRows: visible ? "1fr" : "0fr" }}
			onTransitionEnd={handleTransitionEnd}
		>
			<div className="min-h-0 overflow-hidden">
				{show ? children : renderedChildren}
			</div>
		</div>
	);
}

export function SecurityMfaStepMotion({
	activeKey,
	children,
	direction = "forward",
}: {
	activeKey: string;
	children: ReactNode;
	direction?: "backward" | "forward";
}) {
	const [renderedKey, setRenderedKey] = useState(activeKey);
	const [renderedChildren, setRenderedChildren] = useState(children);
	const [visible, setVisible] = useState(true);
	const currentChildren =
		activeKey === renderedKey ? children : renderedChildren;

	useEffect(() => {
		if (activeKey === renderedKey) {
			setRenderedChildren(children);
			return;
		}

		if (shouldReduceMotion()) {
			setRenderedKey(activeKey);
			setRenderedChildren(children);
			setVisible(true);
			return;
		}

		setVisible(false);
		const timer = window.setTimeout(() => {
			setRenderedKey(activeKey);
			setRenderedChildren(children);
			requestAnimationFrame(() => {
				requestAnimationFrame(() => setVisible(true));
			});
		}, MFA_STEP_SWAP_DELAY_MS);

		return () => window.clearTimeout(timer);
	}, [activeKey, children, renderedKey]);

	useEffect(() => {
		if (activeKey === renderedKey) {
			setRenderedChildren(children);
		}
	}, [activeKey, children, renderedKey]);

	const hiddenTranslate =
		direction === "forward" ? "translate-x-3" : "-translate-x-3";
	const hidingOutgoingStep = !visible && activeKey !== renderedKey;

	return (
		<div className="overflow-hidden">
			<div
				aria-hidden={hidingOutgoingStep}
				className={cn(
					"transition-[opacity,transform] duration-[220ms] ease-[cubic-bezier(0.22,1,0.36,1)] will-change-transform motion-reduce:transition-none",
					visible
						? "translate-x-0 opacity-100"
						: `pointer-events-none ${hiddenTranslate} opacity-0`,
				)}
			>
				{currentChildren}
			</div>
		</div>
	);
}
