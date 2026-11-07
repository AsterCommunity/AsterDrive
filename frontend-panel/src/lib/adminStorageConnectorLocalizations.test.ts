import { createInstance } from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	installStorageConnectorLocalizations,
	invalidateAdminStorageConnectorLocalizations,
	loadAdminStorageConnectorLocalizations,
	scopeRemoteStorageConnectorDescriptors,
	translateStorageConnectorMessage,
} from "@/lib/adminStorageConnectorLocalizations";
import type {
	StorageConnectorDescriptor,
	StorageConnectorLocalizationCatalog,
} from "@/types/api";

const mocks = vi.hoisted(() => ({
	addResourceBundle: vi.fn(),
	removeResourceBundle: vi.fn(),
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
		mocks.removeResourceBundle.mockReset();
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
		installStorageConnectorLocalizations(
			catalog,
			"zh-CN",
			{
				addResourceBundle: mocks.addResourceBundle,
				removeResourceBundle: mocks.removeResourceBundle,
			} as never,
			{ kind: "primary", context: "manage" },
		);

		expect(mocks.addResourceBundle).toHaveBeenCalledWith(
			"zh-CN",
			"aster_storage_primary_com.example.storage",
			{ title: "存储" },
			false,
			false,
		);
	});

	it("uses the connector namespace and preserves an admin fallback", () => {
		const t = vi.fn((key: string, options?: Record<string, unknown>) => {
			if (options?.ns === "aster_storage_primary_com.example.storage")
				return "Plugin title";
			if (options?.ns === "admin") return "Fallback title";
			return key;
		});

		expect(
			translateStorageConnectorMessage(
				t as never,
				{ connector_id: "com.example.storage" },
				"title",
			),
		).toBe("Plugin title");
		expect(t).toHaveBeenCalledWith(
			"title",
			expect.objectContaining({
				defaultValue: "Fallback title",
				ns: "aster_storage_primary_com.example.storage",
			}),
		);
	});
});

describe("remote connector localization trust boundary", () => {
	function resource(
		connectorId: string,
		messages: Record<string, string>,
		namespace = connectorId,
	) {
		return {
			connector_id: connectorId,
			messages,
			namespace,
			requested_locale: "en",
			resolved_locale: "en",
			revision: "1",
		};
	}
	function catalogOf(
		resources: StorageConnectorLocalizationCatalog["resources"],
	): StorageConnectorLocalizationCatalog {
		return { requested_locale: "en", resources };
	}
	function descriptor(connectorId: string): StorageConnectorDescriptor {
		return { connector_id: connectorId } as StorageConnectorDescriptor;
	}
	async function runtime() {
		const instance = createInstance();
		await instance.init({
			lng: "en",
			fallbackLng: "en",
			resources: {
				en: {
					admin: { permission: "Read only", retained: "Admin fallback" },
					core: { confirm: "Confirm" },
				},
			},
		});
		return instance;
	}

	it("isolates attacker-selected namespaces and connector IDs from application bundles", async () => {
		const i18n = await runtime();
		installStorageConnectorLocalizations(
			catalogOf([
				resource("admin", { permission: "Full access" }, "admin"),
				resource(
					"plugin.remote",
					{ confirm: "Accept anything", "admin:permission": "Escape" },
					"core",
				),
			]),
			"en",
			i18n,
			{ kind: "remote", nodeId: 7 },
		);
		expect(i18n.t("admin:permission")).toBe("Read only");
		expect(i18n.t("core:confirm")).toBe("Confirm");
		const [adminConnector, remote] = scopeRemoteStorageConnectorDescriptors(
			[descriptor("admin"), descriptor("plugin.remote")],
			7,
		);
		expect(
			translateStorageConnectorMessage(i18n.t, adminConnector, "permission"),
		).toBe("Full access");
		expect(
			translateStorageConnectorMessage(i18n.t, remote, "admin:permission"),
		).toBe("admin:permission");
	});

	it("keeps identical connector IDs on two nodes and the primary independent", async () => {
		const i18n = await runtime();
		for (const nodeId of [11, 12]) {
			installStorageConnectorLocalizations(
				catalogOf([resource("plugin.shared", { title: `Node ${nodeId}` })]),
				"en",
				i18n,
				{ kind: "remote", nodeId },
			);
		}
		installStorageConnectorLocalizations(
			catalogOf([resource("plugin.shared", { title: "Primary" })]),
			"en",
			i18n,
			{ kind: "primary", context: "manage" },
		);
		for (const nodeId of [11, 12]) {
			const [scoped] = scopeRemoteStorageConnectorDescriptors(
				[descriptor("plugin.shared")],
				nodeId,
			);
			expect(translateStorageConnectorMessage(i18n.t, scoped, "title")).toBe(
				`Node ${nodeId}`,
			);
		}
		expect(
			translateStorageConnectorMessage(
				i18n.t,
				descriptor("plugin.shared"),
				"title",
			),
		).toBe("Primary");
	});

	it("replaces changed messages, removes vanished keys/connectors, and preserves admin fallback", async () => {
		const i18n = await runtime();
		const owner = { kind: "remote", nodeId: 11 } as const;
		installStorageConnectorLocalizations(
			catalogOf([
				resource("plugin.keep", { title: "Old", removed: "Must disappear" }),
				resource("plugin.gone", { title: "Gone" }),
			]),
			"en",
			i18n,
			owner,
		);
		const [keep, gone] = scopeRemoteStorageConnectorDescriptors(
			[descriptor("plugin.keep"), descriptor("plugin.gone")],
			11,
		);
		installStorageConnectorLocalizations(
			catalogOf([resource("plugin.keep", { title: "New" })]),
			"en",
			i18n,
			owner,
		);
		expect(translateStorageConnectorMessage(i18n.t, keep, "title")).toBe("New");
		expect(translateStorageConnectorMessage(i18n.t, keep, "removed")).toBe(
			"removed",
		);
		expect(translateStorageConnectorMessage(i18n.t, gone, "title")).toBe(
			"title",
		);
		expect(translateStorageConnectorMessage(i18n.t, keep, "retained")).toBe(
			"Admin fallback",
		);
	});

	it("rejects duplicate resources and oversized catalogs before mutating translations", async () => {
		const i18n = await runtime();
		const add = vi.spyOn(i18n, "addResourceBundle");
		expect(() =>
			installStorageConnectorLocalizations(
				catalogOf([
					resource("plugin.same", { title: "one" }),
					resource("plugin.same", { title: "two" }),
				]),
				"en",
				i18n,
				{ kind: "remote", nodeId: 11 },
			),
		).toThrow("Duplicate");
		expect(() =>
			installStorageConnectorLocalizations(
				catalogOf([
					resource("plugin.large", { title: "x".repeat(1024 * 1024 + 1) }),
				]),
				"en",
				i18n,
				{ kind: "remote", nodeId: 11 },
			),
		).toThrow("budget");
		expect(add).not.toHaveBeenCalled();
	});
});
