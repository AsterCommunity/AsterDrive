import {
	type ReactNode,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { cn } from "@/lib/utils";

const MEASURED_HEIGHT_DURATION_MS = 220;
const MEASURED_HEIGHT_EASING = "cubic-bezier(0.22, 1, 0.36, 1)";

function shouldReduceMotion() {
	return (
		typeof window !== "undefined" &&
		typeof window.matchMedia === "function" &&
		window.matchMedia("(prefers-reduced-motion: reduce)").matches
	);
}

export function AnimateHeight({
	show,
	children,
}: {
	show: boolean;
	children: ReactNode;
}) {
	const [render, setRender] = useState(show);
	const [visible, setVisible] = useState(show);

	useEffect(() => {
		if (show) {
			setRender(true);
			requestAnimationFrame(() => {
				requestAnimationFrame(() => setVisible(true));
			});
		} else {
			setVisible(false);
		}
	}, [show]);

	const handleTransitionEnd = () => {
		if (!show) setRender(false);
	};

	if (!render) return null;

	return (
		<div
			className="grid transition-[grid-template-rows,opacity] duration-300 ease-out"
			style={{
				gridTemplateRows: visible ? "1fr" : "0fr",
				opacity: visible ? 1 : 0,
			}}
			onTransitionEnd={handleTransitionEnd}
		>
			<div className="focus-ring-space min-h-0 overflow-hidden">{children}</div>
		</div>
	);
}

export function AnimateMeasuredHeight({
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
		container.style.transitionDuration = `${MEASURED_HEIGHT_DURATION_MS}ms`;
		container.style.transitionTimingFunction = MEASURED_HEIGHT_EASING;
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
		}, MEASURED_HEIGHT_DURATION_MS);

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
		<div
			ref={containerRef}
			className={cn("focus-ring-space box-content flow-root", className)}
		>
			<div ref={contentRef} className={contentClassName}>
				{children}
			</div>
		</div>
	);
}

export function AnimateText({
	text,
	className,
}: {
	text: string;
	className?: string;
}) {
	const [displayed, setDisplayed] = useState(text);
	const [animating, setAnimating] = useState(false);

	useEffect(() => {
		if (text === displayed) return;
		setAnimating(true);
		const timer = setTimeout(() => {
			setDisplayed(text);
			setAnimating(false);
		}, 150);
		return () => clearTimeout(timer);
	}, [text, displayed]);

	return (
		<span
			className={cn(
				"inline-block transition-all duration-150",
				animating ? "opacity-0 -translate-y-1" : "opacity-100 translate-y-0",
				className,
			)}
		>
			{displayed}
		</span>
	);
}

export function AnimateSwap({
	activeKey,
	children,
}: {
	activeKey: string;
	children: ReactNode;
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

		setVisible(false);
		const timer = setTimeout(() => {
			setRenderedKey(activeKey);
			setRenderedChildren(children);
			requestAnimationFrame(() => {
				requestAnimationFrame(() => setVisible(true));
			});
		}, 180);

		return () => clearTimeout(timer);
	}, [activeKey, children, renderedKey]);

	useEffect(() => {
		if (activeKey === renderedKey) {
			setRenderedChildren(children);
		}
	}, [activeKey, children, renderedKey]);

	return (
		<div className="focus-ring-space overflow-hidden">
			<div
				className={cn(
					"transition-all duration-200 ease-out will-change-transform",
					visible
						? "translate-y-0 opacity-100"
						: "pointer-events-none translate-y-2 opacity-0",
				)}
				aria-hidden={!visible}
			>
				{currentChildren}
			</div>
		</div>
	);
}

export function AnimateInlineSwap({
	activeKey,
	children,
}: {
	activeKey: string;
	children: ReactNode;
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

		setVisible(false);
		const timer = setTimeout(() => {
			setRenderedKey(activeKey);
			setRenderedChildren(children);
			requestAnimationFrame(() => {
				requestAnimationFrame(() => setVisible(true));
			});
		}, 180);

		return () => clearTimeout(timer);
	}, [activeKey, children, renderedKey]);

	useEffect(() => {
		if (activeKey === renderedKey) {
			setRenderedChildren(children);
		}
	}, [activeKey, children, renderedKey]);

	return (
		<span className="inline-flex overflow-hidden">
			<span
				className={cn(
					"inline-flex items-center transition-all duration-200 ease-out will-change-transform",
					visible
						? "translate-y-0 opacity-100"
						: "pointer-events-none -translate-y-1 opacity-0",
				)}
				aria-hidden={!visible}
			>
				{currentChildren}
			</span>
		</span>
	);
}
