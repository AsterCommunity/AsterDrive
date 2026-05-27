import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type BroadcastListener = (event: MessageEvent) => void;

const broadcastChannels = new Set<MockBroadcastChannel>();

class MockBroadcastChannel {
	name: string;
	listeners = new Set<BroadcastListener>();
	closed = false;

	constructor(name: string) {
		this.name = name;
		broadcastChannels.add(this);
	}

	addEventListener(type: string, listener: BroadcastListener) {
		if (type === "message") {
			this.listeners.add(listener);
		}
	}

	removeEventListener(type: string, listener: BroadcastListener) {
		if (type === "message") {
			this.listeners.delete(listener);
		}
	}

	postMessage(data: unknown) {
		for (const channel of broadcastChannels) {
			if (channel === this || channel.closed || channel.name !== this.name) {
				continue;
			}
			for (const listener of channel.listeners) {
				listener(new MessageEvent("message", { data }));
			}
		}
	}

	close() {
		this.closed = true;
		broadcastChannels.delete(this);
	}
}

async function loadModule() {
	vi.resetModules();
	return await import("@/lib/crossTabRefresh");
}

type CrossTabRefreshModule = Awaited<ReturnType<typeof loadModule>>;

async function loadModuleForTab(tabId: string): Promise<CrossTabRefreshModule> {
	vi.resetModules();
	let sequence = 0;
	const randomUUID = vi.fn(() => {
		sequence += 1;
		return `${tabId}-lock-${sequence}`;
	});
	randomUUID.mockReturnValueOnce(tabId);
	vi.stubGlobal("crypto", { randomUUID });
	return await import("@/lib/crossTabRefresh");
}

function setRefreshLock(
	options: {
		expiresAt?: number;
		lockId?: string;
		ownerId?: string;
		updatedAt?: number | undefined;
	} = {},
) {
	const {
		expiresAt = Date.now() + 15_000,
		lockId = "peer-lock",
		ownerId = "peer-tab",
	} = options;
	const hasUpdatedAt = Object.hasOwn(options, "updatedAt");
	const updatedAt = hasUpdatedAt ? options.updatedAt : Date.now();
	const lock = {
		ownerId,
		lockId,
		expiresAt,
		...(updatedAt === undefined ? {} : { updatedAt }),
	};
	localStorage.setItem("aster-auth-refresh-lock", JSON.stringify(lock));
}

function setRefreshEvent({
	createdAt = Date.now(),
	failureKind,
	lockId = "peer-lock",
	ownerId = "peer-tab",
	status = "success",
}: {
	createdAt?: number;
	failureKind?: "auth" | "transient";
	lockId?: string;
	ownerId?: string;
	status?: "success" | "failure";
} = {}) {
	localStorage.setItem(
		"aster-auth-refresh-event",
		JSON.stringify({
			ownerId,
			lockId,
			status,
			...(failureKind ? { failureKind } : {}),
			createdAt,
		}),
	);
}

function dispatchRefreshEvent({
	createdAt = Date.now(),
	failureKind,
	lockId = "peer-lock",
	ownerId = "peer-tab",
	status = "success",
}: {
	createdAt?: number;
	failureKind?: "auth" | "transient";
	lockId?: string;
	ownerId?: string;
	status?: "success" | "failure";
} = {}) {
	window.dispatchEvent(
		new StorageEvent("storage", {
			key: "aster-auth-refresh-event",
			newValue: JSON.stringify({
				ownerId,
				lockId,
				status,
				...(failureKind ? { failureKind } : {}),
				createdAt,
			}),
		}),
	);
}

function readRefreshLock(): {
	expiresAt: number;
	lockId: string;
	ownerId: string;
} {
	return JSON.parse(
		localStorage.getItem("aster-auth-refresh-lock") ?? "{}",
	) as {
		expiresAt: number;
		lockId: string;
		ownerId: string;
	};
}

describe("cross-tab refresh coordination", () => {
	beforeEach(() => {
		localStorage.clear();
		broadcastChannels.clear();
		vi.useRealTimers();
	});

	afterEach(() => {
		vi.unstubAllGlobals();
		vi.restoreAllMocks();
		vi.resetAllMocks();
		vi.useRealTimers();
	});

	it("runs refresh immediately when no peer holds the lock", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);

		await expect(runWithCrossTabRefreshLock(refresh)).resolves.toBe(true);

		expect(refresh).toHaveBeenCalledTimes(1);
		expect(localStorage.getItem("aster-auth-refresh-lock")).toBeNull();
	});

	it("runs refresh directly outside a browser window", async () => {
		vi.stubGlobal("window", undefined);
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);

		await expect(runWithCrossTabRefreshLock(refresh)).resolves.toBe(true);

		expect(refresh).toHaveBeenCalledTimes(1);
	});

	it("runs refresh directly when a competing lock disappears before waiting", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		const storedGetItem = Storage.prototype.getItem;
		const getItemSpy = vi.spyOn(Storage.prototype, "getItem");
		setRefreshLock();
		getItemSpy.mockImplementation(function (
			this: Storage,
			key: string,
		): string | null {
			if (
				key === "aster-auth-refresh-lock" &&
				getItemSpy.mock.calls.length >= 2
			) {
				return null;
			}
			return storedGetItem.call(this, key);
		});

		await expect(runWithCrossTabRefreshLock(refresh)).resolves.toBe(true);

		expect(refresh).toHaveBeenCalledTimes(1);
	});

	it("broadcasts direct fallback refresh results when the lock disappears", async () => {
		vi.stubGlobal("BroadcastChannel", MockBroadcastChannel);
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		const received: unknown[] = [];
		const observer = new MockBroadcastChannel("aster-auth-refresh");
		observer.addEventListener("message", (event) => {
			received.push(event.data);
		});
		const storedGetItem = Storage.prototype.getItem;
		const getItemSpy = vi.spyOn(Storage.prototype, "getItem");
		setRefreshLock();
		getItemSpy.mockImplementation(function (
			this: Storage,
			key: string,
		): string | null {
			if (
				key === "aster-auth-refresh-lock" &&
				getItemSpy.mock.calls.length >= 2
			) {
				return null;
			}
			return storedGetItem.call(this, key);
		});

		await expect(runWithCrossTabRefreshLock(refresh)).resolves.toBe(true);

		expect(refresh).toHaveBeenCalledTimes(1);
		expect(received).toEqual([
			expect.objectContaining({
				fallback: true,
				status: "success",
			}),
		]);
		expect(
			JSON.parse(localStorage.getItem("aster-auth-refresh-event") ?? "{}"),
		).toMatchObject({
			fallback: true,
			status: "success",
		});
		observer.close();
		expect(broadcastChannels.size).toBe(0);
	});

	it("broadcasts direct fallback refresh failures with classified error kind", async () => {
		vi.stubGlobal("BroadcastChannel", MockBroadcastChannel);
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refreshError = new Error("fallback failed");
		const refresh = vi.fn(async () => {
			throw refreshError;
		});
		const received: unknown[] = [];
		const observer = new MockBroadcastChannel("aster-auth-refresh");
		observer.addEventListener("message", (event) => {
			received.push(event.data);
		});
		const storedGetItem = Storage.prototype.getItem;
		const getItemSpy = vi.spyOn(Storage.prototype, "getItem");
		setRefreshLock();
		getItemSpy.mockImplementation(function (
			this: Storage,
			key: string,
		): string | null {
			if (
				key === "aster-auth-refresh-lock" &&
				getItemSpy.mock.calls.length >= 2
			) {
				return null;
			}
			return storedGetItem.call(this, key);
		});

		await expect(
			runWithCrossTabRefreshLock(refresh, {
				classifyError: () => "auth",
			}),
		).rejects.toBe(refreshError);

		expect(refresh).toHaveBeenCalledTimes(1);
		expect(received).toEqual([
			expect.objectContaining({
				failureKind: "auth",
				fallback: true,
				status: "failure",
			}),
		]);
		expect(
			JSON.parse(localStorage.getItem("aster-auth-refresh-event") ?? "{}"),
		).toMatchObject({
			failureKind: "auth",
			fallback: true,
			status: "failure",
		});
		observer.close();
		expect(broadcastChannels.size).toBe(0);
	});

	it("recovers the lock when a competing lock disappears during acquisition", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		const storedGetItem = Storage.prototype.getItem;
		const getItemSpy = vi.spyOn(Storage.prototype, "getItem");
		let lockReadCount = 0;
		setRefreshLock();
		getItemSpy.mockImplementation(function (
			this: Storage,
			key: string,
		): string | null {
			if (key !== "aster-auth-refresh-lock") {
				return storedGetItem.call(this, key);
			}
			lockReadCount += 1;
			if (lockReadCount === 2 || lockReadCount === 3) {
				return null;
			}
			return storedGetItem.call(this, key);
		});

		await expect(runWithCrossTabRefreshLock(refresh)).resolves.toBe(true);

		expect(refresh).toHaveBeenCalledTimes(1);
		expect(localStorage.getItem("aster-auth-refresh-lock")).toBeNull();
		expect(localStorage.getItem("aster-auth-refresh-event")).toContain(
			'"status":"success"',
		);
	});

	it("retries after a new active peer lock appears during recovery", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		const storedGetItem = Storage.prototype.getItem;
		const getItemSpy = vi.spyOn(Storage.prototype, "getItem");
		let lockReadCount = 0;
		setRefreshLock({
			lockId: "first-peer-lock",
			ownerId: "first-peer",
		});
		const secondPeerLock = JSON.stringify({
			ownerId: "second-peer",
			lockId: "second-peer-lock",
			expiresAt: Date.now() + 15_000,
			updatedAt: Date.now(),
		});
		getItemSpy.mockImplementation(function (
			this: Storage,
			key: string,
		): string | null {
			if (key !== "aster-auth-refresh-lock") {
				return storedGetItem.call(this, key);
			}
			lockReadCount += 1;
			if (lockReadCount === 2) {
				return null;
			}
			if (lockReadCount >= 3) {
				return secondPeerLock;
			}
			return storedGetItem.call(this, key);
		});

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.advanceTimersByTimeAsync(25);
		dispatchRefreshEvent({
			lockId: "second-peer-lock",
			ownerId: "second-peer",
		});

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
		expect(lockReadCount).toBeGreaterThanOrEqual(5);
	});

	it("lets peer waiters use a direct fallback refresh result after lock removal", async () => {
		const waiter = await loadModuleForTab("waiter-tab");
		const fallbackOwner = await loadModuleForTab("fallback-owner");
		const waiterRefresh = vi.fn(async () => undefined);
		const ownerRefresh = vi.fn(async () => undefined);
		const storedGetItem = Storage.prototype.getItem;
		const getItemSpy = vi.spyOn(Storage.prototype, "getItem");
		let ownerLockReads = 0;
		setRefreshLock({
			lockId: "vanishing-peer-lock",
			ownerId: "vanishing-peer",
		});

		const pendingWaiter = waiter.runWithCrossTabRefreshLock(waiterRefresh);
		getItemSpy.mockImplementation(function (
			this: Storage,
			key: string,
		): string | null {
			if (key !== "aster-auth-refresh-lock") {
				return storedGetItem.call(this, key);
			}
			ownerLockReads += 1;
			if (ownerLockReads >= 2) {
				return null;
			}
			return storedGetItem.call(this, key);
		});
		await expect(
			fallbackOwner.runWithCrossTabRefreshLock(ownerRefresh),
		).resolves.toBe(true);
		getItemSpy.mockRestore();
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "aster-auth-refresh-lock",
				newValue: null,
			}),
		);

		await expect(pendingWaiter).resolves.toBe(false);
		expect(ownerRefresh).toHaveBeenCalledTimes(1);
		expect(waiterRefresh).not.toHaveBeenCalled();
	});

	it("waits for another tab's successful refresh instead of refreshing again", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock();

		const pending = runWithCrossTabRefreshLock(refresh);
		dispatchRefreshEvent();

		await expect(pending).resolves.toBe(false);

		expect(refresh).not.toHaveBeenCalled();
	});

	it("accepts peer refresh results from BroadcastChannel", async () => {
		vi.stubGlobal("BroadcastChannel", MockBroadcastChannel);
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock();

		const pending = runWithCrossTabRefreshLock(refresh);
		const peerChannel = new MockBroadcastChannel("aster-auth-refresh");
		peerChannel.postMessage({
			ownerId: "peer-tab",
			lockId: "peer-lock",
			status: "success",
			createdAt: Date.now(),
		});

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
		expect([...broadcastChannels]).toEqual([peerChannel]);
		peerChannel.close();
		expect(broadcastChannels.size).toBe(0);
	});

	it("resolves every waiting tab from the same peer success event", async () => {
		vi.stubGlobal("BroadcastChannel", MockBroadcastChannel);
		const waiterA = await loadModuleForTab("waiter-a");
		const waiterB = await loadModuleForTab("waiter-b");
		const refreshA = vi.fn(async () => undefined);
		const refreshB = vi.fn(async () => undefined);
		setRefreshLock({ ownerId: "owner-tab", lockId: "owner-lock" });

		const pendingA = waiterA.runWithCrossTabRefreshLock(refreshA);
		const pendingB = waiterB.runWithCrossTabRefreshLock(refreshB);
		const ownerChannel = new MockBroadcastChannel("aster-auth-refresh");
		ownerChannel.postMessage({
			ownerId: "owner-tab",
			lockId: "owner-lock",
			status: "success",
			createdAt: Date.now(),
		});

		await expect(Promise.all([pendingA, pendingB])).resolves.toEqual([
			false,
			false,
		]);
		expect(refreshA).not.toHaveBeenCalled();
		expect(refreshB).not.toHaveBeenCalled();
		expect([...broadcastChannels]).toEqual([ownerChannel]);
		ownerChannel.close();
		expect(broadcastChannels.size).toBe(0);
	});

	it("ignores malformed BroadcastChannel messages and owner mismatches", async () => {
		vi.stubGlobal("BroadcastChannel", MockBroadcastChannel);
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({ lockId: "shared-lock", ownerId: "peer-tab" });

		const pending = runWithCrossTabRefreshLock(refresh);
		const peerChannel = new MockBroadcastChannel("aster-auth-refresh");
		peerChannel.postMessage({
			ownerId: "different-peer",
			lockId: "shared-lock",
			status: "success",
			createdAt: Date.now(),
		});
		peerChannel.postMessage({
			ownerId: "peer-tab",
			lockId: "shared-lock",
			status: "done",
			createdAt: Date.now(),
		});
		await Promise.resolve();
		expect(refresh).not.toHaveBeenCalled();

		peerChannel.postMessage({
			ownerId: "peer-tab",
			lockId: "shared-lock",
			status: "success",
			createdAt: Date.now(),
		});

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("rejects without repeating refresh when the peer reports failure", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock();

		const pending = runWithCrossTabRefreshLock(refresh);
		dispatchRefreshEvent({ status: "failure" });

		await expect(pending).rejects.toThrow("peer auth refresh failed");
		expect(refresh).not.toHaveBeenCalled();
	});

	it("marks peer auth failures so callers can clear the session", async () => {
		const { isCrossTabRefreshAuthFailure, runWithCrossTabRefreshLock } =
			await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock();

		const pending = runWithCrossTabRefreshLock(refresh);
		dispatchRefreshEvent({ failureKind: "auth", status: "failure" });

		await expect(pending).rejects.toSatisfy((error) => {
			return isCrossTabRefreshAuthFailure(error);
		});
		expect(refresh).not.toHaveBeenCalled();
	});

	it("detects cross-tab auth failure marker objects and rejects non-markers", async () => {
		const { isCrossTabRefreshAuthFailure } = await loadModule();

		expect(isCrossTabRefreshAuthFailure(null)).toBe(false);
		expect(isCrossTabRefreshAuthFailure(new Error("plain"))).toBe(false);
		expect(
			isCrossTabRefreshAuthFailure({ crossTabRefreshAuthFailure: false }),
		).toBe(false);
		expect(
			isCrossTabRefreshAuthFailure({ crossTabRefreshAuthFailure: true }),
		).toBe(true);
	});

	it("marks peer auth failures for every waiting tab", async () => {
		vi.stubGlobal("BroadcastChannel", MockBroadcastChannel);
		const waiterA = await loadModuleForTab("waiter-a");
		const waiterB = await loadModuleForTab("waiter-b");
		const refreshA = vi.fn(async () => undefined);
		const refreshB = vi.fn(async () => undefined);
		setRefreshLock({ ownerId: "owner-tab", lockId: "owner-lock" });

		const pendingA = waiterA.runWithCrossTabRefreshLock(refreshA);
		const pendingB = waiterB.runWithCrossTabRefreshLock(refreshB);
		const ownerChannel = new MockBroadcastChannel("aster-auth-refresh");
		ownerChannel.postMessage({
			ownerId: "owner-tab",
			lockId: "owner-lock",
			status: "failure",
			failureKind: "auth",
			createdAt: Date.now(),
		});

		await expect(pendingA).rejects.toSatisfy((error) => {
			return waiterA.isCrossTabRefreshAuthFailure(error);
		});
		await expect(pendingB).rejects.toSatisfy((error) => {
			return waiterB.isCrossTabRefreshAuthFailure(error);
		});
		expect(refreshA).not.toHaveBeenCalled();
		expect(refreshB).not.toHaveBeenCalled();
		expect([...broadcastChannels]).toEqual([ownerChannel]);
		ownerChannel.close();
		expect(broadcastChannels.size).toBe(0);
	});

	it("marks stored peer auth failures before installing listeners", async () => {
		const { isCrossTabRefreshAuthFailure, runWithCrossTabRefreshLock } =
			await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock();
		setRefreshEvent({ failureKind: "auth", status: "failure" });

		await expect(runWithCrossTabRefreshLock(refresh)).rejects.toSatisfy(
			(error) => {
				return isCrossTabRefreshAuthFailure(error);
			},
		);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("settles once when the same peer result arrives through storage and BroadcastChannel", async () => {
		vi.stubGlobal("BroadcastChannel", MockBroadcastChannel);
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock();

		const pending = runWithCrossTabRefreshLock(refresh);
		const peerChannel = new MockBroadcastChannel("aster-auth-refresh");
		const event = {
			ownerId: "peer-tab",
			lockId: "peer-lock",
			status: "success",
			createdAt: Date.now(),
		};
		peerChannel.postMessage(event);
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "aster-auth-refresh-event",
				newValue: JSON.stringify(event),
			}),
		);
		peerChannel.postMessage({
			...event,
			status: "failure",
			failureKind: "auth",
		});

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
		expect([...broadcastChannels]).toEqual([peerChannel]);
		peerChannel.close();
		expect(broadcastChannels.size).toBe(0);
	});

	it("ignores unrelated storage events while waiting for a peer result", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock();

		const pending = runWithCrossTabRefreshLock(refresh);
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "unrelated-key",
				newValue: "{}",
			}),
		);
		await Promise.resolve();

		expect(refresh).not.toHaveBeenCalled();
		dispatchRefreshEvent();

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("uses the last refresh event when the peer lock is removed", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock();

		const pending = runWithCrossTabRefreshLock(refresh);
		setRefreshEvent();
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "aster-auth-refresh-lock",
				newValue: null,
			}),
		);

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("uses an already stored fresh event for the current peer lock", async () => {
		vi.stubGlobal("BroadcastChannel", MockBroadcastChannel);
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock();
		setRefreshEvent();

		await expect(runWithCrossTabRefreshLock(refresh)).resolves.toBe(false);

		expect(refresh).not.toHaveBeenCalled();
		expect(broadcastChannels.size).toBe(0);
	});

	it("ignores stored refresh events older than the wait window", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock();
		setRefreshEvent({ createdAt: Date.now() - 20_001 });

		const pending = runWithCrossTabRefreshLock(refresh);
		dispatchRefreshEvent();

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("ignores refresh events with invalid failure kinds", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock();

		const pending = runWithCrossTabRefreshLock(refresh);
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "aster-auth-refresh-event",
				newValue: JSON.stringify({
					ownerId: "peer-tab",
					lockId: "peer-lock",
					status: "failure",
					failureKind: "expired-token",
					createdAt: Date.now(),
				}),
			}),
		);
		await Promise.resolve();
		expect(refresh).not.toHaveBeenCalled();
		dispatchRefreshEvent({ status: "failure" });

		await expect(pending).rejects.toThrow("peer auth refresh failed");
		expect(refresh).not.toHaveBeenCalled();
	});

	it("times out without repeating refresh when the peer never reports a result", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({ expiresAt: Date.now() + 60_000 });

		const pending = expect(runWithCrossTabRefreshLock(refresh)).rejects.toThrow(
			"peer auth refresh timed out",
		);
		for (let elapsed = 0; elapsed < 45_000; elapsed += 1_000) {
			setRefreshLock({
				expiresAt: Date.now() + 60_000,
				updatedAt: Date.now(),
			});
			window.dispatchEvent(
				new StorageEvent("storage", {
					key: "aster-auth-refresh-lock",
					newValue: localStorage.getItem("aster-auth-refresh-lock"),
				}),
			);
			await vi.advanceTimersByTimeAsync(1_000);
		}

		await pending;
		expect(refresh).not.toHaveBeenCalled();
	});

	it("takes over quickly when the peer lock stops heartbeating", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({ expiresAt: Date.now() + 10_000 });

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.advanceTimersByTimeAsync(3_000);

		await expect(pending).resolves.toBe(true);
		expect(refresh).toHaveBeenCalledTimes(1);
	});

	it("keeps waiting when the peer renews its lease before expiry", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({ expiresAt: Date.now() + 1_000 });

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.advanceTimersByTimeAsync(500);
		setRefreshLock({ expiresAt: Date.now() + 5_000 });
		await vi.advanceTimersByTimeAsync(500);

		expect(refresh).not.toHaveBeenCalled();
		dispatchRefreshEvent();

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("treats active locks without updatedAt as live for old tabs", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({
			expiresAt: Date.now() + 10_000,
			updatedAt: undefined,
		});

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.advanceTimersByTimeAsync(3_000);

		expect(refresh).not.toHaveBeenCalled();
		dispatchRefreshEvent();

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("does not schedule stale takeover for old locks without updatedAt", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({
			expiresAt: Date.now() + 10_000,
			updatedAt: undefined,
		});

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.advanceTimersByTimeAsync(9_999);

		expect(refresh).not.toHaveBeenCalled();
		await vi.advanceTimersByTimeAsync(1);

		await expect(pending).resolves.toBe(true);
		expect(refresh).toHaveBeenCalledTimes(1);
	});

	it("extends the peer expiry timer from storage lock renewal events", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		const renewedLock = {
			ownerId: "peer-tab",
			lockId: "peer-lock",
			expiresAt: Date.now() + 5_000,
			updatedAt: Date.now(),
		};
		setRefreshLock({ expiresAt: Date.now() + 1_000 });

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.advanceTimersByTimeAsync(500);
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "aster-auth-refresh-lock",
				newValue: JSON.stringify(renewedLock),
			}),
		);
		await vi.advanceTimersByTimeAsync(500);

		expect(refresh).not.toHaveBeenCalled();
		dispatchRefreshEvent();

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("ignores malformed peer lock renewal events while waiting", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({ expiresAt: Date.now() + 5_000 });

		const pending = runWithCrossTabRefreshLock(refresh);
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "aster-auth-refresh-lock",
				newValue: JSON.stringify({
					ownerId: "peer-tab",
					lockId: "peer-lock",
					expiresAt: "soon",
					updatedAt: Date.now(),
				}),
			}),
		);
		await vi.advanceTimersByTimeAsync(1_000);

		expect(refresh).not.toHaveBeenCalled();
		dispatchRefreshEvent();

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("waits for a newer peer lock when the original peer lease expires", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({
			expiresAt: Date.now() + 1_000,
			lockId: "first-lock",
			ownerId: "first-peer",
		});

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.advanceTimersByTimeAsync(900);
		setRefreshLock({
			expiresAt: Date.now() + 5_000,
			lockId: "second-lock",
			ownerId: "second-peer",
		});
		await vi.advanceTimersByTimeAsync(100);

		expect(refresh).not.toHaveBeenCalled();
		dispatchRefreshEvent({
			lockId: "second-lock",
			ownerId: "second-peer",
		});

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("ignores a late old owner result after a newer peer lock takes over", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({
			expiresAt: Date.now() + 1_000,
			lockId: "first-lock",
			ownerId: "first-peer",
		});

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.advanceTimersByTimeAsync(900);
		setRefreshLock({
			expiresAt: Date.now() + 5_000,
			lockId: "second-lock",
			ownerId: "second-peer",
		});
		await vi.advanceTimersByTimeAsync(100);
		dispatchRefreshEvent({
			lockId: "first-lock",
			ownerId: "first-peer",
		});
		await Promise.resolve();

		expect(refresh).not.toHaveBeenCalled();
		dispatchRefreshEvent({
			lockId: "second-lock",
			ownerId: "second-peer",
		});

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("falls back to a local refresh when the peer lock expires without a result", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({ expiresAt: Date.now() + 1_000 });

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.advanceTimersByTimeAsync(1_000);

		await expect(pending).resolves.toBe(true);
		expect(refresh).toHaveBeenCalledTimes(1);
		expect(localStorage.getItem("aster-auth-refresh-event")).toContain(
			'"status":"success"',
		);
	});

	it("uses a stored peer result at lock expiry instead of taking over", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({ expiresAt: Date.now() + 1_000 });

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.advanceTimersByTimeAsync(900);
		setRefreshEvent();
		await vi.advanceTimersByTimeAsync(100);

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("throws when the retry deadline is already exceeded", async () => {
		const nowSpy = vi.spyOn(Date, "now");
		nowSpy.mockReturnValueOnce(0);
		nowSpy.mockReturnValue(45_001);
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);

		await expect(runWithCrossTabRefreshLock(refresh)).rejects.toThrow(
			"peer auth refresh timed out",
		);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("lets only one waiting tab take over when a peer owner dies", async () => {
		vi.useFakeTimers();
		vi.stubGlobal("BroadcastChannel", MockBroadcastChannel);
		let resolveA: (() => void) | null = null;
		let resolveB: (() => void) | null = null;
		const waiterA = await loadModuleForTab("waiter-a");
		const waiterB = await loadModuleForTab("waiter-b");
		const refreshA = vi.fn(
			() =>
				new Promise<void>((resolve) => {
					resolveA = resolve;
				}),
		);
		const refreshB = vi.fn(
			() =>
				new Promise<void>((resolve) => {
					resolveB = resolve;
				}),
		);
		setRefreshLock({
			expiresAt: Date.now() + 1_000,
			lockId: "dead-lock",
			ownerId: "dead-owner",
		});

		const pendingA = waiterA.runWithCrossTabRefreshLock(refreshA);
		const pendingB = waiterB.runWithCrossTabRefreshLock(refreshB);
		await vi.advanceTimersByTimeAsync(1_000);

		expect(refreshA.mock.calls.length + refreshB.mock.calls.length).toBe(1);
		const takeoverOwner = readRefreshLock().ownerId;
		expect(["waiter-a", "waiter-b"]).toContain(takeoverOwner);
		expect(refreshA).toHaveBeenCalledTimes(
			takeoverOwner === "waiter-a" ? 1 : 0,
		);
		expect(refreshB).toHaveBeenCalledTimes(
			takeoverOwner === "waiter-b" ? 1 : 0,
		);

		if (takeoverOwner === "waiter-a") {
			resolveA?.();
		} else {
			resolveB?.();
		}

		const results = await Promise.all([pendingA, pendingB]);
		expect(results).toEqual(
			takeoverOwner === "waiter-a" ? [true, false] : [false, true],
		);
		expect(refreshA.mock.calls.length + refreshB.mock.calls.length).toBe(1);
		expect(localStorage.getItem("aster-auth-refresh-lock")).toBeNull();
		expect(broadcastChannels.size).toBe(0);
	});

	it("renews the lock while the local refresh is still running", async () => {
		vi.useFakeTimers();
		let resolveRefresh: (() => void) | null = null;
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(
			() =>
				new Promise<void>((resolve) => {
					resolveRefresh = resolve;
				}),
		);

		const pending = runWithCrossTabRefreshLock(refresh);
		expect(refresh).toHaveBeenCalledTimes(1);
		const firstLock = readRefreshLock();

		await vi.advanceTimersByTimeAsync(5_000);
		const renewedLock = readRefreshLock();

		expect(renewedLock.lockId).toBe(firstLock.lockId);
		expect(renewedLock.ownerId).toBe(firstLock.ownerId);
		expect(renewedLock.expiresAt).toBeGreaterThan(firstLock.expiresAt);

		resolveRefresh?.();
		await expect(pending).resolves.toBe(true);
		expect(localStorage.getItem("aster-auth-refresh-lock")).toBeNull();

		await vi.advanceTimersByTimeAsync(5_000);
		expect(localStorage.getItem("aster-auth-refresh-lock")).toBeNull();
	});

	it("stops renewing when another tab steals the local refresh lock", async () => {
		vi.useFakeTimers();
		let resolveRefresh: (() => void) | null = null;
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(
			() =>
				new Promise<void>((resolve) => {
					resolveRefresh = resolve;
				}),
		);

		const pending = runWithCrossTabRefreshLock(refresh);
		expect(refresh).toHaveBeenCalledTimes(1);
		setRefreshLock({
			ownerId: "other-tab",
			lockId: "stolen-lock",
		});
		const stolenLock = readRefreshLock();

		await vi.advanceTimersByTimeAsync(5_000);

		expect(readRefreshLock()).toEqual(stolenLock);
		resolveRefresh?.();
		await expect(pending).resolves.toBe(true);
		expect(readRefreshLock()).toEqual(stolenLock);
		expect(localStorage.getItem("aster-auth-refresh-event")).toBeNull();
	});

	it("broadcasts and closes the channel after a local refresh succeeds", async () => {
		vi.stubGlobal("BroadcastChannel", MockBroadcastChannel);
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		const received: unknown[] = [];
		const observer = new MockBroadcastChannel("aster-auth-refresh");
		observer.addEventListener("message", (event) => {
			received.push(event.data);
		});

		await expect(runWithCrossTabRefreshLock(refresh)).resolves.toBe(true);

		expect(received).toEqual([
			expect.objectContaining({
				status: "success",
			}),
		]);
		expect([...broadcastChannels]).toEqual([observer]);
		observer.close();
		expect(broadcastChannels.size).toBe(0);
	});

	it("writes refresh results when BroadcastChannel is unavailable", async () => {
		vi.stubGlobal("BroadcastChannel", undefined);
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);

		await expect(runWithCrossTabRefreshLock(refresh)).resolves.toBe(true);

		expect(refresh).toHaveBeenCalledTimes(1);
		expect(localStorage.getItem("aster-auth-refresh-event")).toContain(
			'"status":"success"',
		);
	});

	it("ignores stale events from a previous refresh round", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({ lockId: "fresh-lock" });
		setRefreshEvent({ lockId: "stale-lock" });

		const pending = runWithCrossTabRefreshLock(refresh);
		dispatchRefreshEvent({ lockId: "fresh-lock" });

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("ignores events with the same lock id from a different owner", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		setRefreshLock({ lockId: "colliding-lock", ownerId: "peer-tab" });

		const pending = runWithCrossTabRefreshLock(refresh);
		dispatchRefreshEvent({
			lockId: "colliding-lock",
			ownerId: "different-peer",
		});
		await Promise.resolve();
		expect(refresh).not.toHaveBeenCalled();
		dispatchRefreshEvent({
			lockId: "colliding-lock",
			ownerId: "peer-tab",
		});

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("ignores malformed stored json and malformed refresh events", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);

		localStorage.setItem("aster-auth-refresh-lock", "{");
		await expect(runWithCrossTabRefreshLock(refresh)).resolves.toBe(true);
		expect(refresh).toHaveBeenCalledTimes(1);

		refresh.mockClear();
		setRefreshLock();
		localStorage.setItem("aster-auth-refresh-event", "{");
		const pending = runWithCrossTabRefreshLock(refresh);
		dispatchRefreshEvent();

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});

	it("ignores invalid lock shapes and refreshes locally", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		localStorage.setItem(
			"aster-auth-refresh-lock",
			JSON.stringify({
				ownerId: "peer-tab",
				lockId: "peer-lock",
				expiresAt: "soon",
			}),
		);

		await expect(runWithCrossTabRefreshLock(refresh)).resolves.toBe(true);

		expect(refresh).toHaveBeenCalledTimes(1);
		expect(localStorage.getItem("aster-auth-refresh-lock")).toBeNull();
	});

	it("reports a failure event and releases the lock when local refresh fails", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refreshError = new Error("refresh failed");
		const refresh = vi.fn(async () => {
			throw refreshError;
		});

		await expect(runWithCrossTabRefreshLock(refresh)).rejects.toBe(
			refreshError,
		);

		expect(refresh).toHaveBeenCalledTimes(1);
		expect(localStorage.getItem("aster-auth-refresh-lock")).toBeNull();
		expect(
			JSON.parse(localStorage.getItem("aster-auth-refresh-event") ?? "{}"),
		).toMatchObject({
			status: "failure",
		});
	});

	it("does not release or report against a newer same-tab lock", async () => {
		let resolveRefresh: (() => void) | null = null;
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(
			() =>
				new Promise<void>((resolve) => {
					resolveRefresh = resolve;
				}),
		);

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.waitFor(() => {
			expect(refresh).toHaveBeenCalledTimes(1);
		});
		const firstLock = readRefreshLock();
		setRefreshLock({
			ownerId: firstLock.ownerId,
			lockId: "newer-same-tab-lock",
		});

		resolveRefresh?.();
		await expect(pending).resolves.toBe(true);

		expect(
			JSON.parse(localStorage.getItem("aster-auth-refresh-lock") ?? "{}"),
		).toMatchObject({
			ownerId: firstLock.ownerId,
			lockId: "newer-same-tab-lock",
		});
		expect(localStorage.getItem("aster-auth-refresh-event")).toBeNull();
	});

	it("does not release or report against a lock stolen by another tab", async () => {
		let resolveRefresh: (() => void) | null = null;
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(
			() =>
				new Promise<void>((resolve) => {
					resolveRefresh = resolve;
				}),
		);

		const pending = runWithCrossTabRefreshLock(refresh);
		await vi.waitFor(() => {
			expect(refresh).toHaveBeenCalledTimes(1);
		});
		setRefreshLock({
			ownerId: "other-tab",
			lockId: "stolen-lock",
		});

		resolveRefresh?.();
		await expect(pending).resolves.toBe(true);

		expect(readRefreshLock()).toMatchObject({
			ownerId: "other-tab",
			lockId: "stolen-lock",
		});
		expect(localStorage.getItem("aster-auth-refresh-event")).toBeNull();
	});
});
