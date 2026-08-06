import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	installAdminStorageConnectorLocalizations,
	invalidateAdminStorageConnectorLocalizations,
	loadAdminStorageConnectorLocalizations,
	translateStorageConnectorMessage,
} from "@/lib/adminStorageConnectorLocalizations";

const mocks = vi.hoisted(() => ({
	addResourceBundle: vi.fn(),
	listStorageDriverLocalizations: vi.fn(),
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		listStorageDriverLocalizations: (...args: unknown[]) =>
			mocks.listStorageDriverLocalizations(...args),
	},
}));

const catalog = {
	requested_locale: "zh-CN",
	resources: [
		{
			connector_id: "com.example.storage",
			messages: { title: "存储" },
			namespace: "com.example.storage",
			requested_locale: "zh-CN",
			resolved_locale: "zh",
			revision: "revision",
		},
	],
} as never;

function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((resolvePromise) => {
		resolve = resolvePromise;
	});
	return { promise, resolve };
}

describe("adminStorageConnectorLocalizations", () => {
	beforeEach(() => {
		invalidateAdminStorageConnectorLocalizations();
		mocks.addResourceBundle.mockReset();
		mocks.listStorageDriverLocalizations.mockReset();
	});

	it("deduplicates the same context and locale while isolating other locales", async () => {
		mocks.listStorageDriverLocalizations.mockResolvedValue(catalog);

		await Promise.all([
			loadAdminStorageConnectorLocalizations({
				context: "create",
				locale: "zh-CN",
			}),
			loadAdminStorageConnectorLocalizations({
				context: "create",
				locale: "zh-CN",
			}),
		]);
		await loadAdminStorageConnectorLocalizations({
			context: "create",
			locale: "en",
		});

		expect(mocks.listStorageDriverLocalizations).toHaveBeenCalledTimes(2);
	});

	it("does not let a stale request replace a forced refresh for the same locale", async () => {
		const stale = deferred<typeof catalog>();
		const refreshed = {
			...catalog,
			resources: [
				{
					...catalog.resources[0],
					messages: { title: "Refreshed" },
					revision: "refreshed-revision",
				},
			],
		};
		mocks.listStorageDriverLocalizations
			.mockReturnValueOnce(stale.promise)
			.mockResolvedValueOnce(refreshed);

		const staleLoad = loadAdminStorageConnectorLocalizations({
			context: "manage",
			locale: "en",
		});
		await expect(
			loadAdminStorageConnectorLocalizations({
				context: "manage",
				force: true,
				locale: "en",
			}),
		).resolves.toEqual(refreshed);
		stale.resolve(catalog);
		await staleLoad;

		await expect(
			loadAdminStorageConnectorLocalizations({
				context: "manage",
				locale: "en",
			}),
		).resolves.toEqual(refreshed);
		expect(mocks.listStorageDriverLocalizations).toHaveBeenCalledTimes(2);
	});

	it("installs resolved plugin messages under the requested frontend language", () => {
		installAdminStorageConnectorLocalizations(catalog, "zh-CN", {
			addResourceBundle: mocks.addResourceBundle,
		} as never);

		expect(mocks.addResourceBundle).toHaveBeenCalledWith(
			"zh-CN",
			"com.example.storage",
			{ title: "存储" },
			true,
			true,
		);
	});

	it("uses the connector namespace and preserves an admin fallback", () => {
		const t = vi.fn((key: string, options?: Record<string, unknown>) => {
			if (options?.ns === "com.example.storage") return "Plugin title";
			if (options?.ns === "admin") return "Fallback title";
			return key;
		});

		expect(
			translateStorageConnectorMessage(
				t as never,
				"com.example.storage",
				"title",
			),
		).toBe("Plugin title");
		expect(t).toHaveBeenCalledWith(
			"title",
			expect.objectContaining({
				defaultValue: "Fallback title",
				ns: "com.example.storage",
			}),
		);
	});
});
