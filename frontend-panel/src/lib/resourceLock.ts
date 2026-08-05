import type { ResourceLockState } from "@/types/api";

export function isResourceLocked(lockState: ResourceLockState): boolean {
	return lockState.state !== "unlocked";
}

export function isDirectResourceLock(lockState: ResourceLockState): boolean {
	return lockState.state === "direct";
}
