import {
	type ReactNode,
	type TransitionEvent,
	useCallback,
	useReducer,
	useRef,
} from "react";
import { cn } from "@/lib/utils";
import {
	MFA_MOTION_DURATION_MS,
	shouldReduceMotion,
} from "./securityMfaMotionConfig";

type PresencePhase = "entered" | "entering" | "exited" | "exiting";
type PresenceAction =
	| { type: "show"; motionId: number; reduceMotion: boolean }
	| { type: "enter"; motionId: number }
	| { type: "hide"; motionId: number; reduceMotion: boolean }
	| { type: "exit"; motionId: number };

interface PresenceState {
	motionId: number;
	phase: PresencePhase;
	show: boolean;
}

function presenceReducer(
	state: PresenceState,
	action: PresenceAction,
): PresenceState {
	switch (action.type) {
		case "show":
			return {
				motionId: action.motionId,
				phase: action.reduceMotion ? "entered" : "entering",
				show: true,
			};
		case "enter":
			return action.motionId === state.motionId
				? { ...state, phase: "entered" }
				: state;
		case "hide":
			return {
				motionId: action.motionId,
				phase:
					action.reduceMotion || state.phase === "exited"
						? "exited"
						: "exiting",
				show: false,
			};
		case "exit":
			return action.motionId === state.motionId
				? { ...state, phase: "exited" }
				: state;
	}
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
	const lastShownChildrenRef = useRef(children);
	const motionIdRef = useRef(0);
	const cleanupRef = useRef<(() => void) | null>(null);
	const [state, dispatchPhase] = useReducer(presenceReducer, {
		motionId: 0,
		phase: show ? "entered" : "exited",
		show,
	});
	const { motionId, phase } = state;

	if (show) {
		lastShownChildrenRef.current = children;
	}

	if (show !== state.show) {
		motionIdRef.current += 1;
		const reduceMotion = shouldReduceMotion();
		dispatchPhase(
			show
				? { type: "show", motionId: motionIdRef.current, reduceMotion }
				: { type: "hide", motionId: motionIdRef.current, reduceMotion },
		);
	}

	const bindMotionLifecycle = useCallback(
		(node: HTMLDivElement | null) => {
			cleanupRef.current?.();
			cleanupRef.current = null;
			if (!node) return;

			if (phase === "entering") {
				let firstFrame: number | null = null;
				let secondFrame: number | null = null;
				firstFrame = window.requestAnimationFrame(() => {
					secondFrame = window.requestAnimationFrame(() =>
						dispatchPhase({ type: "enter", motionId }),
					);
				});
				cleanupRef.current = () => {
					if (firstFrame !== null) {
						window.cancelAnimationFrame(firstFrame);
					}
					if (secondFrame !== null) {
						window.cancelAnimationFrame(secondFrame);
					}
				};
				return;
			}

			if (phase === "exiting") {
				const timer = window.setTimeout(() => {
					dispatchPhase({ type: "exit", motionId });
				}, MFA_MOTION_DURATION_MS);
				cleanupRef.current = () => window.clearTimeout(timer);
			}
		},
		[motionId, phase],
	);

	const visible = phase === "entered";
	const mounted = phase !== "exited";

	const handleTransitionEnd = (event: TransitionEvent<HTMLDivElement>) => {
		if (!show && event.target === event.currentTarget) {
			dispatchPhase({ type: "exit", motionId });
		}
	};

	if (!mounted) {
		return null;
	}

	return (
		<div
			ref={bindMotionLifecycle}
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
				{show ? children : lastShownChildrenRef.current}
			</div>
		</div>
	);
}
