import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export function AnimatedTreeGroup({
	children,
	className,
	open,
}: {
	children: ReactNode;
	className?: string;
	open: boolean;
}) {
	return (
		<div
			aria-hidden={!open}
			inert={!open}
			className={cn(
				"grid overflow-hidden transition-[grid-template-rows] duration-[180ms] ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none",
				open ? "grid-rows-[1fr]" : "grid-rows-[0fr]",
				className,
			)}
		>
			<div
				className={cn(
					"min-h-0 origin-top overflow-hidden transition-transform duration-[180ms] ease-[cubic-bezier(0.22,1,0.36,1)] will-change-transform motion-reduce:transition-none",
					open ? "scale-y-100" : "scale-y-0",
				)}
			>
				{children}
			</div>
		</div>
	);
}
