import { adminPolicyService, adminUserService } from "@/services/adminService";
import type { StoragePolicy, UserInfo } from "@/types/api";

export type AdminLookupKind = "users" | "policies";
export type AdminLookupOption = {
	id: number;
	label: string;
	description: string;
};

function userOption(user: UserInfo): AdminLookupOption {
	return { id: user.id, label: user.username, description: user.email };
}
function policyOption(policy: StoragePolicy): AdminLookupOption {
	return { id: policy.id, label: policy.name, description: `#${policy.id}` };
}

// HTTP adapter; the admin route owns the bounded, short-lived lookup cache.
export const adminLookupService = {
	async search(
		kind: AdminLookupKind,
		keyword: string,
		offset: number,
		signal: AbortSignal,
	) {
		const params = {
			keyword: keyword.trim() || undefined,
			offset,
			limit: 20,
			sort_by: "id" as const,
			sort_order: "asc" as const,
		};
		if (kind === "users") {
			const page = await adminUserService.list(params, { signal });
			return { items: page.items.map(userOption), total: page.total };
		}
		const page = await adminPolicyService.list(params, { signal });
		return { items: page.items.map(policyOption), total: page.total };
	},
	async get(kind: AdminLookupKind, id: number, signal: AbortSignal) {
		return kind === "users"
			? userOption(await adminUserService.get(id, { signal }))
			: policyOption(await adminPolicyService.get(id, { signal }));
	},
};
