import { adminPolicyService } from "@/services/adminService";
import type { DriverType, StorageConnectorDescriptor } from "@/types/api";

export const ADMIN_STORAGE_DRIVER_DESCRIPTOR_CACHE_TTL_MS = 30_000;

let cachedDescriptors: StorageConnectorDescriptor[] | null = null;
let cachedDescriptorsLoadedAt = 0;
let pendingDescriptorRequest: Promise<StorageConnectorDescriptor[]> | null =
	null;
let descriptorRequestSerial = 0;

function getFreshDescriptorCache() {
	if (
		cachedDescriptors != null &&
		Date.now() - cachedDescriptorsLoadedAt <
			ADMIN_STORAGE_DRIVER_DESCRIPTOR_CACHE_TTL_MS
	) {
		return cachedDescriptors;
	}
	return null;
}

export function readAdminStorageDriverDescriptors() {
	return cachedDescriptors;
}

export function primeAdminStorageDriverDescriptors(
	descriptors: StorageConnectorDescriptor[],
) {
	cachedDescriptors = descriptors;
	cachedDescriptorsLoadedAt = Date.now();
}

export function invalidateAdminStorageDriverDescriptors() {
	cachedDescriptors = null;
	cachedDescriptorsLoadedAt = 0;
	pendingDescriptorRequest = null;
	descriptorRequestSerial += 1;
}

export async function loadAdminStorageDriverDescriptors(options?: {
	force?: boolean;
}) {
	const force = options?.force ?? false;
	const freshDescriptors = getFreshDescriptorCache();
	if (!force && freshDescriptors != null) {
		return freshDescriptors;
	}

	if (!force && pendingDescriptorRequest != null) {
		return pendingDescriptorRequest;
	}

	const requestSerial = ++descriptorRequestSerial;
	const request = adminPolicyService
		.listStorageDriverDescriptors()
		.then((descriptors) => {
			if (requestSerial === descriptorRequestSerial) {
				primeAdminStorageDriverDescriptors(descriptors);
			}
			return descriptors;
		})
		.finally(() => {
			if (pendingDescriptorRequest === request) {
				pendingDescriptorRequest = null;
			}
		});

	pendingDescriptorRequest = request;
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
