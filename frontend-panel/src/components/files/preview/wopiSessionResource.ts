import type { WopiLaunchSession } from "@/types/api";

export interface WopiSessionSnapshot {
	loading: boolean;
	session: WopiLaunchSession | null;
}

export interface WopiSessionResource {
	getSnapshot: () => WopiSessionSnapshot;
	subscribe: (listener: () => void) => () => void;
}

const initialWopiSessionSnapshot: WopiSessionSnapshot = {
	loading: true,
	session: null,
};

function isValidActionUrl(value: string) {
	try {
		const parsed = new URL(value);
		return parsed.protocol === "http:" || parsed.protocol === "https:";
	} catch {
		return false;
	}
}

export function createWopiSessionResource(
	createSession: () => Promise<WopiLaunchSession>,
): WopiSessionResource {
	const listeners = new Set<() => void>();
	let snapshot = initialWopiSessionSnapshot;
	let started = false;

	function emit(nextSnapshot: WopiSessionSnapshot) {
		snapshot = nextSnapshot;
		for (const listener of listeners) {
			listener();
		}
	}

	function start() {
		if (started) return;
		started = true;

		void createSession()
			.then((nextSession) => {
				if (
					!nextSession.action_url ||
					!isValidActionUrl(nextSession.action_url)
				) {
					emit({ loading: false, session: null });
					return;
				}

				emit({ loading: false, session: nextSession });
			})
			.catch(() => emit({ loading: false, session: null }));
	}

	return {
		getSnapshot: () => snapshot,
		subscribe: (listener) => {
			listeners.add(listener);
			start();

			return () => {
				listeners.delete(listener);
			};
		},
	};
}
