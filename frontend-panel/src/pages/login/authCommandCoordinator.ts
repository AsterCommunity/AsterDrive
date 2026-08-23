import {
	type AuthUiFlow,
	type AuthUiFlowEvent,
	authUiFlowReducer,
} from "./loginPageState";

export interface AuthCommandCoordinator {
	dispatch(
		event: AuthUiFlowEvent,
		serial?: number,
		options?: AuthCommandDispatchOptions,
	): AuthUiFlow;
	begin(): number;
	isCurrent(serial: number): boolean;
	state(): AuthUiFlow;
}

export interface AuthCommandDispatchOptions {
	/** Allows a finally/cleanup event to land after its result generation is stale. */
	allowStale?: boolean;
}

export function createAuthCommandCoordinator(
	initial: AuthUiFlow,
): AuthCommandCoordinator {
	let current = initial;
	let serial = 0;
	return {
		dispatch(event, candidate, options) {
			if (
				candidate !== undefined &&
				candidate !== serial &&
				!options?.allowStale
			) {
				return current;
			}
			current = authUiFlowReducer(current, event);
			return current;
		},
		/** Starts a global generation; every older guarded result becomes stale. */
		begin() {
			serial += 1;
			return serial;
		},
		isCurrent(candidate) {
			return candidate === serial;
		},
		state() {
			return current;
		},
	};
}
