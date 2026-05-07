import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	listAllPolicyGroups: vi.fn(),
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyGroupService: {
		listAll: (...args: unknown[]) => mockState.listAllPolicyGroups(...args),
	},
}));

function createPolicyGroup(id: number) {
	return {
		created_at: "2026-03-28T00:00:00Z",
		description: "",
		id,
		is_default: false,
		is_enabled: true,
		items: [],
		name: `Group ${id}`,
		updated_at: "2026-03-28T00:00:00Z",
	};
}

async function loadLookup() {
	vi.resetModules();
	return await import("@/lib/adminPolicyGroupLookup");
}

describe("adminPolicyGroupLookup", () => {
	beforeEach(() => {
		mockState.listAllPolicyGroups.mockReset();
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-05-07T00:00:00Z"));
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("reuses fresh cache and revalidates after the TTL", async () => {
		const { loadAdminPolicyGroupLookup } = await loadLookup();
		mockState.listAllPolicyGroups
			.mockResolvedValueOnce([createPolicyGroup(1)])
			.mockResolvedValueOnce([createPolicyGroup(2)]);

		await expect(loadAdminPolicyGroupLookup()).resolves.toEqual([
			createPolicyGroup(1),
		]);
		await expect(loadAdminPolicyGroupLookup()).resolves.toEqual([
			createPolicyGroup(1),
		]);

		vi.advanceTimersByTime(30_001);
		await expect(loadAdminPolicyGroupLookup()).resolves.toEqual([
			createPolicyGroup(2),
		]);

		expect(mockState.listAllPolicyGroups).toHaveBeenCalledTimes(2);
	});

	it("deduplicates concurrent non-forced loads", async () => {
		let resolveLoad!: (value: unknown[]) => void;
		mockState.listAllPolicyGroups.mockReturnValueOnce(
			new Promise((resolve) => {
				resolveLoad = resolve;
			}),
		);

		const { loadAdminPolicyGroupLookup } = await loadLookup();

		const firstLoad = loadAdminPolicyGroupLookup();
		const secondLoad = loadAdminPolicyGroupLookup();

		expect(mockState.listAllPolicyGroups).toHaveBeenCalledTimes(1);

		resolveLoad([createPolicyGroup(1)]);

		await expect(Promise.all([firstLoad, secondLoad])).resolves.toEqual([
			[createPolicyGroup(1)],
			[createPolicyGroup(1)],
		]);
	});

	it("keeps a forced refresh from being overwritten by an older request", async () => {
		let resolveSlowLoad!: (value: unknown[]) => void;
		mockState.listAllPolicyGroups
			.mockReturnValueOnce(
				new Promise((resolve) => {
					resolveSlowLoad = resolve;
				}),
			)
			.mockResolvedValueOnce([createPolicyGroup(2)]);

		const { loadAdminPolicyGroupLookup, readAdminPolicyGroupLookup } =
			await loadLookup();

		const slowLoad = loadAdminPolicyGroupLookup();
		await expect(loadAdminPolicyGroupLookup({ force: true })).resolves.toEqual([
			createPolicyGroup(2),
		]);

		resolveSlowLoad([createPolicyGroup(1)]);
		await expect(slowLoad).resolves.toEqual([createPolicyGroup(1)]);

		expect(readAdminPolicyGroupLookup()).toEqual([createPolicyGroup(2)]);
	});
});
