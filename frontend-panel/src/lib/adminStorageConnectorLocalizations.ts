import type { i18n as I18n } from "i18next";
import { adminPolicyService } from "@/services/adminService";
import type {
	StorageConnectorCatalogContext,
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

export function installAdminStorageConnectorLocalizations(
	catalog: StorageConnectorLocalizationCatalog,
	language: string,
	i18n: Pick<I18n, "addResourceBundle">,
) {
	for (const resource of catalog.resources) {
		i18n.addResourceBundle(
			language,
			resource.namespace,
			resource.messages,
			true,
			true,
		);
	}
}

export function translateStorageConnectorMessage(
	t: (key: string, options?: Record<string, number | string>) => unknown,
	connectorId: string | null | undefined,
	messageId: string,
	options?: Record<string, number | string>,
) {
	const fallback = String(
		t(messageId, {
			...options,
			ns: "admin",
			defaultValue: messageId,
		}),
	);
	if (!connectorId) {
		return fallback;
	}
	return String(
		t(messageId, {
			...options,
			ns: connectorId,
			defaultValue: fallback,
		}),
	);
}
