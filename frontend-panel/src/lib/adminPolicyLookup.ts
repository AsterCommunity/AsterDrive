import { adminPolicyService } from "@/services/adminService";
import type { StoragePolicy } from "@/types/api";

export const ADMIN_POLICY_LOOKUP_LIMIT = 100;
export const ADMIN_POLICY_LOOKUP_CACHE_TTL_MS = 30_000;

let cachedPolicies: StoragePolicy[] | null = null;
let cachedPoliciesLoadedAt = 0;
let pendingPolicyRequest: Promise<StoragePolicy[]> | null = null;
let policyLookupRequestSerial = 0;

function getFreshPolicyCache() {
	if (
		cachedPolicies != null &&
		Date.now() - cachedPoliciesLoadedAt < ADMIN_POLICY_LOOKUP_CACHE_TTL_MS
	) {
		return cachedPolicies;
	}
	return null;
}

async function listAllPolicies(pageSize: number) {
	if (!Number.isInteger(pageSize) || pageSize <= 0) {
		throw new Error("pageSize must be a positive integer");
	}

	const allPolicies: StoragePolicy[] = [];
	let offset = 0;
	let total = 0;
	let pageCount = 0;
	let maxPages = Number.POSITIVE_INFINITY;

	do {
		pageCount += 1;
		if (pageCount > maxPages) {
			throw new Error("pagination exceeded max iterations");
		}

		const previousOffset = offset;
		const previousCount = allPolicies.length;
		const page = await adminPolicyService.list({
			limit: pageSize,
			offset,
		});
		allPolicies.push(...page.items);
		total = Number.isFinite(page.total) ? page.total : allPolicies.length;
		maxPages = Math.max(1, Math.ceil(total / pageSize)) + 2;
		offset += page.items.length;
		if (page.items.length === 0) {
			if (allPolicies.length < total) {
				throw new Error("incomplete pages from adminPolicyService.list");
			}
			break;
		}
		if (offset <= previousOffset || allPolicies.length <= previousCount) {
			throw new Error("pagination did not make progress");
		}
	} while (allPolicies.length < total);

	return allPolicies;
}

export function readAdminPolicyLookup() {
	return cachedPolicies;
}

export function primeAdminPolicyLookup(policies: StoragePolicy[]) {
	cachedPolicies = policies;
	cachedPoliciesLoadedAt = Date.now();
}

export function invalidateAdminPolicyLookup() {
	cachedPolicies = null;
	cachedPoliciesLoadedAt = 0;
	pendingPolicyRequest = null;
	policyLookupRequestSerial += 1;
}

export async function loadAdminPolicyLookup(options?: {
	force?: boolean;
	limit?: number;
}) {
	const force = options?.force ?? false;
	const limit = options?.limit ?? ADMIN_POLICY_LOOKUP_LIMIT;

	const freshPolicies = getFreshPolicyCache();
	if (!force && freshPolicies != null) {
		return freshPolicies;
	}

	if (!force && pendingPolicyRequest != null) {
		return pendingPolicyRequest;
	}

	const requestSerial = ++policyLookupRequestSerial;
	const request = listAllPolicies(limit)
		.then((policies) => {
			if (requestSerial === policyLookupRequestSerial) {
				primeAdminPolicyLookup(policies);
			}
			return policies;
		})
		.finally(() => {
			if (pendingPolicyRequest === request) {
				pendingPolicyRequest = null;
			}
		});

	pendingPolicyRequest = request;
	return request;
}
