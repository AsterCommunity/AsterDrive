import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	listRemoteNodes: vi.fn(),
}));

vi.mock("@/services/adminService", () => ({
	adminRemoteNodeService: {
		list: (...args: unknown[]) => mockState.listRemoteNodes(...args),
	},
}));

function createRemoteNode(id: number) {
	return {
		base_url: `https://node-${id}.example.com`,
		created_at: "2026-03-28T00:00:00Z",
		id,
		last_seen_at: null,
		name: `Node ${id}`,
		node_key: `node-${id}`,
		status: "active",
		updated_at: "2026-03-28T00:00:00Z",
	};
}

async function loadLookup() {
	vi.resetModules();
	return await import("@/lib/adminRemoteNodeLookup");
}

describe("adminRemoteNodeLookup", () => {
	beforeEach(() => {
		mockState.listRemoteNodes.mockReset();
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-05-07T00:00:00Z"));
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("loads all pages and reuses the cache inside the freshness window", async () => {
		const remoteNodes = Array.from({ length: 3 }, (_, index) =>
			createRemoteNode(index + 1),
		);
		mockState.listRemoteNodes.mockImplementation(
			async (params?: { limit?: number; offset?: number }) => {
				const limit = params?.limit ?? 2;
				const offset = params?.offset ?? 0;
				return {
					items: remoteNodes.slice(offset, offset + limit),
					total: remoteNodes.length,
				};
			},
		);

		const { loadAdminRemoteNodeLookup } = await loadLookup();

		await expect(loadAdminRemoteNodeLookup({ limit: 2 })).resolves.toEqual(
			remoteNodes,
		);
		await expect(loadAdminRemoteNodeLookup({ limit: 2 })).resolves.toEqual(
			remoteNodes,
		);

		expect(mockState.listRemoteNodes).toHaveBeenCalledTimes(2);
		expect(mockState.listRemoteNodes).toHaveBeenNthCalledWith(1, {
			limit: 2,
			offset: 0,
		});
		expect(mockState.listRemoteNodes).toHaveBeenNthCalledWith(2, {
			limit: 2,
			offset: 2,
		});
	});

	it("deduplicates concurrent non-forced loads", async () => {
		let resolveLoad!: (value: { items: unknown[]; total: number }) => void;
		mockState.listRemoteNodes.mockReturnValueOnce(
			new Promise((resolve) => {
				resolveLoad = resolve;
			}),
		);

		const { loadAdminRemoteNodeLookup } = await loadLookup();

		const firstLoad = loadAdminRemoteNodeLookup();
		const secondLoad = loadAdminRemoteNodeLookup();

		expect(mockState.listRemoteNodes).toHaveBeenCalledTimes(1);

		resolveLoad({ items: [createRemoteNode(1)], total: 1 });

		await expect(Promise.all([firstLoad, secondLoad])).resolves.toEqual([
			[createRemoteNode(1)],
			[createRemoteNode(1)],
		]);
	});

	it("revalidates after the TTL and supports explicit invalidation", async () => {
		const { invalidateAdminRemoteNodeLookup, loadAdminRemoteNodeLookup } =
			await loadLookup();
		mockState.listRemoteNodes
			.mockResolvedValueOnce({ items: [createRemoteNode(1)], total: 1 })
			.mockResolvedValueOnce({ items: [createRemoteNode(2)], total: 1 })
			.mockResolvedValueOnce({ items: [createRemoteNode(3)], total: 1 });

		await expect(loadAdminRemoteNodeLookup()).resolves.toEqual([
			createRemoteNode(1),
		]);

		vi.advanceTimersByTime(30_001);
		await expect(loadAdminRemoteNodeLookup()).resolves.toEqual([
			createRemoteNode(2),
		]);

		invalidateAdminRemoteNodeLookup();
		await expect(loadAdminRemoteNodeLookup()).resolves.toEqual([
			createRemoteNode(3),
		]);

		expect(mockState.listRemoteNodes).toHaveBeenCalledTimes(3);
	});

	it("keeps a forced refresh from being overwritten by an older request", async () => {
		let resolveSlowLoad!: (value: { items: unknown[]; total: number }) => void;
		mockState.listRemoteNodes
			.mockReturnValueOnce(
				new Promise((resolve) => {
					resolveSlowLoad = resolve;
				}),
			)
			.mockResolvedValueOnce({ items: [createRemoteNode(2)], total: 1 });

		const { loadAdminRemoteNodeLookup, readAdminRemoteNodeLookup } =
			await loadLookup();

		const slowLoad = loadAdminRemoteNodeLookup();
		await expect(loadAdminRemoteNodeLookup({ force: true })).resolves.toEqual([
			createRemoteNode(2),
		]);

		resolveSlowLoad({ items: [createRemoteNode(1)], total: 1 });
		await expect(slowLoad).resolves.toEqual([createRemoteNode(1)]);

		expect(readAdminRemoteNodeLookup()).toEqual([createRemoteNode(2)]);
	});
});
