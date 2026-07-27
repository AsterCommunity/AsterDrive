import { create } from "zustand";
import { authService } from "@/services/authService";
import type { SystemSetupState } from "@/types/api";

interface SystemSetupStoreState {
	error: unknown;
	invalidate: () => void;
	isChecking: boolean;
	setupState: SystemSetupState | null;
	refresh: () => Promise<SystemSetupState>;
	setSetupState: (setupState: SystemSetupState) => void;
}

let inFlightRefresh: Promise<SystemSetupState> | null = null;
let setupStateSerial = 0;

export const useSystemSetupStore = create<SystemSetupStoreState>((set) => ({
	error: null,
	invalidate: () => {
		setupStateSerial += 1;
		inFlightRefresh = null;
		set({ error: null, isChecking: false, setupState: null });
	},
	isChecking: false,
	setupState: null,
	refresh: () => {
		if (inFlightRefresh) return inFlightRefresh;

		const requestSerial = setupStateSerial;
		set({ error: null, isChecking: true });
		const request = authService
			.check()
			.then((result) => {
				if (requestSerial === setupStateSerial) {
					set({
						error: null,
						isChecking: false,
						setupState: result.setup_state,
					});
				}
				return result.setup_state;
			})
			.catch((error: unknown) => {
				if (requestSerial === setupStateSerial) {
					set({ error, isChecking: false });
				}
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
	setSetupState: (setupState) => {
		setupStateSerial += 1;
		inFlightRefresh = null;
		set({ error: null, isChecking: false, setupState });
	},
}));
