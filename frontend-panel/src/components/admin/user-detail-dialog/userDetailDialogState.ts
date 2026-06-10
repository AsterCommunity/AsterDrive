import type { UserInfo } from "@/types/api";

export function userDetailDraftKey(user: UserInfo) {
	return [
		user.id,
		user.email_verified ? "verified" : "unverified",
		user.must_change_password ? "must-change-password" : "password-ok",
		user.policy_group_id ?? "none",
		user.role,
		user.status,
		user.storage_quota ?? 0,
	].join(":");
}
