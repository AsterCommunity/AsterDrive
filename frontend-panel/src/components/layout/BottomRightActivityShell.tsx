import {
	createContext,
	type ReactNode,
	use,
	useCallback,
	useLayoutEffect,
	useState,
} from "react";
import { createPortal } from "react-dom";
import { BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY } from "@/lib/constants";

const BottomRightActivityShellContext = createContext<
	HTMLDivElement | null | undefined
>(undefined);

export function BottomRightActivityShell({
	children,
}: {
	children: ReactNode;
}) {
	const [container, setContainer] = useState<HTMLDivElement | null>(null);
	const handleContainerRef = useCallback((node: HTMLDivElement | null) => {
		setContainer(node);
	}, []);

	useLayoutEffect(() => {
		if (!container) return;
		const root = document.documentElement;
		const updateHeight = () => {
			root.style.setProperty(
				BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY,
				`${Math.ceil(container.getBoundingClientRect().height)}px`,
			);
		};
		updateHeight();
		const observer =
			typeof ResizeObserver === "function"
				? new ResizeObserver(updateHeight)
				: null;
		observer?.observe(container);
		window.addEventListener("resize", updateHeight);
		return () => {
			observer?.disconnect();
			window.removeEventListener("resize", updateHeight);
			root.style.removeProperty(BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY);
		};
	}, [container]);

	return (
		<BottomRightActivityShellContext value={container}>
			{children}
			<div
				ref={handleContainerRef}
				data-testid="bottom-right-activity-shell"
				className="pointer-events-none fixed right-4 bottom-4 z-(--z-fixed) flex w-[28rem] max-w-[calc(100vw-2rem)] flex-col divide-y divide-border/60 overflow-hidden rounded-lg bg-card/95 shadow-lg shadow-black/10 ring-1 ring-border/60 backdrop-blur-sm empty:hidden dark:bg-card/85 dark:shadow-none dark:ring-border/70"
			/>
		</BottomRightActivityShellContext>
	);
}

export function BottomRightActivityPortal({
	children,
}: {
	children: ReactNode;
}) {
	const container = use(BottomRightActivityShellContext);
	if (container === undefined) return children;
	return container ? createPortal(children, container) : null;
}
