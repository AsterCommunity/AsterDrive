import { useLayoutEffect, useRef, useState } from "react";

/** Presence owns the DOM until its actual transitions finish, including reduced motion. */
export function useDesktopInfoPanelMount(
	open: boolean,
	isDesktop: boolean,
	onExitComplete?: (open: boolean) => void,
) {
	const panelRef = useRef<HTMLDivElement>(null);
	const [desktopMounted, setMounted] = useState(open);
	const [desktopVisible, setVisible] = useState(open);
	useLayoutEffect(() => {
		if (!isDesktop) {
			setMounted(open);
			setVisible(open);
			return;
		}
		if (open) setMounted(true);
		else setVisible(false);
	}, [open, isDesktop]);
	useLayoutEffect(() => {
		if (!isDesktop || !desktopMounted) return;
		const node = panelRef.current;
		if (!node) return;
		// Commit the collapsed geometry before showing; this also works after a fast reopen.
		if (open) {
			void node.offsetWidth;
			setVisible(true);
			return;
		}
		// Wait for the closed classes to reach the DOM before collecting transitions.
		if (desktopVisible) return;
		let active = true;
		const animations = node
			.getAnimations({ subtree: true })
			.filter(
				(animation) =>
					animation.effect?.getComputedTiming().iterations !== Infinity,
			);
		void Promise.allSettled(
			animations.map((animation) => animation.finished),
		).then(() => {
			if (!active) return;
			setMounted(false);
			onExitComplete?.(false);
		});
		return () => {
			active = false;
		};
	}, [open, isDesktop, desktopMounted, desktopVisible, onExitComplete]);
	return { desktopMounted, desktopVisible, panelRef };
}
