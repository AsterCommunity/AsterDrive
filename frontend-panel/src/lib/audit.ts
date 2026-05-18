import type { TFunction } from "i18next";
import type { AuditAction, AuditEntityType } from "@/types/api";

export const AUDIT_ENTITY_TYPE_FILTER_VALUES = [
	"auth_session",
	"batch",
	"external_auth_identity",
	"external_auth_provider",
	"file",
	"folder",
	"passkey",
	"policy_group",
	"remote_ingress_profile",
	"remote_node",
	"resource_lock",
	"share",
	"storage_policy",
	"stream_ticket",
	"system_config",
	"task",
	"team",
	"trash",
	"upload_session",
	"user",
	"webdav_account",
] as const satisfies readonly AuditEntityType[];

export function isAuditEntityType(value: string): value is AuditEntityType {
	type MissingAuditEntityType = Exclude<
		AuditEntityType,
		(typeof AUDIT_ENTITY_TYPE_FILTER_VALUES)[number]
	>;
	const filterValuesCoverOpenApi: MissingAuditEntityType extends never
		? true
		: never = true;
	return (
		filterValuesCoverOpenApi &&
		AUDIT_ENTITY_TYPE_FILTER_VALUES.includes(value as AuditEntityType)
	);
}

function resolveAuditTranslation(
	t: TFunction,
	key: string,
	ns: "admin" | "settings",
	fallback?: string,
) {
	const translated = t(key, { ns, defaultValue: key });
	return translated === key ? fallback : translated;
}

export function formatAuditAction(t: TFunction, action: AuditAction | string) {
	const value = String(action);
	return (
		resolveAuditTranslation(t, `audit_action_${value}`, "admin") ??
		resolveAuditTranslation(t, value, "settings", value) ??
		value
	);
}

export function getAuditActionBadgeClass(action: AuditAction | string) {
	const value = String(action);
	if (value.includes("delete")) {
		return "border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950/60 dark:text-red-300";
	}
	if (value.includes("upload")) {
		return "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/60 dark:text-emerald-300";
	}
	if (value.includes("share")) {
		return "border-sky-200 bg-sky-50 text-sky-700 dark:border-sky-900 dark:bg-sky-950/60 dark:text-sky-300";
	}
	if (value.includes("login")) {
		return "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900 dark:bg-amber-950/60 dark:text-amber-300";
	}
	return "border-border bg-muted/30 text-muted-foreground";
}

export function formatAuditEntityType(
	t: TFunction,
	entityType: string | null | undefined,
) {
	if (!entityType) {
		return "---";
	}

	return (
		resolveAuditTranslation(
			t,
			`audit_entity_type_${entityType}`,
			"admin",
			entityType,
		) ?? entityType
	);
}
