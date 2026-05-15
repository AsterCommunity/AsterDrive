import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";

export function AnimateHeight({
	show,
	children,
}: {
	show: boolean;
	children: React.ReactNode;
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
			<div className="overflow-hidden">{children}</div>
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
	children: React.ReactNode;
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
		<div className="overflow-hidden">
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
	children: React.ReactNode;
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
