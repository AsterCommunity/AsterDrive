import type { StorageChangeEventPayload } from "@/lib/storageEventEcho";

type StorageChangeListener = (event: StorageChangeEventPayload) => void;

const listeners = new Set<StorageChangeListener>();

export function subscribeStorageChange(listener: StorageChangeListener) {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

export function publishStorageChange(event: StorageChangeEventPayload) {
	for (const listener of listeners) {
		listener(event);
	}
}
