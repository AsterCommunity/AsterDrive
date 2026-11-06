import { beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	list: vi.fn(),
	warn: vi.fn(),
}));

vi.mock("@/services/teamService", () => ({
	teamService: {
		list: (...args: unknown[]) => mockState.list(...args),
	},
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: (...args: unknown[]) => mockState.warn(...args),
		error: vi.fn(),
		debug: vi.fn(),
	},
}));

function createDeferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (reason?: unknown) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

async function loadTeamStore() {
	vi.resetModules();
	return await import("@/stores/teamStore");
}

describe("teamStore", () => {
	beforeEach(() => {
		mockState.list.mockReset();
		mockState.warn.mockReset();
	});

	it("loads teams once per user and shares the in-flight request", async () => {
		const deferred = createDeferred<Array<{ id: number; name: string }>>();
		mockState.list.mockReturnValue(deferred.promise);

		const { useTeamStore } = await loadTeamStore();

		const firstLoad = useTeamStore.getState().ensureLoaded(7);
		const secondLoad = useTeamStore.getState().ensureLoaded(7);

		await new Promise((resolve) => setTimeout(resolve, 0));

		expect(mockState.list).toHaveBeenCalledTimes(1);
		expect(useTeamStore.getState().loading).toBe(true);

		deferred.resolve([{ id: 1, name: "Core" }]);
		await Promise.all([firstLoad, secondLoad]);

		expect(useTeamStore.getState()).toMatchObject({
			teams: [{ id: 1, name: "Core" }],
			loading: false,
			loadedForUserId: 7,
		});

		await useTeamStore.getState().ensureLoaded(7);

		expect(mockState.list).toHaveBeenCalledTimes(1);
	});

	it("reloads when the user changes and clears state when invalidated", async () => {
		mockState.list
			.mockResolvedValueOnce([{ id: 1, name: "Core" }])
			.mockResolvedValueOnce([{ id: 2, name: "Ops" }]);

		const { useTeamStore } = await loadTeamStore();

		await useTeamStore.getState().ensureLoaded(7);
		await useTeamStore.getState().ensureLoaded(9);

		expect(mockState.list).toHaveBeenCalledTimes(2);
		expect(useTeamStore.getState()).toMatchObject({
			teams: [{ id: 2, name: "Ops" }],
			loadedForUserId: 9,
		});

		useTeamStore.getState().invalidate();
		expect(useTeamStore.getState().loadedForUserId).toBeNull();

		await useTeamStore.getState().reload(null);
		expect(useTeamStore.getState()).toMatchObject({
			teams: [],
			loading: false,
			loadedForUserId: null,
		});

		mockState.list.mockResolvedValueOnce([{ id: 3, name: "Design" }]);
		await useTeamStore.getState().reload(11);

		expect(useTeamStore.getState()).toMatchObject({
			teams: [{ id: 3, name: "Design" }],
			loading: false,
			loadedForUserId: 11,
		});

		useTeamStore.getState().clear();
		expect(useTeamStore.getState()).toMatchObject({
			teams: [],
			loading: false,
			loadedForUserId: null,
		});
	});

	it("logs failures, resets state, and allows a retry", async () => {
		mockState.list
			.mockRejectedValueOnce(new Error("offline"))
			.mockResolvedValueOnce([{ id: 4, name: "QA" }]);

		const { useTeamStore } = await loadTeamStore();

		await expect(useTeamStore.getState().ensureLoaded(7)).rejects.toThrow(
			"offline",
		);

		expect(mockState.warn).toHaveBeenCalledWith(
			"Failed to load teams",
			expect.any(Error),
		);
		expect(useTeamStore.getState()).toMatchObject({
			teams: [],
			loading: false,
			loadedForUserId: null,
		});

		await useTeamStore.getState().ensureLoaded(7);

		expect(mockState.list).toHaveBeenCalledTimes(2);
		expect(useTeamStore.getState()).toMatchObject({
			teams: [{ id: 4, name: "QA" }],
			loading: false,
			loadedForUserId: 7,
		});
	});

	it("discards a previous account response without clearing the new request", async () => {
		const oldRequest = createDeferred<Array<{ id: number; name: string }>>();
		const newRequest = createDeferred<Array<{ id: number; name: string }>>();
		mockState.list
			.mockReturnValueOnce(oldRequest.promise)
			.mockReturnValueOnce(newRequest.promise);
		const { useTeamStore } = await loadTeamStore();
		const oldLoad = useTeamStore.getState().ensureLoaded(7);
		await vi.waitFor(() => expect(mockState.list).toHaveBeenCalledTimes(1));
		useTeamStore.getState().clear();
		const newLoad = useTeamStore.getState().ensureLoaded(9);
		await vi.waitFor(() => expect(mockState.list).toHaveBeenCalledTimes(2));
		oldRequest.resolve([{ id: 1, name: "Private old team" }]);
		await oldLoad;
		expect(useTeamStore.getState()).toMatchObject({
			teams: [],
			loading: true,
			loadedForUserId: null,
		});
		const sharedLoad = useTeamStore.getState().ensureLoaded(9);
		newRequest.resolve([{ id: 2, name: "New team" }]);
		await Promise.all([newLoad, sharedLoad]);
		expect(mockState.list).toHaveBeenCalledTimes(2);
		expect(useTeamStore.getState()).toMatchObject({
			teams: [{ id: 2, name: "New team" }],
			loadedForUserId: 9,
		});
	});
});
