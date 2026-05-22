import { useEffect, useReducer } from "react";

const DESKTOP_PANEL_EXIT_MS = 220;

interface DesktopInfoPanelMountState {
	desktopMounted: boolean;
	desktopVisible: boolean;
}

type DesktopInfoPanelMountAction =
	| { type: "sync"; open: boolean }
	| { type: "mount" }
	| { type: "show" }
	| { type: "hide" }
	| { type: "unmount" };

function desktopInfoPanelMountReducer(
	state: DesktopInfoPanelMountState,
	action: DesktopInfoPanelMountAction,
): DesktopInfoPanelMountState {
	switch (action.type) {
		case "sync":
			return {
				desktopMounted: action.open,
				desktopVisible: action.open,
			};
		case "mount":
			return state.desktopMounted ? state : { ...state, desktopMounted: true };
		case "show":
			return state.desktopVisible ? state : { ...state, desktopVisible: true };
		case "hide":
			return state.desktopVisible ? { ...state, desktopVisible: false } : state;
		case "unmount":
			return state.desktopMounted ? { ...state, desktopMounted: false } : state;
	}
}

export function useDesktopInfoPanelMount(open: boolean, isDesktop: boolean) {
	const [state, dispatch] = useReducer(desktopInfoPanelMountReducer, {
		desktopMounted: open,
		desktopVisible: open,
	});

	useEffect(() => {
		if (!isDesktop) {
			dispatch({ type: "sync", open });
			return;
		}

		let enterTimeout: number | null = null;
		let exitTimeout: number | null = null;

		if (open) {
			dispatch({ type: "mount" });
			enterTimeout = window.setTimeout(() => {
				dispatch({ type: "show" });
			}, 0);
		} else {
			dispatch({ type: "hide" });
			exitTimeout = window.setTimeout(() => {
				dispatch({ type: "unmount" });
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

	return state;
}
