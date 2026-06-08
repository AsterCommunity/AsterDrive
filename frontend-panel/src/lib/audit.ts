import type { TFunction } from "i18next";
import type {
	AuditAction,
	AuditEntityType,
	AuditLogEntry,
	AuditPresentationMessage,
	TeamAuditEntryInfo,
} from "@/types/api";

export const AUDIT_ENTITY_TYPE_FILTER_VALUES = [
	"auth_session",
	"batch",
	"external_auth_identity",
	"external_auth_provider",
	"file",
	"folder",
	"invitation",
	"mail",
	"mfa_factor",
	"passkey",
	"policy_group",
	"remote_ingress_profile",
	"remote_node",
	"resource_lock",
	"share",
	"storage_policy",
	"stream_ticket",
	"system_config",
	"tag",
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
	// Follower audit rows normally live in a follower node's own database rather
	// than the primary admin audit feed. Keep translations here so direct follower
	// views or future aggregation do not expose raw action names.
	return (
		resolveAuditTranslation(t, `audit_action_${value}`, "admin") ??
		resolveAuditTranslation(t, value, "settings", value) ??
		value
	);
}

type AuditActionTone = "danger" | "success" | "info" | "warning";

const AUDIT_ACTION_TONES = {
	admin_delete_config: "danger",
	admin_delete_external_auth_provider: "danger",
	admin_delete_policy: "danger",
	admin_delete_policy_group: "danger",
	admin_delete_remote_ingress_profile: "danger",
	admin_delete_remote_node: "danger",
	admin_delete_share: "danger",
	admin_force_delete_user: "danger",
	batch_delete: "danger",
	file_delete: "danger",
	file_purge: "danger",
	file_version_delete: "danger",
	folder_delete: "danger",
	folder_purge: "danger",
	mail_delivery_failed: "danger",
	property_delete: "danger",
	share_batch_delete: "danger",
	share_delete: "danger",
	user_passkey_delete: "danger",
	webdav_account_delete: "danger",

	archive_download: "success",
	file_download: "success",
	file_upload: "success",
	mail_send: "success",
	user_upload_avatar: "success",

	file_direct_link_create: "info",
	file_preview_link_create: "info",
	share_create: "info",
	share_update: "info",

	user_external_auth_login: "warning",
	user_login: "warning",
	user_passkey_login: "warning",
	user_refresh_token_reuse_detected: "warning",
} as const satisfies Partial<Record<AuditAction, AuditActionTone>>;

const AUDIT_ACTION_TONE_CLASSES = {
	danger:
		"border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950/60 dark:text-red-300",
	info: "border-sky-200 bg-sky-50 text-sky-700 dark:border-sky-900 dark:bg-sky-950/60 dark:text-sky-300",
	success:
		"border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/60 dark:text-emerald-300",
	warning:
		"border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900 dark:bg-amber-950/60 dark:text-amber-300",
} as const satisfies Record<AuditActionTone, string>;

export function getAuditActionBadgeClass(action: AuditAction | string) {
	const tone =
		typeof action === "string" &&
		Object.hasOwn(AUDIT_ACTION_TONES, action) &&
		AUDIT_ACTION_TONES[action as keyof typeof AUDIT_ACTION_TONES];
	if (tone) {
		return AUDIT_ACTION_TONE_CLASSES[tone];
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

type AuditPresentationEntry = Pick<
	AuditLogEntry | TeamAuditEntryInfo,
	"action" | "presentation"
>;

function presentationParams(
	message: AuditPresentationMessage | undefined | null,
) {
	return message?.params &&
		typeof message.params === "object" &&
		!Array.isArray(message.params)
		? (message.params as Record<string, unknown>)
		: {};
}

function stringParam(
	params: Record<string, unknown>,
	key: string,
): string | undefined {
	const value = params[key];
	if (typeof value === "string") {
		return value;
	}
	if (typeof value === "number" || typeof value === "boolean") {
		return String(value);
	}
	return undefined;
}

function formatAuditPresentationMessage(
	t: TFunction,
	message: AuditPresentationMessage | undefined | null,
	fallback?: string,
) {
	if (!message?.code) {
		return fallback;
	}

	const params = presentationParams(message);
	const direct = resolveAuditTranslation(
		t,
		`audit_presentation_${message.code}`,
		"admin",
	);
	if (direct) {
		return t(`audit_presentation_${message.code}`, {
			ns: "admin",
			defaultValue: direct,
			...params,
		});
	}

	if (message.code.startsWith("audit_action_")) {
		// Old backend responses may already use an action code here; keep that path working.
		return resolveAuditTranslation(t, message.code, "admin", fallback);
	}

	const actionLabel = resolveAuditTranslation(
		t,
		`audit_action_${message.code}`,
		"admin",
	);
	if (actionLabel) {
		return actionLabel;
	}

	const entityLabel = resolveAuditTranslation(
		t,
		`audit_entity_type_${message.code}`,
		"admin",
	);
	if (entityLabel) {
		const name = stringParam(params, "name");
		return name ? `${name} · ${entityLabel}` : entityLabel;
	}

	return fallback;
}

export function formatAuditSummary(
	t: TFunction,
	entry: AuditPresentationEntry,
) {
	return (
		formatAuditPresentationMessage(t, entry.presentation?.summary) ??
		formatAuditAction(t, entry.action)
	);
}

export function formatAuditTarget(
	t: TFunction,
	entry: Pick<AuditLogEntry, "entity_name" | "entity_type" | "presentation">,
) {
	return (
		formatAuditPresentationMessage(t, entry.presentation?.target) ??
		entry.entity_name ??
		formatAuditEntityType(t, entry.entity_type)
	);
}

export function formatAuditTargetType(
	t: TFunction,
	entry: Pick<AuditLogEntry, "entity_type" | "presentation">,
) {
	const targetCode = entry.presentation?.target?.code;
	if (targetCode) {
		// A structured target type is only trustworthy when it resolves to a known entity type.
		const translatedTargetType = resolveAuditTranslation(
			t,
			`audit_entity_type_${targetCode}`,
			"admin",
		);
		if (translatedTargetType) {
			return translatedTargetType;
		}
	}
	return formatAuditEntityType(t, entry.entity_type);
}

export function formatAuditDetail(t: TFunction, entry: AuditPresentationEntry) {
	return formatAuditPresentationMessage(t, entry.presentation?.detail);
}
