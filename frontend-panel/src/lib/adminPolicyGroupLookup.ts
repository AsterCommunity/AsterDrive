import { adminPolicyGroupService } from "@/services/adminService";
import type { StoragePolicyGroup } from "@/types/api";

export const ADMIN_POLICY_GROUP_LOOKUP_LIMIT = 100;
export const ADMIN_POLICY_GROUP_LOOKUP_CACHE_TTL_MS = 30_000;

let cachedPolicyGroups: StoragePolicyGroup[] | null = null;
let cachedPolicyGroupsLoadedAt = 0;
let pendingPolicyGroupRequest: Promise<StoragePolicyGroup[]> | null = null;
let policyGroupLookupRequestSerial = 0;

function getFreshPolicyGroupCache() {
	if (
		cachedPolicyGroups != null &&
		Date.now() - cachedPolicyGroupsLoadedAt <
			ADMIN_POLICY_GROUP_LOOKUP_CACHE_TTL_MS
	) {
		return cachedPolicyGroups;
	}
	return null;
}

export function readAdminPolicyGroupLookup() {
	return cachedPolicyGroups;
}

export function primeAdminPolicyGroupLookup(
	policyGroups: StoragePolicyGroup[],
) {
	cachedPolicyGroups = policyGroups;
	cachedPolicyGroupsLoadedAt = Date.now();
}

export function invalidateAdminPolicyGroupLookup() {
	cachedPolicyGroups = null;
	cachedPolicyGroupsLoadedAt = 0;
	pendingPolicyGroupRequest = null;
	policyGroupLookupRequestSerial += 1;
}

export async function loadAdminPolicyGroupLookup(options?: {
	force?: boolean;
	limit?: number;
}) {
	const force = options?.force ?? false;
	const limit = options?.limit ?? ADMIN_POLICY_GROUP_LOOKUP_LIMIT;

	const freshPolicyGroups = getFreshPolicyGroupCache();
	if (!force && freshPolicyGroups != null) {
		return freshPolicyGroups;
	}

	if (!force && pendingPolicyGroupRequest != null) {
		return pendingPolicyGroupRequest;
	}

	const requestSerial = ++policyGroupLookupRequestSerial;
	const request = adminPolicyGroupService
		.listAll(limit)
		.then((policyGroups) => {
			if (requestSerial === policyGroupLookupRequestSerial) {
				primeAdminPolicyGroupLookup(policyGroups);
			}
			return policyGroups;
		})
		.finally(() => {
			if (pendingPolicyGroupRequest === request) {
				pendingPolicyGroupRequest = null;
			}
		});

	pendingPolicyGroupRequest = request;
	return request;
}
