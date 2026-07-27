import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	invalidateAdminStorageDriverDescriptors,
	loadAdminStorageDriverDescriptors,
	readAdminStorageDriverDescriptors,
} from "@/lib/adminStorageDriverDescriptors";

const mocks = vi.hoisted(() => ({
	listStorageDriverDescriptors: vi.fn(),
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		listStorageDriverDescriptors: (...args: unknown[]) =>
			mocks.listStorageDriverDescriptors(...args),
	},
}));

function descriptor(driverType: "local" | "s3") {
	return { driver_type: driverType } as never;
}

function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((resolvePromise) => {
		resolve = resolvePromise;
	});
	return { promise, resolve };
}

describe("adminStorageDriverDescriptors", () => {
	beforeEach(() => {
		invalidateAdminStorageDriverDescriptors();
		mocks.listStorageDriverDescriptors.mockReset();
	});

	it("keeps manage, create, and setup caches isolated", async () => {
		mocks.listStorageDriverDescriptors.mockImplementation(
			async (query?: { context?: "create" | "setup" }) => {
				if (query?.context === "setup") {
					return [descriptor("s3")];
				}
				if (query?.context === "create") {
					return [descriptor("local"), descriptor("s3")];
				}
				return [descriptor("local")];
			},
		);

		await Promise.all([
			loadAdminStorageDriverDescriptors({ context: "manage" }),
			loadAdminStorageDriverDescriptors({ context: "create" }),
			loadAdminStorageDriverDescriptors({ context: "setup" }),
		]);

		expect(readAdminStorageDriverDescriptors("manage")).toEqual([
			descriptor("local"),
		]);
		expect(readAdminStorageDriverDescriptors("create")).toEqual([
			descriptor("local"),
			descriptor("s3"),
		]);
		expect(readAdminStorageDriverDescriptors("setup")).toEqual([
			descriptor("s3"),
		]);
		expect(mocks.listStorageDriverDescriptors).toHaveBeenCalledWith(undefined);
		expect(mocks.listStorageDriverDescriptors).toHaveBeenCalledWith({
			context: "create",
		});
		expect(mocks.listStorageDriverDescriptors).toHaveBeenCalledWith({
			context: "setup",
		});
	});

	it("deduplicates concurrent requests within the same catalog context", async () => {
		const request = deferred<never[]>();
		mocks.listStorageDriverDescriptors.mockReturnValue(request.promise);

		const first = loadAdminStorageDriverDescriptors({ context: "setup" });
		const second = loadAdminStorageDriverDescriptors({ context: "setup" });
		request.resolve([]);

		await expect(first).resolves.toEqual([]);
		await expect(second).resolves.toEqual([]);
		expect(mocks.listStorageDriverDescriptors).toHaveBeenCalledTimes(1);
	});

	it("does not let a stale request repopulate caches after invalidation", async () => {
		const stale = deferred<never[]>();
		mocks.listStorageDriverDescriptors
			.mockReturnValueOnce(stale.promise)
			.mockResolvedValueOnce([descriptor("s3")]);

		const staleLoad = loadAdminStorageDriverDescriptors({ context: "setup" });
		invalidateAdminStorageDriverDescriptors();
		await loadAdminStorageDriverDescriptors({ context: "setup" });
		stale.resolve([descriptor("local")]);
		await staleLoad;

		expect(readAdminStorageDriverDescriptors("setup")).toEqual([
			descriptor("s3"),
		]);
	});
});
