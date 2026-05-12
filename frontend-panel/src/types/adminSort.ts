import type { SortOrder } from "@/lib/pagination";

export type AdminUserSortBy =
	| "id"
	| "username"
	| "email"
	| "role"
	| "status"
	| "storage_used"
	| "storage_quota"
	| "created_at"
	| "updated_at";

export type AdminTeamSortBy =
	| "id"
	| "name"
	| "storage_used"
	| "storage_quota"
	| "created_at"
	| "updated_at"
	| "archived_at";

export type AdminTeamMemberSortBy =
	| "username"
	| "email"
	| "role"
	| "status"
	| "created_at"
	| "updated_at";

export type AdminPolicySortBy =
	| "id"
	| "name"
	| "driver_type"
	| "endpoint"
	| "bucket"
	| "is_default"
	| "created_at"
	| "updated_at";

export type AdminPolicyGroupSortBy =
	| "id"
	| "name"
	| "is_enabled"
	| "is_default"
	| "created_at"
	| "updated_at";

export type AdminRemoteNodeSortBy =
	| "id"
	| "name"
	| "base_url"
	| "is_enabled"
	| "last_checked_at"
	| "created_at"
	| "updated_at";

export type AdminShareSortBy =
	| "id"
	| "token"
	| "user_id"
	| "download_count"
	| "max_downloads"
	| "expires_at"
	| "created_at"
	| "updated_at";

export type AdminTaskSortBy =
	| "id"
	| "display_name"
	| "kind"
	| "status"
	| "progress"
	| "created_at"
	| "updated_at"
	| "started_at"
	| "finished_at";

export type AdminLockSortBy =
	| "id"
	| "path"
	| "entity_type"
	| "owner_id"
	| "timeout_at"
	| "shared"
	| "deep"
	| "created_at";

export type AdminAuditLogSortBy =
	| "id"
	| "created_at"
	| "user_id"
	| "action"
	| "entity_type"
	| "entity_name"
	| "ip_address";

export interface AdminSortParams<SortBy extends string> {
	sort_by?: SortBy;
	sort_order?: SortOrder;
}
