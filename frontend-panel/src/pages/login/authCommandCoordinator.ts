import {
	type AuthUiFlow,
	type AuthUiFlowEvent,
	authUiFlowReducer,
} from "./loginPageState";

export interface AuthCommandCoordinator {
	dispatch(event: AuthUiFlowEvent, serial?: number): AuthUiFlow;
	begin(): number;
	cancel(): void;
	isCurrent(serial: number): boolean;
	state(): AuthUiFlow;
}

export function createAuthCommandCoordinator(
	initial: AuthUiFlow,
): AuthCommandCoordinator {
	let current = initial;
	let serial = 0;
	return {
		dispatch(event, candidate) {
			if (candidate !== undefined && candidate !== serial) {
				return current;
			}
			current = authUiFlowReducer(current, event);
			return current;
		},
		begin() {
			serial += 1;
			return serial;
		},
		cancel() {
			serial += 1;
		},
		isCurrent(candidate) {
			return candidate === serial;
		},
		state() {
			return current;
		},
	};
}
