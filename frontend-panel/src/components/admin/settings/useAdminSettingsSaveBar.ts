import { useEffect, useRef, useState } from "react";

type SaveBarPhase = "hidden" | "entering" | "visible" | "exiting";

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
	const [reservedHeight, setReservedHeight] = useState(0);

	useEffect(() => {
		phaseRef.current = phase;
	}, [phase]);

	useEffect(() => {
		if (timerRef.current !== null) {
			window.clearTimeout(timerRef.current);
			timerRef.current = null;
		}

		if (hasUnsavedChanges) {
			setPhase("entering");
			timerRef.current = window.setTimeout(() => {
				setPhase("visible");
				timerRef.current = null;
			}, enterDurationMs);
			return;
		}

		if (phaseRef.current === "hidden") {
			return;
		}

		setPhase((previous) => (previous === "hidden" ? previous : "exiting"));
		timerRef.current = window.setTimeout(() => {
			setPhase("hidden");
			timerRef.current = null;
		}, exitDurationMs);

		return () => {
			if (timerRef.current !== null) {
				window.clearTimeout(timerRef.current);
				timerRef.current = null;
			}
		};
	}, [enterDurationMs, exitDurationMs, hasUnsavedChanges]);

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
			setReservedHeight(0);
			return;
		}

		const fallbackHeight =
			viewportWidth < mobileBreakpoint
				? mobileMinReservedHeight
				: desktopMinReservedHeight;
		const node = measureRef.current;
		if (!node) {
			setReservedHeight(fallbackHeight);
			return;
		}

		const updateReservedHeight = () => {
			const measuredHeight = Math.ceil(node.getBoundingClientRect().height);
			setReservedHeight(Math.max(measuredHeight, fallbackHeight));
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
		reservedHeight,
	};
}
