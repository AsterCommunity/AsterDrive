import { useSyncExternalStore } from "react";

const listeners = new Set<() => void>();
let mountRequested = false;

export function requestMusicPlayerHostMount() {
	if (mountRequested) return;
	mountRequested = true;
	for (const listener of listeners) {
		listener();
	}
}

function subscribeMusicPlayerHostMount(listener: () => void) {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

function getMusicPlayerHostMountSnapshot() {
	return mountRequested;
}

export function useMusicPlayerHostMountRequested() {
	return useSyncExternalStore(
		subscribeMusicPlayerHostMount,
		getMusicPlayerHostMountSnapshot,
		getMusicPlayerHostMountSnapshot,
	);
}
