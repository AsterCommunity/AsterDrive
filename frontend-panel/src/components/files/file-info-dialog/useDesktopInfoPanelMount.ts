import { useEffect, useState } from "react";

const DESKTOP_PANEL_EXIT_MS = 220;

export function useDesktopInfoPanelMount(open: boolean, isDesktop: boolean) {
	const [desktopMounted, setDesktopMounted] = useState(open);
	const [desktopVisible, setDesktopVisible] = useState(open);

	useEffect(() => {
		if (!isDesktop) {
			setDesktopMounted(open);
			setDesktopVisible(open);
			return;
		}

		let enterTimeout: number | null = null;
		let exitTimeout: number | null = null;

		if (open) {
			setDesktopMounted(true);
			enterTimeout = window.setTimeout(() => {
				setDesktopVisible(true);
			}, 0);
		} else {
			setDesktopVisible(false);
			exitTimeout = window.setTimeout(() => {
				setDesktopMounted(false);
			}, DESKTOP_PANEL_EXIT_MS);
		}

		return () => {
			if (enterTimeout != null) {
				window.clearTimeout(enterTimeout);
			}
			if (exitTimeout != null) {
				window.clearTimeout(exitTimeout);
			}
		};
	}, [isDesktop, open]);

	return {
		desktopMounted,
		desktopVisible,
	};
}
