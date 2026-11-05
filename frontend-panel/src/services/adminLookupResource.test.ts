import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	ADMIN_LOOKUP_CACHE_TTL_MS,
	createAdminLookupResource,
} from "./adminLookupResource";

const mocks = vi.hoisted(() => ({ search: vi.fn(), get: vi.fn() }));
vi.mock("@/services/adminLookupService", () => ({ adminLookupService: mocks }));
const option = { id: 7, label: "Archive", description: "#7" };
const page = { items: [option], total: 1 };
const signal = () => new AbortController().signal;

beforeEach(() => {
	vi.clearAllMocks();
	mocks.search.mockResolvedValue(page);
	mocks.get.mockResolvedValue(option);
});
afterEach(() => vi.useRealTimers());

describe("admin lookup resource", () => {
	it("reuses pages and identities for 30 seconds and keeps kinds, keywords and offsets separate", async () => {
		vi.useFakeTimers();
		const resource = createAdminLookupResource();
		await resource.search("policies", " archive ", 0, signal());
		await resource.search("policies", "archive", 0, signal());
		expect(mocks.search).toHaveBeenCalledTimes(1);
		await resource.search("users", "archive", 0, signal());
		await resource.search("policies", "archive", 20, signal());
		await resource.search("policies", "other", 0, signal());
		expect(mocks.search).toHaveBeenCalledTimes(4);
		await resource.get("policies", 7, signal());
		await resource.get("policies", 7, signal());
		expect(mocks.get).toHaveBeenCalledTimes(1);
		vi.advanceTimersByTime(ADMIN_LOOKUP_CACHE_TTL_MS);
		await resource.search("policies", "archive", 0, signal());
		await resource.get("policies", 7, signal());
		expect(mocks.search).toHaveBeenCalledTimes(5);
		expect(mocks.get).toHaveBeenCalledTimes(2);
	});
	it("shares an in-flight request without letting one consumer cancel the others", async () => {
		let finish!: (value: typeof page) => void;
		mocks.search.mockReturnValueOnce(
			new Promise((resolve) => {
				finish = resolve;
			}),
		);
		const resource = createAdminLookupResource();
		const first = new AbortController();
		const one = resource.search("users", "", 0, first.signal);
		const two = resource.search("users", "", 0, signal());
		const aborted = expect(one).rejects.toMatchObject({ name: "AbortError" });
		first.abort();
		await aborted;
		expect(mocks.search).toHaveBeenCalledTimes(1);
		expect(mocks.search.mock.calls[0][3].aborted).toBe(false);
		finish(page);
		await expect(two).resolves.toEqual(page);
	});
	it("aborts unused requests and never lets their late results replace a newer page", async () => {
		let finish!: (value: typeof page) => void;
		mocks.search.mockReturnValueOnce(
			new Promise((resolve) => {
				finish = resolve;
			}),
		);
		const resource = createAdminLookupResource();
		const controller = new AbortController();
		const old = resource.search("users", "", 0, controller.signal);
		const aborted = expect(old).rejects.toMatchObject({ name: "AbortError" });
		controller.abort();
		await aborted;
		expect(mocks.search.mock.calls[0][3].aborted).toBe(true);
		const fresh = { items: [{ ...option, label: "New" }], total: 1 };
		mocks.search.mockResolvedValueOnce(fresh);
		await expect(resource.search("users", "", 0, signal())).resolves.toEqual(
			fresh,
		);
		finish(page);
		await expect(resource.search("users", "", 0, signal())).resolves.toEqual(
			fresh,
		);
	});
	it("does not cache failures, supports forced reloads and isolates route sessions", async () => {
		const resource = createAdminLookupResource();
		mocks.search.mockRejectedValueOnce(new Error("offline"));
		await expect(resource.search("users", "", 0, signal())).rejects.toThrow(
			"offline",
		);
		await resource.search("users", "", 0, signal());
		await resource.search("users", "", 0, signal(), true);
		await createAdminLookupResource().search("users", "", 0, signal());
		expect(mocks.search).toHaveBeenCalledTimes(4);
	});
	it("bounds the search cache and evicts the least recently used result", async () => {
		const resource = createAdminLookupResource();
		for (let i = 0; i < 101; i++)
			await resource.search("users", String(i), 0, signal());
		await resource.search("users", "100", 0, signal());
		expect(mocks.search).toHaveBeenCalledTimes(101);
		await resource.search("users", "0", 0, signal());
		expect(mocks.search).toHaveBeenCalledTimes(102);
	});
});
