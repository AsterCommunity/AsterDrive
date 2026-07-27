import { create } from "zustand";
import { authService } from "@/services/authService";
import type { SystemSetupState } from "@/types/api";

interface SystemSetupStoreState {
	error: unknown;
	isChecking: boolean;
	setupState: SystemSetupState | null;
	refresh: () => Promise<SystemSetupState>;
	setSetupState: (setupState: SystemSetupState) => void;
}

let inFlightRefresh: Promise<SystemSetupState> | null = null;

export const useSystemSetupStore = create<SystemSetupStoreState>((set) => ({
	error: null,
	isChecking: false,
	setupState: null,
	refresh: () => {
		if (inFlightRefresh) return inFlightRefresh;

		set({ error: null, isChecking: true });
		const request = authService
			.check()
			.then((result) => {
				set({
					error: null,
					isChecking: false,
					setupState: result.setup_state,
				});
				return result.setup_state;
			})
			.catch((error: unknown) => {
				set({ error, isChecking: false });
				throw error;
			})
			.finally(() => {
				if (inFlightRefresh === request) {
					inFlightRefresh = null;
				}
			});

		inFlightRefresh = request;
		return request;
	},
	setSetupState: (setupState) => set({ error: null, setupState }),
}));
