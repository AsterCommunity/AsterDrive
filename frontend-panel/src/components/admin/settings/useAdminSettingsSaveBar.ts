import { useEffect, useRef, useState } from "react";

type SaveBarPhase = "hidden" | "entering" | "visible" | "exiting";

const EXIT_UNMOUNT_GRACE_MS = 50;
const NEXT_FRAME_DELAY_MS = 16;

interface UseAdminSettingsSaveBarProps {
	desktopMinReservedHeight: number;
	enterDurationMs: number;
	exitDurationMs: number;
	hasUnsavedChanges: boolean;
	mobileBreakpoint: number;
	mobileMinReservedHeight: number;
	viewportWidth: number;
}

export function useAdminSettingsSaveBar({
	desktopMinReservedHeight,
	enterDurationMs,
	exitDurationMs,
	hasUnsavedChanges,
	mobileBreakpoint,
	mobileMinReservedHeight,
	viewportWidth,
}: UseAdminSettingsSaveBarProps) {
	const timerRef = useRef<number | null>(null);
	const phaseRef = useRef<SaveBarPhase>("hidden");
	const measureRef = useRef<HTMLDivElement | null>(null);
	const [phase, setPhase] = useState<SaveBarPhase>("hidden");
	const [measuredReservedHeight, setMeasuredReservedHeight] = useState(0);

	useEffect(() => {
		phaseRef.current = phase;
	}, [phase]);

	useEffect(() => {
		const clearTimer = () => {
			if (timerRef.current !== null) {
				window.clearTimeout(timerRef.current);
				timerRef.current = null;
			}
		};
		const setPhaseState = (nextPhase: SaveBarPhase) => {
			phaseRef.current = nextPhase;
			setPhase(nextPhase);
		};
		const scheduleHidden = () => {
			timerRef.current = window.setTimeout(() => {
				setPhaseState("hidden");
				timerRef.current = null;
			}, exitDurationMs + EXIT_UNMOUNT_GRACE_MS);
		};
		const scheduleExit = (delayMs = 0) => {
			if (delayMs === 0) {
				setPhaseState("exiting");
				scheduleHidden();
				return;
			}

			timerRef.current = window.setTimeout(() => {
				timerRef.current = null;
				setPhaseState("exiting");
				scheduleHidden();
			}, delayMs);
		};

		clearTimer();

		if (hasUnsavedChanges) {
			if (phaseRef.current === "visible") return;

			if (phaseRef.current !== "entering") {
				setPhaseState("entering");
			}

			timerRef.current = window.setTimeout(() => {
				setPhaseState("visible");
				timerRef.current = null;
			}, 0);
			return;
		}

		if (phaseRef.current === "hidden" || phaseRef.current === "exiting") {
			return;
		}

		if (phaseRef.current === "entering") {
			timerRef.current = window.setTimeout(() => {
				timerRef.current = null;
				setPhaseState("visible");
				scheduleExit(NEXT_FRAME_DELAY_MS);
			}, NEXT_FRAME_DELAY_MS);
			return;
		}

		scheduleExit();

		return () => {
			clearTimer();
		};
	}, [exitDurationMs, hasUnsavedChanges]);

	useEffect(() => {
		const timerState = timerRef;
		return () => {
			if (timerState.current !== null) {
				window.clearTimeout(timerState.current);
			}
		};
	}, []);

	useEffect(() => {
		if (phase === "hidden") {
			setMeasuredReservedHeight(0);
			return;
		}

		const fallbackHeight =
			viewportWidth < mobileBreakpoint
				? mobileMinReservedHeight
				: desktopMinReservedHeight;
		const node = measureRef.current;
		if (!node) {
			setMeasuredReservedHeight(fallbackHeight);
			return;
		}

		const updateReservedHeight = () => {
			const measuredHeight = Math.ceil(node.getBoundingClientRect().height);
			setMeasuredReservedHeight(Math.max(measuredHeight, fallbackHeight));
		};

		updateReservedHeight();

		if (typeof ResizeObserver === "undefined") {
			return;
		}

		const resizeObserver = new ResizeObserver(() => {
			updateReservedHeight();
		});
		resizeObserver.observe(node);

		return () => {
			resizeObserver.disconnect();
		};
	}, [
		desktopMinReservedHeight,
		mobileBreakpoint,
		mobileMinReservedHeight,
		phase,
		viewportWidth,
	]);

	return {
		measureRef,
		phase,
		reservedHeight:
			phase === "hidden" || phase === "exiting" ? 0 : measuredReservedHeight,
		transitionDurationMs:
			phase === "exiting" ? exitDurationMs : enterDurationMs,
	};
}
