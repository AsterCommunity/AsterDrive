import { type ReactNode, useCallback, useReducer, useRef } from "react";
import { cn } from "@/lib/utils";
import {
	MFA_STEP_SWAP_DELAY_MS,
	shouldReduceMotion,
} from "./securityMfaMotionConfig";

interface StepMotionState {
	motionId: number;
	pendingKey: string | null;
	renderedKey: string;
	visible: boolean;
}

type StepMotionAction =
	| {
			type: "startSwitch";
			activeKey: string;
			motionId: number;
			reduceMotion: boolean;
	  }
	| { type: "show"; motionId: number }
	| { type: "switchKey"; motionId: number };

function stepMotionReducer(
	state: StepMotionState,
	action: StepMotionAction,
): StepMotionState {
	switch (action.type) {
		case "startSwitch":
			if (action.reduceMotion) {
				return {
					motionId: action.motionId,
					pendingKey: null,
					renderedKey: action.activeKey,
					visible: true,
				};
			}
			return {
				...state,
				motionId: action.motionId,
				pendingKey: action.activeKey,
				visible: false,
			};
		case "show":
			return action.motionId === state.motionId
				? { ...state, visible: true }
				: state;
		case "switchKey":
			return action.motionId === state.motionId && state.pendingKey !== null
				? {
						...state,
						renderedKey: state.pendingKey,
						pendingKey: null,
					}
				: state;
	}
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
	const previousChildrenRef = useRef(children);
	const motionIdRef = useRef(0);
	const cleanupRef = useRef<(() => void) | null>(null);
	const [state, dispatchMotion] = useReducer(stepMotionReducer, {
		motionId: 0,
		pendingKey: null,
		renderedKey: activeKey,
		visible: true,
	});
	const { motionId, pendingKey, renderedKey, visible } = state;

	if (activeKey === renderedKey) {
		previousChildrenRef.current = children;
	} else if (activeKey !== pendingKey) {
		motionIdRef.current += 1;
		dispatchMotion({
			type: "startSwitch",
			activeKey,
			motionId: motionIdRef.current,
			reduceMotion: shouldReduceMotion(),
		});
	}

	const bindMotionLifecycle = useCallback(
		(node: HTMLDivElement | null) => {
			cleanupRef.current?.();
			cleanupRef.current = null;
			if (!node || pendingKey === null || visible) return;

			const timer = window.setTimeout(() => {
				dispatchMotion({
					type: "switchKey",
					motionId,
				});
				const firstFrame = window.requestAnimationFrame(() => {
					const secondFrame = window.requestAnimationFrame(() =>
						dispatchMotion({ type: "show", motionId }),
					);
					cleanupRef.current = () => {
						window.cancelAnimationFrame(firstFrame);
						window.cancelAnimationFrame(secondFrame);
					};
				});
			}, MFA_STEP_SWAP_DELAY_MS);

			cleanupRef.current = () => window.clearTimeout(timer);
		},
		[motionId, pendingKey, visible],
	);

	const currentChildren =
		activeKey === renderedKey ? children : previousChildrenRef.current;
	const hiddenTranslate =
		direction === "forward" ? "translate-x-3" : "-translate-x-3";
	const hidingOutgoingStep = !visible && activeKey !== renderedKey;

	return (
		<div className="overflow-hidden">
			<div
				ref={bindMotionLifecycle}
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
