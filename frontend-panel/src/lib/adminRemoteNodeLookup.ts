import { adminRemoteNodeService } from "@/services/adminService";
import type { RemoteNodeInfo } from "@/types/api";

export const ADMIN_REMOTE_NODE_LOOKUP_LIMIT = 100;
export const ADMIN_REMOTE_NODE_LOOKUP_CACHE_TTL_MS = 30_000;

let cachedRemoteNodes: RemoteNodeInfo[] | null = null;
let cachedRemoteNodesLoadedAt = 0;
let pendingRemoteNodeRequest: Promise<RemoteNodeInfo[]> | null = null;
let remoteNodeLookupRequestSerial = 0;

function getFreshRemoteNodeCache() {
	if (
		cachedRemoteNodes != null &&
		Date.now() - cachedRemoteNodesLoadedAt <
			ADMIN_REMOTE_NODE_LOOKUP_CACHE_TTL_MS
	) {
		return cachedRemoteNodes;
	}
	return null;
}

async function listAllRemoteNodes(pageSize: number) {
	if (!Number.isInteger(pageSize) || pageSize <= 0) {
		throw new Error("pageSize must be a positive integer");
	}

	const allRemoteNodes: RemoteNodeInfo[] = [];
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
		const previousCount = allRemoteNodes.length;
		const page = await adminRemoteNodeService.list({
			limit: pageSize,
			offset,
		});
		allRemoteNodes.push(...page.items);
		total = Number.isFinite(page.total) ? page.total : allRemoteNodes.length;
		maxPages = Math.max(1, Math.ceil(total / pageSize)) + 2;
		offset += page.items.length;
		if (page.items.length === 0) {
			if (allRemoteNodes.length < total) {
				throw new Error("incomplete pages from adminRemoteNodeService.list");
			}
			break;
		}
		if (offset <= previousOffset || allRemoteNodes.length <= previousCount) {
			throw new Error("pagination did not make progress");
		}
	} while (allRemoteNodes.length < total);

	return allRemoteNodes;
}

export function readAdminRemoteNodeLookup() {
	return cachedRemoteNodes;
}

export function primeAdminRemoteNodeLookup(remoteNodes: RemoteNodeInfo[]) {
	cachedRemoteNodes = remoteNodes;
	cachedRemoteNodesLoadedAt = Date.now();
}

export function invalidateAdminRemoteNodeLookup() {
	cachedRemoteNodes = null;
	cachedRemoteNodesLoadedAt = 0;
	pendingRemoteNodeRequest = null;
	remoteNodeLookupRequestSerial += 1;
}

export async function loadAdminRemoteNodeLookup(options?: {
	force?: boolean;
	limit?: number;
}) {
	const force = options?.force ?? false;
	const limit = options?.limit ?? ADMIN_REMOTE_NODE_LOOKUP_LIMIT;

	const freshRemoteNodes = getFreshRemoteNodeCache();
	if (!force && freshRemoteNodes != null) {
		return freshRemoteNodes;
	}

	if (!force && pendingRemoteNodeRequest != null) {
		return pendingRemoteNodeRequest;
	}

	const requestSerial = ++remoteNodeLookupRequestSerial;
	const request = listAllRemoteNodes(limit)
		.then((remoteNodes) => {
			if (requestSerial === remoteNodeLookupRequestSerial) {
				primeAdminRemoteNodeLookup(remoteNodes);
			}
			return remoteNodes;
		})
		.finally(() => {
			if (pendingRemoteNodeRequest === request) {
				pendingRemoteNodeRequest = null;
			}
		});

	pendingRemoteNodeRequest = request;
	return request;
}
