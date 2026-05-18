import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

async function loadModule() {
	vi.resetModules();
	return await import("@/lib/crossTabRefresh");
}

describe("cross-tab refresh coordination", () => {
	beforeEach(() => {
		localStorage.clear();
		vi.useRealTimers();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("runs refresh immediately when no peer holds the lock", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);

		await expect(runWithCrossTabRefreshLock(refresh)).resolves.toBe(true);

		expect(refresh).toHaveBeenCalledTimes(1);
		expect(localStorage.getItem("aster-auth-refresh-lock")).toBeNull();
	});

	it("waits for another tab's successful refresh instead of refreshing again", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		localStorage.setItem(
			"aster-auth-refresh-lock",
			JSON.stringify({
				ownerId: "peer-tab",
				lockId: "peer-lock",
				expiresAt: Date.now() + 15_000,
			}),
		);

		const pending = runWithCrossTabRefreshLock(refresh);
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "aster-auth-refresh-event",
				newValue: JSON.stringify({
					ownerId: "peer-tab",
					lockId: "peer-lock",
					status: "success",
					createdAt: Date.now(),
				}),
			}),
		);

		await expect(pending).resolves.toBe(false);

		expect(refresh).not.toHaveBeenCalled();
	});

	it("rejects without repeating refresh when the peer reports failure", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		localStorage.setItem(
			"aster-auth-refresh-lock",
			JSON.stringify({
				ownerId: "peer-tab",
				lockId: "peer-lock",
				expiresAt: Date.now() + 15_000,
			}),
		);

		const pending = runWithCrossTabRefreshLock(refresh);
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "aster-auth-refresh-event",
				newValue: JSON.stringify({
					ownerId: "peer-tab",
					lockId: "peer-lock",
					status: "failure",
					createdAt: Date.now(),
				}),
			}),
		);

		await expect(pending).rejects.toThrow("peer auth refresh failed");
		expect(refresh).not.toHaveBeenCalled();
	});

	it("times out without repeating refresh when the peer never reports a result", async () => {
		vi.useFakeTimers();

		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		localStorage.setItem(
			"aster-auth-refresh-lock",
			JSON.stringify({
				ownerId: "peer-tab",
				lockId: "peer-lock",
				expiresAt: Date.now() + 15_000,
			}),
		);

		const pending = expect(runWithCrossTabRefreshLock(refresh)).rejects.toThrow(
			"peer auth refresh timed out",
		);
		await vi.advanceTimersByTimeAsync(20_000);

		await pending;
		expect(refresh).not.toHaveBeenCalled();
	});

	it("ignores stale events from a previous refresh round", async () => {
		const { runWithCrossTabRefreshLock } = await loadModule();
		const refresh = vi.fn(async () => undefined);
		localStorage.setItem(
			"aster-auth-refresh-lock",
			JSON.stringify({
				ownerId: "peer-tab",
				lockId: "fresh-lock",
				expiresAt: Date.now() + 15_000,
			}),
		);
		localStorage.setItem(
			"aster-auth-refresh-event",
			JSON.stringify({
				ownerId: "peer-tab",
				lockId: "stale-lock",
				status: "success",
				createdAt: Date.now(),
			}),
		);

		const pending = runWithCrossTabRefreshLock(refresh);
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "aster-auth-refresh-event",
				newValue: JSON.stringify({
					ownerId: "peer-tab",
					lockId: "fresh-lock",
					status: "success",
					createdAt: Date.now(),
				}),
			}),
		);

		await expect(pending).resolves.toBe(false);
		expect(refresh).not.toHaveBeenCalled();
	});
});
