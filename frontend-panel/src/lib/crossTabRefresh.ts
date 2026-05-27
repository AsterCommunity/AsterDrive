const REFRESH_LOCK_KEY = "aster-auth-refresh-lock";
const REFRESH_EVENT_KEY = "aster-auth-refresh-event";
const REFRESH_CHANNEL_NAME = "aster-auth-refresh";
// Keep this below the backend same-client stale-refresh grace window. If a tab
// dies after the server rotates the refresh JTI but before it broadcasts the
// result, a waiter may take over with the old cookie when the lease expires.
const REFRESH_LOCK_TTL_MS = 10_000;
const REFRESH_LOCK_HEARTBEAT_MS = 1_000;
const REFRESH_LOCK_STALE_MS = 3_000;
const REFRESH_WAIT_TIMEOUT_MS = 45_000;

type RefreshFailureKind = "auth" | "transient";

type RefreshLock = {
	ownerId: string;
	lockId: string;
	expiresAt: number;
	updatedAt?: number;
};

type RefreshEvent = {
	ownerId: string;
	lockId: string;
	status: "success" | "failure";
	failureKind?: RefreshFailureKind;
	fallback?: true;
	createdAt: number;
};

type PeerWaitResult =
	| RefreshEvent["status"]
	| "auth_failure"
	| "expired"
	| "stale"
	| "timeout";

type RunWithCrossTabRefreshLockOptions = {
	classifyError?: (error: unknown) => RefreshFailureKind;
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

class PeerRefreshAuthFailedError extends Error {
	readonly crossTabRefreshAuthFailure = true;

	constructor() {
		super("peer auth refresh failed");
		this.name = "PeerRefreshAuthFailedError";
	}
}

export function isCrossTabRefreshAuthFailure(error: unknown): boolean {
	return (
		error instanceof PeerRefreshAuthFailedError ||
		(typeof error === "object" &&
			error !== null &&
			"crossTabRefreshAuthFailure" in error &&
			error.crossTabRefreshAuthFailure === true)
	);
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
		Number.isFinite(record.expiresAt) &&
		(record.updatedAt === undefined ||
			(typeof record.updatedAt === "number" &&
				Number.isFinite(record.updatedAt)))
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
		(record.failureKind === undefined ||
			record.failureKind === "auth" ||
			record.failureKind === "transient") &&
		(record.fallback === undefined || record.fallback === true) &&
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

function lockUpdatedAt(lock: RefreshLock): number | undefined {
	return lock.updatedAt;
}

function lockIsLive(lock: RefreshLock | null, now = Date.now()) {
	if (lock === null || !lockIsActive(lock, now)) return false;
	const updatedAt = lockUpdatedAt(lock);
	return updatedAt === undefined || now - updatedAt < REFRESH_LOCK_STALE_MS;
}

function writeLock(lock: RefreshLock) {
	localStorage.setItem(REFRESH_LOCK_KEY, JSON.stringify(lock));
}

function tryAcquireLock(): RefreshLock | null {
	const now = Date.now();
	const currentLock = readLock();
	if (lockIsLive(currentLock, now) && currentLock?.ownerId !== currentTabId) {
		return null;
	}

	const nextLock: RefreshLock = {
		ownerId: currentTabId,
		lockId: lockId(),
		expiresAt: now + REFRESH_LOCK_TTL_MS,
		updatedAt: now,
	};
	writeLock(nextLock);

	const storedLock = readLock();
	if (
		storedLock?.ownerId === currentTabId &&
		storedLock.lockId === nextLock.lockId
	) {
		return storedLock;
	}
	if (storedLock === null) {
		localStorage.removeItem(REFRESH_LOCK_KEY);
	}
	return null;
}

function releaseLock(acquiredLock: RefreshLock) {
	const lock = readLock();
	if (lock?.ownerId === currentTabId && lock.lockId === acquiredLock.lockId) {
		localStorage.removeItem(REFRESH_LOCK_KEY);
	}
}

function refreshLockLease(acquiredLock: RefreshLock): RefreshLock | null {
	const lock = readLock();
	if (lock?.ownerId !== currentTabId || lock.lockId !== acquiredLock.lockId) {
		return null;
	}

	const renewedLock = {
		...lock,
		expiresAt: Date.now() + REFRESH_LOCK_TTL_MS,
		updatedAt: Date.now(),
	};
	writeLock(renewedLock);
	return renewedLock;
}

function openRefreshChannel(): BroadcastChannel | null {
	if (
		typeof BroadcastChannel === "undefined" ||
		typeof window === "undefined"
	) {
		return null;
	}

	return new BroadcastChannel(REFRESH_CHANNEL_NAME);
}

function broadcastRefreshEvent(event: RefreshEvent) {
	localStorage.setItem(REFRESH_EVENT_KEY, JSON.stringify(event));
	const channel = openRefreshChannel();
	try {
		channel?.postMessage(event);
	} finally {
		channel?.close();
	}
}

function writeRefreshEvent(
	status: RefreshEvent["status"],
	acquiredLock: RefreshLock,
	failureKind?: RefreshFailureKind,
) {
	const currentLock = readLock();
	if (
		!currentLock ||
		currentLock.ownerId !== currentTabId ||
		currentLock.lockId !== acquiredLock.lockId
	) {
		return;
	}

	broadcastRefreshEvent({
		ownerId: currentTabId,
		lockId: currentLock.lockId,
		status,
		...(failureKind ? { failureKind } : {}),
		createdAt: Date.now(),
	});
}

function eventMatchesPeerLock(event: RefreshEvent, peerLock: RefreshLock) {
	return (
		event.ownerId === peerLock.ownerId &&
		event.ownerId !== currentTabId &&
		event.lockId === peerLock.lockId &&
		Date.now() - event.createdAt <= REFRESH_WAIT_TIMEOUT_MS
	);
}

function eventMatchesFallbackRefresh(
	event: RefreshEvent,
	waitStartedAt: number,
) {
	return (
		event.fallback === true &&
		event.ownerId !== currentTabId &&
		event.createdAt >= waitStartedAt &&
		Date.now() - event.createdAt <= REFRESH_WAIT_TIMEOUT_MS &&
		!lockIsActive(readLock())
	);
}

function getStoredEventForLock(
	peerLock: RefreshLock,
	waitStartedAt: number,
): RefreshEvent | null {
	const event = parseJson<RefreshEvent>(
		localStorage.getItem(REFRESH_EVENT_KEY),
	);
	return isRefreshEvent(event) &&
		(eventMatchesPeerLock(event, peerLock) ||
			eventMatchesFallbackRefresh(event, waitStartedAt))
		? event
		: null;
}

function resultFromRefreshEvent(event: RefreshEvent): PeerWaitResult {
	if (event.status === "failure" && event.failureKind === "auth") {
		return "auth_failure";
	}
	return event.status;
}

function waitForPeerRefresh(
	peerLock: RefreshLock,
	deadline: number,
	waitStartedAt: number,
) {
	return new Promise<PeerWaitResult>((resolve) => {
		let settled = false;
		let expiryTimeout: ReturnType<typeof setTimeout> | null = null;
		let staleTimeout: ReturnType<typeof setTimeout> | null = null;
		let timeout: ReturnType<typeof setTimeout> | null = null;
		const channel = openRefreshChannel();

		const cleanup = () => {
			window.removeEventListener("storage", onStorage);
			if (channel) {
				channel.removeEventListener("message", onChannelMessage);
				channel.close();
			}
			if (expiryTimeout !== null) clearTimeout(expiryTimeout);
			if (staleTimeout !== null) clearTimeout(staleTimeout);
			if (timeout !== null) clearTimeout(timeout);
		};

		const finish = (status: PeerWaitResult) => {
			if (settled) return;
			settled = true;
			cleanup();
			resolve(status);
		};

		const handleEvent = (event: RefreshEvent | null) => {
			if (
				!event ||
				(!eventMatchesPeerLock(event, peerLock) &&
					!eventMatchesFallbackRefresh(event, waitStartedAt))
			) {
				return;
			}
			finish(resultFromRefreshEvent(event));
		};

		const scheduleExpiry = (expiresAt: number) => {
			if (expiryTimeout !== null) clearTimeout(expiryTimeout);
			expiryTimeout = setTimeout(
				() => {
					const latestEvent = getStoredEventForLock(peerLock, waitStartedAt);
					if (latestEvent) {
						finish(resultFromRefreshEvent(latestEvent));
						return;
					}
					finish("expired");
				},
				Math.max(0, expiresAt - Date.now()),
			);
		};

		const scheduleStale = (updatedAt: number | undefined) => {
			if (staleTimeout !== null) clearTimeout(staleTimeout);
			if (updatedAt === undefined) return;
			staleTimeout = setTimeout(
				() => {
					const latestEvent = getStoredEventForLock(peerLock, waitStartedAt);
					if (latestEvent) {
						finish(resultFromRefreshEvent(latestEvent));
						return;
					}
					finish("stale");
				},
				Math.max(0, updatedAt + REFRESH_LOCK_STALE_MS - Date.now()),
			);
		};

		function onStorage(event: StorageEvent) {
			if (event.key === REFRESH_EVENT_KEY) {
				const refreshEvent = parseJson<RefreshEvent>(event.newValue);
				handleEvent(isRefreshEvent(refreshEvent) ? refreshEvent : null);
				return;
			}
			if (event.key !== REFRESH_LOCK_KEY) {
				return;
			}

			if (event.newValue === null) {
				handleEvent(getStoredEventForLock(peerLock, waitStartedAt));
				finish("expired");
				return;
			}

			const updatedLock = parseJson<unknown>(event.newValue);
			if (
				isRefreshLock(updatedLock) &&
				updatedLock.ownerId === peerLock.ownerId &&
				updatedLock.lockId === peerLock.lockId &&
				lockIsActive(updatedLock)
			) {
				scheduleExpiry(updatedLock.expiresAt);
				scheduleStale(lockUpdatedAt(updatedLock));
			}
		}

		function onChannelMessage(event: MessageEvent) {
			handleEvent(isRefreshEvent(event.data) ? event.data : null);
		}

		const latestEvent = getStoredEventForLock(peerLock, waitStartedAt);
		if (latestEvent) {
			finish(resultFromRefreshEvent(latestEvent));
			return;
		}

		window.addEventListener("storage", onStorage);
		channel?.addEventListener("message", onChannelMessage);
		scheduleExpiry(peerLock.expiresAt);
		scheduleStale(lockUpdatedAt(peerLock));
		timeout = setTimeout(
			() => {
				finish("timeout");
			},
			Math.max(0, deadline - Date.now()),
		);
	});
}

async function refreshWithLock(
	initialLock: RefreshLock,
	refresh: () => Promise<void>,
	options: RunWithCrossTabRefreshLockOptions,
) {
	let currentLock = initialLock;
	const renewalTimer = setInterval(() => {
		const renewedLock = refreshLockLease(currentLock);
		if (renewedLock !== null) {
			currentLock = renewedLock;
		}
	}, REFRESH_LOCK_HEARTBEAT_MS);

	try {
		await refresh();
		writeRefreshEvent("success", currentLock);
		return true;
	} catch (error) {
		writeRefreshEvent(
			"failure",
			currentLock,
			options.classifyError?.(error) ?? "transient",
		);
		throw error;
	} finally {
		clearInterval(renewalTimer);
		releaseLock(currentLock);
	}
}

async function refreshWithoutConfirmedLock(
	refresh: () => Promise<void>,
	options: RunWithCrossTabRefreshLockOptions,
) {
	const directLockId = lockId();
	try {
		await refresh();
		broadcastRefreshEvent({
			ownerId: currentTabId,
			lockId: directLockId,
			status: "success",
			fallback: true,
			createdAt: Date.now(),
		});
		return true;
	} catch (error) {
		broadcastRefreshEvent({
			ownerId: currentTabId,
			lockId: directLockId,
			status: "failure",
			failureKind: options.classifyError?.(error) ?? "transient",
			fallback: true,
			createdAt: Date.now(),
		});
		throw error;
	}
}

export async function runWithCrossTabRefreshLock(
	refresh: () => Promise<void>,
	options: RunWithCrossTabRefreshLockOptions = {},
): Promise<boolean> {
	if (typeof window === "undefined") {
		await refresh();
		return true;
	}

	const deadline = Date.now() + REFRESH_WAIT_TIMEOUT_MS;
	while (Date.now() <= deadline) {
		const waitStartedAt = Date.now();
		const lock = tryAcquireLock();
		if (lock !== null) {
			return refreshWithLock(lock, refresh, options);
		}

		const peerLock = readLock();
		if (peerLock === null || !lockIsLive(peerLock)) {
			const recoveredLock = tryAcquireLock();
			if (recoveredLock !== null) {
				return refreshWithLock(recoveredLock, refresh, options);
			}
			if (!lockIsLive(readLock())) {
				return refreshWithoutConfirmedLock(refresh, options);
			}
			await new Promise((resolve) => setTimeout(resolve, 25));
			continue;
		}

		const peerResult = await waitForPeerRefresh(
			peerLock,
			deadline,
			waitStartedAt,
		);
		if (peerResult === "success") {
			return false;
		}
		if (peerResult === "failure") {
			throw new PeerRefreshFailedError();
		}
		if (peerResult === "auth_failure") {
			throw new PeerRefreshAuthFailedError();
		}
		if (peerResult === "timeout") {
			throw new PeerRefreshTimedOutError();
		}
	}

	throw new PeerRefreshTimedOutError();
}
