import { adminPolicyService } from "@/services/adminService";
import type {
	DriverType,
	StorageConnectorCatalogContext,
	StorageConnectorDescriptor,
} from "@/types/api";

export const ADMIN_STORAGE_DRIVER_DESCRIPTOR_CACHE_TTL_MS = 30_000;

const DEFAULT_CATALOG_CONTEXT: StorageConnectorCatalogContext = "manage";

interface DescriptorCacheEntry {
	descriptors: StorageConnectorDescriptor[] | null;
	loadedAt: number;
	pendingRequest: Promise<StorageConnectorDescriptor[]> | null;
	requestSerial: number;
}

const descriptorCaches = new Map<
	StorageConnectorCatalogContext,
	DescriptorCacheEntry
>();

function descriptorCache(context: StorageConnectorCatalogContext) {
	let cache = descriptorCaches.get(context);
	if (!cache) {
		cache = {
			descriptors: null,
			loadedAt: 0,
			pendingRequest: null,
			requestSerial: 0,
		};
		descriptorCaches.set(context, cache);
	}
	return cache;
}

function getFreshDescriptorCache(context: StorageConnectorCatalogContext) {
	const cache = descriptorCache(context);
	if (
		cache.descriptors != null &&
		Date.now() - cache.loadedAt < ADMIN_STORAGE_DRIVER_DESCRIPTOR_CACHE_TTL_MS
	) {
		return cache.descriptors;
	}
	return null;
}

export function readAdminStorageDriverDescriptors(
	context: StorageConnectorCatalogContext = DEFAULT_CATALOG_CONTEXT,
) {
	return descriptorCache(context).descriptors;
}

export function primeAdminStorageDriverDescriptors(
	descriptors: StorageConnectorDescriptor[],
	context: StorageConnectorCatalogContext = DEFAULT_CATALOG_CONTEXT,
) {
	const cache = descriptorCache(context);
	cache.descriptors = descriptors;
	cache.loadedAt = Date.now();
}

export function invalidateAdminStorageDriverDescriptors() {
	for (const cache of descriptorCaches.values()) {
		cache.descriptors = null;
		cache.loadedAt = 0;
		cache.pendingRequest = null;
		cache.requestSerial += 1;
	}
}

export async function loadAdminStorageDriverDescriptors(options?: {
	force?: boolean;
	context?: StorageConnectorCatalogContext;
}) {
	const force = options?.force ?? false;
	const context = options?.context ?? DEFAULT_CATALOG_CONTEXT;
	const cache = descriptorCache(context);
	const freshDescriptors = getFreshDescriptorCache(context);
	if (!force && freshDescriptors != null) {
		return freshDescriptors;
	}

	if (!force && cache.pendingRequest != null) {
		return cache.pendingRequest;
	}

	const requestSerial = ++cache.requestSerial;
	const request = adminPolicyService
		.listStorageDriverDescriptors(
			context === DEFAULT_CATALOG_CONTEXT ? undefined : { context },
		)
		.then((descriptors) => {
			if (requestSerial === cache.requestSerial) {
				primeAdminStorageDriverDescriptors(descriptors, context);
			}
			return descriptors;
		})
		.finally(() => {
			if (cache.pendingRequest === request) {
				cache.pendingRequest = null;
			}
		});

	cache.pendingRequest = request;
	return request;
}

export function getStorageDriverDescriptor(
	descriptors: StorageConnectorDescriptor[] | null,
	driverType: DriverType,
) {
	return (
		descriptors?.find((descriptor) => descriptor.driver_type === driverType) ??
		null
	);
}
