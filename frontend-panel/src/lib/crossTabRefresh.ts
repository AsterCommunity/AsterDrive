const REFRESH_LOCK_KEY = "aster-auth-refresh-lock";
const REFRESH_EVENT_KEY = "aster-auth-refresh-event";
const REFRESH_LOCK_TTL_MS = 15_000;
const REFRESH_WAIT_TIMEOUT_MS = 20_000;

type RefreshLock = {
	ownerId: string;
	lockId: string;
	expiresAt: number;
};

type RefreshEvent = {
	ownerId: string;
	lockId: string;
	status: "success" | "failure";
	createdAt: number;
};

class PeerRefreshFailedError extends Error {
	constructor() {
		super("peer auth refresh failed");
		this.name = "PeerRefreshFailedError";
	}
}

class PeerRefreshTimedOutError extends Error {
	constructor() {
		super("peer auth refresh timed out");
		this.name = "PeerRefreshTimedOutError";
	}
}

function isRefreshLock(value: unknown): value is RefreshLock {
	if (typeof value !== "object" || value === null) return false;

	const record = value as Record<string, unknown>;
	return (
		typeof record.ownerId === "string" &&
		record.ownerId.length > 0 &&
		typeof record.lockId === "string" &&
		record.lockId.length > 0 &&
		typeof record.expiresAt === "number" &&
		Number.isFinite(record.expiresAt)
	);
}

function isRefreshEvent(value: unknown): value is RefreshEvent {
	if (typeof value !== "object" || value === null) return false;

	const record = value as Record<string, unknown>;
	return (
		typeof record.ownerId === "string" &&
		record.ownerId.length > 0 &&
		typeof record.lockId === "string" &&
		record.lockId.length > 0 &&
		(record.status === "success" || record.status === "failure") &&
		typeof record.createdAt === "number" &&
		Number.isFinite(record.createdAt)
	);
}

function tabId() {
	return (
		globalThis.crypto?.randomUUID?.() ??
		`tab-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`
	);
}

function lockId() {
	return (
		globalThis.crypto?.randomUUID?.() ??
		`lock-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`
	);
}

const currentTabId = tabId();

function parseJson<T>(value: string | null): T | null {
	if (!value) return null;
	try {
		return JSON.parse(value) as T;
	} catch {
		return null;
	}
}

function readLock(): RefreshLock | null {
	const lock = parseJson<unknown>(localStorage.getItem(REFRESH_LOCK_KEY));
	return isRefreshLock(lock) ? lock : null;
}

function lockIsActive(lock: RefreshLock | null, now = Date.now()) {
	return lock !== null && lock.expiresAt > now;
}

function writeRefreshEvent(status: RefreshEvent["status"]) {
	const currentLock = readLock();
	if (!currentLock || currentLock.ownerId !== currentTabId) {
		return;
	}

	const event: RefreshEvent = {
		ownerId: currentTabId,
		lockId: currentLock.lockId,
		status,
		createdAt: Date.now(),
	};
	localStorage.setItem(REFRESH_EVENT_KEY, JSON.stringify(event));
}

function releaseLock() {
	const lock = readLock();
	if (lock?.ownerId === currentTabId) {
		localStorage.removeItem(REFRESH_LOCK_KEY);
	}
}

function tryAcquireLock() {
	const now = Date.now();
	const currentLock = readLock();
	if (lockIsActive(currentLock, now) && currentLock?.ownerId !== currentTabId) {
		return null;
	}

	const nextLock: RefreshLock = {
		ownerId: currentTabId,
		lockId: lockId(),
		expiresAt: now + REFRESH_LOCK_TTL_MS,
	};
	localStorage.setItem(REFRESH_LOCK_KEY, JSON.stringify(nextLock));

	const storedLock = readLock();
	return storedLock?.ownerId === currentTabId ? storedLock : null;
}

function waitForPeerRefresh(peerLock: RefreshLock) {
	return new Promise<RefreshEvent["status"] | "timeout">((resolve) => {
		let settled = false;
		let timeout: ReturnType<typeof setTimeout> | null = null;

		const cleanup = () => {
			window.removeEventListener("storage", onStorage);
			if (timeout !== null) clearTimeout(timeout);
		};

		const finish = (status: RefreshEvent["status"] | "timeout") => {
			if (settled) return;
			settled = true;
			cleanup();
			resolve(status);
		};

		const handleEvent = (event: RefreshEvent | null) => {
			if (!isRefreshEvent(event)) {
				return;
			}
			if (event.ownerId === currentTabId || event.lockId !== peerLock.lockId) {
				return;
			}
			if (Date.now() - event.createdAt > REFRESH_WAIT_TIMEOUT_MS) return;
			finish(event.status);
		};

		function onStorage(event: StorageEvent) {
			if (event.key === REFRESH_EVENT_KEY) {
				handleEvent(parseJson<RefreshEvent>(event.newValue));
				return;
			}
			if (event.key === REFRESH_LOCK_KEY && event.newValue === null) {
				const lastEvent = parseJson<RefreshEvent>(
					localStorage.getItem(REFRESH_EVENT_KEY),
				);
				handleEvent(lastEvent);
			}
		}

		window.addEventListener("storage", onStorage);
		timeout = setTimeout(() => {
			finish("timeout");
		}, REFRESH_WAIT_TIMEOUT_MS);
		handleEvent(
			parseJson<RefreshEvent>(localStorage.getItem(REFRESH_EVENT_KEY)),
		);
	});
}

export async function runWithCrossTabRefreshLock(
	refresh: () => Promise<void>,
): Promise<boolean> {
	if (typeof window === "undefined") {
		await refresh();
		return true;
	}

	const lock = tryAcquireLock();
	if (lock !== null) {
		try {
			await refresh();
			writeRefreshEvent("success");
			return true;
		} catch (error) {
			writeRefreshEvent("failure");
			throw error;
		} finally {
			releaseLock();
		}
	}

	const peerLock = readLock();
	if (peerLock === null || !lockIsActive(peerLock)) {
		await refresh();
		return true;
	}

	const peerResult = await waitForPeerRefresh(peerLock);
	if (peerResult === "failure") {
		throw new PeerRefreshFailedError();
	}
	if (peerResult === "timeout") {
		throw new PeerRefreshTimedOutError();
	}
	return false;
}
