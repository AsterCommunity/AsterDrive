import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	listPolicies: vi.fn(),
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		list: (...args: unknown[]) => mockState.listPolicies(...args),
	},
}));

function createPolicy(id: number) {
	return {
		allowed_types: [],
		base_path: "",
		bucket: "",
		chunk_size: 5 * 1024 * 1024,
		created_at: "2026-03-28T00:00:00Z",
		driver_type: "local",
		endpoint: "",
		id,
		is_default: false,
		max_file_size: 0,
		name: `Policy ${id}`,
		options: {},
		remote_node_id: null,
		updated_at: "2026-03-28T00:00:00Z",
	};
}

async function loadLookup() {
	vi.resetModules();
	return await import("@/lib/adminPolicyLookup");
}

describe("adminPolicyLookup", () => {
	beforeEach(() => {
		mockState.listPolicies.mockReset();
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-05-07T00:00:00Z"));
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("loads all pages and reuses the cache inside the freshness window", async () => {
		const policies = Array.from({ length: 3 }, (_, index) =>
			createPolicy(index + 1),
		);
		mockState.listPolicies.mockImplementation(
			async (params?: { limit?: number; offset?: number }) => {
				const limit = params?.limit ?? 2;
				const offset = params?.offset ?? 0;
				return {
					items: policies.slice(offset, offset + limit),
					total: policies.length,
				};
			},
		);

		const { loadAdminPolicyLookup } = await loadLookup();

		await expect(loadAdminPolicyLookup({ limit: 2 })).resolves.toEqual(
			policies,
		);
		await expect(loadAdminPolicyLookup({ limit: 2 })).resolves.toEqual(
			policies,
		);

		expect(mockState.listPolicies).toHaveBeenCalledTimes(2);
		expect(mockState.listPolicies).toHaveBeenNthCalledWith(1, {
			limit: 2,
			offset: 0,
		});
		expect(mockState.listPolicies).toHaveBeenNthCalledWith(2, {
			limit: 2,
			offset: 2,
		});
	});

	it("deduplicates concurrent non-forced loads", async () => {
		let resolveLoad!: (value: { items: unknown[]; total: number }) => void;
		mockState.listPolicies.mockReturnValueOnce(
			new Promise((resolve) => {
				resolveLoad = resolve;
			}),
		);

		const { loadAdminPolicyLookup } = await loadLookup();

		const firstLoad = loadAdminPolicyLookup();
		const secondLoad = loadAdminPolicyLookup();

		expect(mockState.listPolicies).toHaveBeenCalledTimes(1);

		resolveLoad({ items: [createPolicy(1)], total: 1 });

		await expect(Promise.all([firstLoad, secondLoad])).resolves.toEqual([
			[createPolicy(1)],
			[createPolicy(1)],
		]);
	});

	it("revalidates after the TTL and supports explicit invalidation", async () => {
		const { invalidateAdminPolicyLookup, loadAdminPolicyLookup } =
			await loadLookup();
		mockState.listPolicies
			.mockResolvedValueOnce({ items: [createPolicy(1)], total: 1 })
			.mockResolvedValueOnce({ items: [createPolicy(2)], total: 1 })
			.mockResolvedValueOnce({ items: [createPolicy(3)], total: 1 });

		await expect(loadAdminPolicyLookup()).resolves.toEqual([createPolicy(1)]);

		vi.advanceTimersByTime(30_001);
		await expect(loadAdminPolicyLookup()).resolves.toEqual([createPolicy(2)]);

		invalidateAdminPolicyLookup();
		await expect(loadAdminPolicyLookup()).resolves.toEqual([createPolicy(3)]);

		expect(mockState.listPolicies).toHaveBeenCalledTimes(3);
	});

	it("keeps a forced refresh from being overwritten by an older request", async () => {
		let resolveSlowLoad!: (value: { items: unknown[]; total: number }) => void;
		mockState.listPolicies
			.mockReturnValueOnce(
				new Promise((resolve) => {
					resolveSlowLoad = resolve;
				}),
			)
			.mockResolvedValueOnce({ items: [createPolicy(2)], total: 1 });

		const { loadAdminPolicyLookup, readAdminPolicyLookup } = await loadLookup();

		const slowLoad = loadAdminPolicyLookup();
		await expect(loadAdminPolicyLookup({ force: true })).resolves.toEqual([
			createPolicy(2),
		]);

		resolveSlowLoad({ items: [createPolicy(1)], total: 1 });
		await expect(slowLoad).resolves.toEqual([createPolicy(1)]);

		expect(readAdminPolicyLookup()).toEqual([createPolicy(2)]);
	});
});
