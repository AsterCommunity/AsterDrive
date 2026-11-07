import type { i18n as I18n } from "i18next";
import { adminPolicyService } from "@/services/adminService";
import type {
	StorageConnectorCatalogContext,
	StorageConnectorDescriptor,
	StorageConnectorLocalizationCatalog,
} from "@/types/api";

const DEFAULT_CATALOG_CONTEXT: StorageConnectorCatalogContext = "manage";

interface LocalizationCacheEntry {
	catalog: StorageConnectorLocalizationCatalog | null;
	pendingRequest: Promise<StorageConnectorLocalizationCatalog> | null;
	requestSerial: number;
}

const localizationCaches = new Map<string, LocalizationCacheEntry>();

function cacheKey(context: StorageConnectorCatalogContext, locale: string) {
	return `${context}:${locale}`;
}

function localizationCache(
	context: StorageConnectorCatalogContext,
	locale: string,
) {
	const key = cacheKey(context, locale);
	let cache = localizationCaches.get(key);
	if (!cache) {
		cache = { catalog: null, pendingRequest: null, requestSerial: 0 };
		localizationCaches.set(key, cache);
	}
	return cache;
}

export function invalidateAdminStorageConnectorLocalizations() {
	for (const cache of localizationCaches.values()) {
		cache.catalog = null;
		cache.pendingRequest = null;
		cache.requestSerial += 1;
	}
}

export async function loadAdminStorageConnectorLocalizations(options: {
	context?: StorageConnectorCatalogContext;
	force?: boolean;
	locale: string;
}) {
	const context = options.context ?? DEFAULT_CATALOG_CONTEXT;
	const locale = options.locale.trim() || "en";
	const cache = localizationCache(context, locale);
	if (!options.force && cache.catalog) {
		return cache.catalog;
	}
	if (!options.force && cache.pendingRequest) {
		return cache.pendingRequest;
	}

	const requestSerial = ++cache.requestSerial;
	const request = adminPolicyService
		.listStorageDriverLocalizations({
			context,
			locale,
		})
		.then((catalog) => {
			if (requestSerial === cache.requestSerial) {
				cache.catalog = catalog;
			}
			return catalog;
		})
		.finally(() => {
			if (cache.pendingRequest === request) {
				cache.pendingRequest = null;
			}
		});
	cache.pendingRequest = request;
	return request;
}

type LocalizationOwner =
	| { kind: "primary"; context: StorageConnectorCatalogContext }
	| { kind: "remote"; nodeId: number };

type LocalizationRuntime = Pick<
	I18n,
	"addResourceBundle" | "removeResourceBundle"
>;

type ConnectorTranslationSource = Pick<
	StorageConnectorDescriptor,
	"connector_id"
> & {
	/** Client-owned display scope, overwritten when adapting a remote catalog. */
	readonly localizationNamespace?: string;
};

const installedNamespaces = new WeakMap<object, Map<string, Set<string>>>();
const MAX_CATALOG_RESOURCES = 128;
const MAX_CATALOG_MESSAGES = 10_000;
const MAX_CATALOG_CHARACTERS = 1024 * 1024;

function ownerKey(owner: LocalizationOwner) {
	switch (owner.kind) {
		case "primary":
			return `primary-${owner.context}`;
		case "remote":
			return `remote-${owner.nodeId}`;
	}
}

function connectorNamespace(connectorId: string, nodeId?: number) {
	// Neither an API namespace nor a connector ID can select an application bundle.
	return `aster_storage_${nodeId === undefined ? "primary" : `remote_${nodeId}`}_${encodeURIComponent(connectorId)}`;
}

export function installStorageConnectorLocalizations(
	catalog: StorageConnectorLocalizationCatalog,
	language: string,
	i18n: LocalizationRuntime,
	owner: LocalizationOwner,
) {
	if (catalog.resources.length > MAX_CATALOG_RESOURCES) {
		throw new Error(
			"Storage connector localization catalog exceeds resource budget",
		);
	}
	const prepared: Array<{
		namespace: string;
		messages: Record<string, string>;
	}> = [];
	let messages = 0;
	let characters = 0;
	const seen = new Set<string>();
	for (const resource of catalog.resources) {
		if (
			typeof resource.connector_id !== "string" ||
			resource.connector_id.length === 0 ||
			resource.connector_id.length > 256 ||
			!resource.messages ||
			typeof resource.messages !== "object" ||
			Array.isArray(resource.messages)
		) {
			throw new Error("Invalid connector localization resource");
		}
		const namespace = connectorNamespace(
			resource.connector_id,
			owner.kind === "remote" ? owner.nodeId : undefined,
		);
		if (seen.has(namespace))
			throw new Error("Duplicate connector localization resource");
		seen.add(namespace);
		const safeMessages: Record<string, string> = Object.create(null);
		for (const [key, value] of Object.entries(resource.messages)) {
			if (typeof value !== "string")
				throw new Error("Invalid connector localization message");
			messages += 1;
			characters += key.length + value.length;
			if (
				messages > MAX_CATALOG_MESSAGES ||
				characters > MAX_CATALOG_CHARACTERS
			) {
				throw new Error(
					"Storage connector localization catalog exceeds message budget",
				);
			}
			// Plain scalar keys only: no namespace escape or prototype paths.
			if (
				key === "__proto__" ||
				key === "constructor" ||
				key === "prototype" ||
				key.includes(":")
			)
				continue;
			safeMessages[key] = value;
		}
		prepared.push({ namespace, messages: safeMessages });
	}
	let owners = installedNamespaces.get(i18n);
	if (!owners) {
		owners = new Map();
		installedNamespaces.set(i18n, owners);
	}
	const key = `${language}:${ownerKey(owner)}`;
	const previous = owners.get(key) ?? new Set<string>();
	const current = new Set(prepared.map((resource) => resource.namespace));
	for (const namespace of previous) {
		if (
			!current.has(namespace) &&
			![...owners.entries()].some(
				([otherOwner, namespaces]) =>
					otherOwner !== key &&
					otherOwner.startsWith(`${language}:`) &&
					namespaces.has(namespace),
			)
		)
			i18n.removeResourceBundle(language, namespace);
	}
	for (const resource of prepared) {
		// Replace the complete bundle so removed keys do not survive a refresh.
		i18n.removeResourceBundle(language, resource.namespace);
		i18n.addResourceBundle(
			language,
			resource.namespace,
			resource.messages,
			false,
			false,
		);
	}
	owners.set(key, current);
}

/** Display descriptors carry local node identity; wire connector IDs remain unchanged. */
export function scopeRemoteStorageConnectorDescriptors(
	descriptors: StorageConnectorDescriptor[],
	nodeId: number,
) {
	if (descriptors.length > MAX_CATALOG_RESOURCES)
		throw new Error(
			"Storage connector descriptor catalog exceeds resource budget",
		);
	return descriptors.map((descriptor) => ({
		...descriptor,
		localizationNamespace: connectorNamespace(descriptor.connector_id, nodeId),
	}));
}

export function translateStorageConnectorMessage(
	t: (key: string, options?: Record<string, number | string>) => unknown,
	connector: ConnectorTranslationSource | null | undefined,
	messageId: string,
	options?: Record<string, number | string>,
) {
	const translationOptions = { ...options, nsSeparator: "", keySeparator: "" };
	const fallback = String(
		t(messageId, {
			...translationOptions,
			ns: "admin",
			defaultValue: messageId,
		}),
	);
	if (!connector) return fallback;
	return String(
		t(messageId, {
			...translationOptions,
			ns:
				connector.localizationNamespace ??
				connectorNamespace(connector.connector_id),
			defaultValue: fallback,
		}),
	);
}
