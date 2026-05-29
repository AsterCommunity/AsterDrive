import { formatDateAbsolute, formatNumber } from "@/lib/format";
import type {
	BackgroundTaskKind,
	BackgroundTaskStatus,
	StoragePolicyMigrationTaskResult,
	TaskInfo,
	TaskStepInfo,
	TaskStepStatus,
} from "@/types/api";

export const ACTIVE_TASK_STATUSES = new Set<BackgroundTaskStatus>([
	"pending",
	"processing",
	"retry",
]);

type TaskTranslate = (
	key: string,
	values?: Record<string, number | string>,
) => string;

const RUNTIME_TASK_NAME_KEYS: Record<string, string> = {
	"audit-cleanup": "runtime_task_audit_cleanup",
	"audit-log-cleanup": "runtime_task_audit_cleanup",
	"auth-session-cleanup": "runtime_task_auth_session_cleanup",
	"background-task-dispatch": "runtime_task_background_task_dispatch",
	"blob-reconcile": "runtime_task_blob_reconcile",
	"completed-upload-cleanup": "runtime_task_completed_upload_cleanup",
	"external-auth-flow-cleanup": "runtime_task_external_auth_flow_cleanup",
	"lock-cleanup": "runtime_task_lock_cleanup",
	"mail-outbox-dispatch": "runtime_task_mail_outbox_dispatch",
	"mfa-flow-cleanup": "runtime_task_mfa_flow_cleanup",
	"remote-node-health-test": "runtime_task_remote_node_health_test",
	"system-health-check": "runtime_task_system_health_check",
	"task-cleanup": "runtime_task_task_cleanup",
	"team-archive-cleanup": "runtime_task_team_archive_cleanup",
	"trash-cleanup": "runtime_task_trash_cleanup",
	"upload-cleanup": "runtime_task_upload_cleanup",
	"wopi-session-cleanup": "runtime_task_wopi_session_cleanup",
};

const RUNTIME_DISPLAY_NAMES_TO_TASK_NAMES: Record<string, string> = {
	"Audit log cleanup": "audit-cleanup",
	"Background task dispatch": "background-task-dispatch",
	"Blob reconcile": "blob-reconcile",
	"Completed upload cleanup": "completed-upload-cleanup",
	"External auth flow cleanup": "external-auth-flow-cleanup",
	"Lock cleanup": "lock-cleanup",
	"Mail outbox dispatch": "mail-outbox-dispatch",
	"Remote node health test": "remote-node-health-test",
	"System health check": "system-health-check",
	"Task artifact cleanup": "task-cleanup",
	"Team archive cleanup": "team-archive-cleanup",
	"Trash cleanup": "trash-cleanup",
	"Upload cleanup": "upload-cleanup",
	"WOPI session cleanup": "wopi-session-cleanup",
	"auth session cleanup": "auth-session-cleanup",
	"mfa flow cleanup": "mfa-flow-cleanup",
};

const KNOWN_STATUS_TEXT_KEYS: Record<string, string> = {
	"Archive preview ready": "status_text_archive_preview_ready",
	"Auth session cleanup failed": "status_text_auth_session_cleanup_failed",
	"Background task dispatch failed":
		"status_text_background_task_dispatch_failed",
	"Blob maintenance finished": "status_text_blob_maintenance_finished",
	"Blob reconcile failed": "status_text_blob_reconcile_failed",
	"Completed upload cleanup failed":
		"status_text_completed_upload_cleanup_failed",
	"External auth flow cleanup failed":
		"status_text_external_auth_flow_cleanup_failed",
	"Lock cleanup failed": "status_text_lock_cleanup_failed",
	"MFA flow cleanup failed": "status_text_mfa_flow_cleanup_failed",
	"Mail outbox dispatch failed": "status_text_mail_outbox_dispatch_failed",
	"Media metadata failed": "status_text_media_metadata_failed",
	"Media metadata ready": "status_text_media_metadata_ready",
	"Media metadata unsupported": "status_text_media_metadata_unsupported",
	"Migration completed": "status_text_storage_migration_completed",
	"Storage migration completed": "status_text_storage_migration_completed",
	"Task artifact cleanup failed": "status_text_task_cleanup_failed",
	"Task panicked": "status_text_task_panicked",
	"Team archive cleanup failed": "status_text_team_archive_cleanup_failed",
	"Temporary upload cleanup finished":
		"status_text_temporary_upload_cleanup_finished",
	"Thumbnail already available": "status_text_thumbnail_already_available",
	"Thumbnail ready": "status_text_thumbnail_ready",
	"Trash cleanup failed": "status_text_trash_cleanup_failed",
	"Trash purged": "status_text_trash_purged",
	"Upload cleanup failed": "status_text_upload_cleanup_failed",
	"WOPI session cleanup failed": "status_text_wopi_session_cleanup_failed",
	"Waiting for presigned URLs to expire":
		"status_text_waiting_presigned_url_expiry",
	"system healthy": "status_text_system_healthy",
};

function translateWithFallback(
	t: TaskTranslate,
	key: string,
	values: Record<string, number | string> | undefined,
	fallback: string,
) {
	const translated = t(key, values);
	return translated === key ? fallback : translated;
}

function translateFirstWithFallback(
	t: TaskTranslate,
	keys: string[],
	values: Record<string, number | string> | undefined,
	fallback: string,
) {
	for (const key of keys) {
		const translated = t(key, values);
		if (translated !== key) {
			return translated;
		}
	}
	return fallback;
}

function fallbackTitleFromId(prefix: string, id: number) {
	return `${prefix} #${id}`;
}

function formatBlobLabel(t: TaskTranslate, blobId: number) {
	return translateWithFallback(
		t,
		"tasks:task_name_blob_id",
		{ id: blobId },
		fallbackTitleFromId("Blob", blobId),
	);
}

function formatPolicyLabel(t: TaskTranslate, policyId: number) {
	return translateWithFallback(
		t,
		"tasks:summary_policy_id",
		{ id: policyId },
		fallbackTitleFromId("Policy", policyId),
	);
}

function formatFileOrBlobLabel(
	t: TaskTranslate,
	fileName: string | null | undefined,
	blobId: number,
) {
	const trimmed = fileName?.trim();
	return trimmed ? trimmed : formatBlobLabel(t, blobId);
}

const MEDIA_PROCESSOR_KEY_ALIASES: Record<string, string> = {
	asterdrive_built_in: "images",
	asterdrive_built_in_audio: "lofty",
	native: "storage_native",
	storage_native_processor: "storage_native",
};

function normalizeMediaProcessorKey(processor: string) {
	return processor
		.trim()
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "_")
		.replace(/^_+|_+$/g, "");
}

function formatMediaProcessorName(
	t: TaskTranslate,
	processor: string,
	fallback = processor.replaceAll("_", " "),
) {
	const normalized = normalizeMediaProcessorKey(processor);
	const candidates = [
		processor,
		normalized,
		MEDIA_PROCESSOR_KEY_ALIASES[normalized],
		normalized ? `storage_${normalized}` : undefined,
	].filter((candidate): candidate is string => Boolean(candidate));
	const keys = [...new Set(candidates)].map(
		(candidate) => `tasks:media_processor_${candidate}`,
	);
	return translateFirstWithFallback(t, keys, undefined, fallback);
}

function formatMediaMetadataKind(t: TaskTranslate, kind: string | undefined) {
	if (!kind) {
		return translateWithFallback(
			t,
			"tasks:media_metadata_kind_generic",
			undefined,
			"media",
		);
	}
	const key = `tasks:media_metadata_kind_${kind}`;
	return translateWithFallback(t, key, undefined, kind);
}

export function formatRuntimeTaskName(
	t: TaskTranslate,
	taskName: string,
	fallback = taskName.replaceAll("-", " "),
) {
	const key = RUNTIME_TASK_NAME_KEYS[taskName];
	if (!key) {
		return fallback;
	}
	return translateWithFallback(t, `tasks:${key}`, undefined, fallback);
}

function formatRuntimeDisplayNameFallback(
	t: TaskTranslate,
	displayName: string,
) {
	const taskName = RUNTIME_DISPLAY_NAMES_TO_TASK_NAMES[displayName];
	if (!taskName) {
		return displayName;
	}
	const key = RUNTIME_TASK_NAME_KEYS[taskName];
	if (!key) {
		return displayName;
	}
	return translateWithFallback(t, `tasks:${key}`, undefined, displayName);
}

function formatBlobMaintenanceScopeFromRaw(t: TaskTranslate, rawScope: string) {
	if (rawScope === "all blobs") {
		return translateWithFallback(
			t,
			"tasks:blob_maintenance_scope_all",
			undefined,
			rawScope,
		);
	}
	const selectedMatch = rawScope.match(/^(\d+) blob\(s\)$/);
	if (selectedMatch) {
		const count = Number(selectedMatch[1]);
		return translateWithFallback(
			t,
			"tasks:blob_maintenance_scope_selected",
			{ count },
			rawScope,
		);
	}
	return rawScope;
}

export function formatTaskDisplayNameFromRaw(
	t: TaskTranslate,
	kind: BackgroundTaskKind,
	displayName: string,
) {
	switch (kind) {
		case "archive_extract": {
			const match = displayName.match(/^Extract (.+)$/);
			if (match) {
				return translateWithFallback(
					t,
					"tasks:task_name_archive_extract",
					{ name: match[1] },
					displayName,
				);
			}
			break;
		}
		case "archive_compress": {
			const match = displayName.match(/^Compress (.+)$/);
			if (match) {
				return translateWithFallback(
					t,
					"tasks:task_name_archive_compress",
					{ name: match[1] },
					displayName,
				);
			}
			break;
		}
		case "archive_preview_generate": {
			const match = displayName.match(
				/^Generate archive preview for file #(\d+) blob #(\d+)(?:\s+.*)?$/,
			);
			if (match) {
				return translateWithFallback(
					t,
					"tasks:task_name_archive_preview_generate_file_id",
					{ blobId: Number(match[2]), fileId: Number(match[1]) },
					displayName,
				);
			}
			break;
		}
		case "thumbnail_generate": {
			const match = displayName.match(
				/^Generate thumbnail for blob #(\d+) via (.+)$/,
			);
			if (match) {
				return translateWithFallback(
					t,
					"tasks:task_name_thumbnail_generate_blob_with_processor",
					{
						blob: formatBlobLabel(t, Number(match[1])),
						processor: formatMediaProcessorName(t, match[2], match[2]),
					},
					displayName,
				);
			}
			break;
		}
		case "media_metadata_extract": {
			const match = displayName.match(
				/^Extract (.+) metadata for blob #(\d+)$/,
			);
			if (match) {
				return translateWithFallback(
					t,
					"tasks:task_name_media_metadata_extract_blob",
					{
						blob: formatBlobLabel(t, Number(match[2])),
						kind: formatMediaMetadataKind(t, match[1]),
					},
					displayName,
				);
			}
			break;
		}
		case "trash_purge_all":
			if (displayName === "Empty trash") {
				return translateWithFallback(
					t,
					"tasks:task_name_trash_purge_all",
					undefined,
					displayName,
				);
			}
			break;
		case "storage_policy_temp_cleanup": {
			const match = displayName.match(
				/^Clean (?:deleted )?storage policy #(\d+) temporary uploads$/,
			);
			if (match) {
				return translateWithFallback(
					t,
					"tasks:task_name_storage_policy_temp_cleanup_policy_id",
					{ policy: formatPolicyLabel(t, Number(match[1])) },
					displayName,
				);
			}
			break;
		}
		case "storage_policy_migration": {
			const match = displayName.match(
				/^Migrate storage policy #(\d+) to #(\d+)$/,
			);
			if (match) {
				return translateWithFallback(
					t,
					"tasks:task_name_storage_policy_migration",
					{
						source: formatPolicyLabel(t, Number(match[1])),
						target: formatPolicyLabel(t, Number(match[2])),
					},
					displayName,
				);
			}
			break;
		}
		case "blob_maintenance": {
			const integrityMatch = displayName.match(/^Check integrity for (.+)$/);
			if (integrityMatch) {
				return translateWithFallback(
					t,
					"tasks:blob_maintenance_integrity_check_name",
					{ scope: formatBlobMaintenanceScopeFromRaw(t, integrityMatch[1]) },
					displayName,
				);
			}
			const reconcileMatch = displayName.match(
				/^Reconcile references for (.+)$/,
			);
			if (reconcileMatch) {
				return translateWithFallback(
					t,
					"tasks:blob_maintenance_ref_count_reconcile_name",
					{ scope: formatBlobMaintenanceScopeFromRaw(t, reconcileMatch[1]) },
					displayName,
				);
			}
			const cleanupMatch = displayName.match(/^Clean orphan blobs for (.+)$/);
			if (cleanupMatch) {
				return translateWithFallback(
					t,
					"tasks:blob_maintenance_orphan_cleanup_name",
					{ scope: formatBlobMaintenanceScopeFromRaw(t, cleanupMatch[1]) },
					displayName,
				);
			}
			break;
		}
		case "system_runtime":
			return formatRuntimeDisplayNameFallback(t, displayName);
	}
	return displayName;
}

function formatHealthComponentName(t: TaskTranslate, name: string) {
	const key = `tasks:runtime_health_component_${name}`;
	return translateWithFallback(t, key, undefined, name.replaceAll("_", " "));
}

function formatHealthStatus(t: TaskTranslate, status: string) {
	const key = `tasks:runtime_health_status_${status}`;
	return translateWithFallback(t, key, undefined, status);
}

function formatHealthComponentStatus(
	t: TaskTranslate,
	name: string,
	status: string,
) {
	const component = formatHealthComponentName(t, name);
	const statusLabel = formatHealthStatus(t, status);
	return translateWithFallback(
		t,
		"tasks:runtime_health_component_status",
		{ component, status: statusLabel },
		`${component} ${statusLabel}`,
	);
}

function formatRuntimeSystemHealthDetail(
	t: TaskTranslate,
	health: NonNullable<
		Extract<TaskInfo["result"], { kind: "system_runtime" }>["system_health"]
	>,
) {
	if (health.status === "healthy") {
		return translateWithFallback(
			t,
			"tasks:status_text_system_healthy",
			undefined,
			"system healthy",
		);
	}
	const issueComponents = health.components.filter(
		(component) => component.status !== "healthy",
	);
	const components =
		issueComponents.length > 0
			? issueComponents
					.map((component) =>
						formatHealthComponentStatus(t, component.name, component.status),
					)
					.join(", ")
			: formatHealthStatus(t, health.status);
	return translateWithFallback(
		t,
		"tasks:runtime_system_health_issue_detail",
		{ components },
		components,
	);
}

function formatKnownStatusText(t: TaskTranslate, text: string) {
	const key = KNOWN_STATUS_TEXT_KEYS[text];
	if (key) {
		return translateWithFallback(t, `tasks:${key}`, undefined, text);
	}

	let match = text.match(/^deleted (\d+) completed sessions \((\d+) broken\)$/);
	if (match) {
		return translateWithFallback(
			t,
			"tasks:status_text_deleted_completed_upload_sessions",
			{ broken: Number(match[2]), count: Number(match[1]) },
			text,
		);
	}

	match = text.match(/^fixed (\d+) ref counts, deleted (\d+) orphan blobs$/);
	if (match) {
		return translateWithFallback(
			t,
			"tasks:status_text_blob_reconcile_finished",
			{ deleted: Number(match[2]), fixed: Number(match[1]) },
			text,
		);
	}

	const cleanupMatch = text.match(
		/^cleaned up (\d+) expired (upload sessions|trash entries|archived teams|locks|auth sessions|external auth flows|MFA flows|audit log entries|task artifacts|WOPI sessions)$/,
	);
	if (cleanupMatch) {
		const cleanupKeyByTarget: Record<string, string> = {
			"MFA flows": "status_text_cleaned_expired_mfa_flows",
			"WOPI sessions": "status_text_cleaned_expired_wopi_sessions",
			"archived teams": "status_text_cleaned_expired_archived_teams",
			"audit log entries": "status_text_cleaned_expired_audit_logs",
			"auth sessions": "status_text_cleaned_expired_auth_sessions",
			"external auth flows": "status_text_cleaned_expired_external_auth_flows",
			locks: "status_text_cleaned_expired_locks",
			"task artifacts": "status_text_cleaned_expired_task_artifacts",
			"trash entries": "status_text_cleaned_expired_trash_entries",
			"upload sessions": "status_text_cleaned_expired_upload_sessions",
		};
		const cleanupKey = cleanupKeyByTarget[cleanupMatch[2]];
		if (cleanupKey) {
			return translateWithFallback(
				t,
				`tasks:${cleanupKey}`,
				{ count: Number(cleanupMatch[1]) },
				text,
			);
		}
	}

	const healthMatch = text.match(
		/^([a-z_]+)=(healthy|degraded|unhealthy):(.*)$/,
	);
	if (healthMatch) {
		const componentStatus = formatHealthComponentStatus(
			t,
			healthMatch[1],
			healthMatch[2],
		);
		const detail = healthMatch[3]?.trim();
		const components = detail
			? `${componentStatus}: ${detail}`
			: componentStatus;
		return translateWithFallback(
			t,
			"tasks:runtime_system_health_issue_detail",
			{ components },
			text,
		);
	}

	return null;
}

export function formatTaskStatusText(
	t: TaskTranslate,
	text: string | null | undefined,
	task?: Pick<TaskInfo, "kind" | "result">,
) {
	const trimmed = text?.trim();
	if (!trimmed) {
		return null;
	}
	if (
		task?.kind === "system_runtime" &&
		task.result?.kind === "system_runtime" &&
		task.result.system_health
	) {
		return formatRuntimeSystemHealthDetail(t, task.result.system_health);
	}
	return formatKnownStatusText(t, trimmed) ?? trimmed;
}

export function formatTaskDetail(
	t: TaskTranslate,
	task: TaskInfo,
	emptyFallback = "-",
) {
	return (
		formatTaskStatusText(t, task.last_error ?? task.status_text, task) ??
		emptyFallback
	);
}

export function statusBadgeVariant(status: BackgroundTaskStatus) {
	switch (status) {
		case "pending":
		case "processing":
		case "retry":
			return "secondary";
		case "succeeded":
			return "default";
		case "failed":
			return "destructive";
		case "canceled":
			return "outline";
	}
}

export function taskMetaTextClass(status: BackgroundTaskStatus) {
	switch (status) {
		case "processing":
		case "retry":
			return "text-primary";
		case "succeeded":
			return "text-foreground";
		case "failed":
			return "text-destructive";
		case "pending":
		case "canceled":
			return "text-muted-foreground";
	}
}

export function stepStatusTextClass(status: TaskStepStatus) {
	switch (status) {
		case "active":
			return "text-primary";
		case "succeeded":
			return "text-foreground";
		case "failed":
			return "text-destructive";
		case "skipped":
		case "canceled":
		case "pending":
			return "text-muted-foreground";
	}
}

export function stepProgressPercent(step: TaskStepInfo) {
	if (step.progress_total <= 0) {
		return step.status === "succeeded" ? 100 : 0;
	}
	return Math.max(
		0,
		Math.min(
			100,
			Math.floor((step.progress_current * 100) / step.progress_total),
		),
	);
}

export function stepConnectorClass(status: TaskStepStatus) {
	switch (status) {
		case "succeeded":
			return "bg-primary/70";
		case "active":
			return "bg-primary/35";
		case "failed":
			return "bg-destructive/35";
		case "skipped":
			return "bg-border/40";
		case "canceled":
			return "bg-border/60";
		case "pending":
			return "bg-border/40";
	}
}

export function stepCircleClass(status: TaskStepStatus) {
	switch (status) {
		case "active":
			return "border-primary bg-primary text-primary-foreground ring-4 ring-primary/15";
		case "succeeded":
			return "border-primary/40 bg-primary/12 text-foreground";
		case "failed":
			return "border-destructive/50 bg-destructive/10 text-destructive";
		case "skipped":
			return "border-border/60 bg-muted/20 text-muted-foreground";
		case "canceled":
			return "border-border/70 bg-muted/35 text-muted-foreground";
		case "pending":
			return "border-border/60 bg-background/90 text-muted-foreground";
	}
}

export function stepCircleLabel(index: number, status: TaskStepStatus) {
	switch (status) {
		case "failed":
			return "!";
		case "skipped":
			return String(index + 1);
		case "canceled":
			return "X";
		default:
			return String(index + 1);
	}
}

export function currentTaskStep(task: TaskInfo) {
	return (
		task.steps.find((step) => step.status === "active") ??
		task.steps.find((step) => step.status === "failed") ??
		task.steps[task.steps.length - 1] ??
		null
	);
}

export function formatTaskStatus(
	t: TaskTranslate,
	status: BackgroundTaskStatus,
) {
	switch (status) {
		case "pending":
			return t("tasks:status_pending");
		case "processing":
			return t("tasks:status_processing");
		case "retry":
			return t("tasks:status_retry");
		case "succeeded":
			return t("tasks:status_succeeded");
		case "failed":
			return t("tasks:status_failed");
		case "canceled":
			return t("tasks:status_canceled");
	}
}

export function formatTaskKind(t: TaskTranslate, kind: BackgroundTaskKind) {
	switch (kind) {
		case "archive_extract":
			return t("tasks:kind_archive_extract");
		case "archive_compress":
			return t("tasks:kind_archive_compress");
		case "archive_preview_generate":
			return t("tasks:kind_archive_preview_generate");
		case "thumbnail_generate":
			return t("tasks:kind_thumbnail_generate");
		case "media_metadata_extract":
			return t("tasks:kind_media_metadata_extract");
		case "trash_purge_all":
			return t("tasks:kind_trash_purge_all");
		case "storage_policy_temp_cleanup":
			return t("tasks:kind_storage_policy_temp_cleanup");
		case "storage_policy_migration":
			return t("tasks:kind_storage_policy_migration");
		case "blob_maintenance":
			return t("tasks:kind_blob_maintenance");
		case "system_runtime":
			return t("tasks:kind_system_runtime");
		default:
			return String(kind).replaceAll("_", " ");
	}
}

export function formatTaskDisplayName(t: TaskTranslate, task: TaskInfo) {
	switch (task.payload.kind) {
		case "archive_extract":
			return translateWithFallback(
				t,
				"tasks:task_name_archive_extract",
				{ name: task.payload.source_file_name },
				task.display_name,
			);
		case "archive_compress":
			return translateWithFallback(
				t,
				"tasks:task_name_archive_compress",
				{ name: task.payload.archive_name },
				task.display_name,
			);
		case "archive_preview_generate":
			return translateWithFallback(
				t,
				"tasks:task_name_archive_preview_generate",
				{ name: task.payload.source_file_name },
				task.display_name,
			);
		case "thumbnail_generate": {
			const source = formatFileOrBlobLabel(
				t,
				task.payload.source_file_name,
				task.payload.blob_id,
			);
			return translateWithFallback(
				t,
				"tasks:task_name_thumbnail_generate",
				{
					processor: formatMediaProcessorName(t, task.payload.processor),
					source,
				},
				task.display_name,
			);
		}
		case "media_metadata_extract": {
			const source = formatFileOrBlobLabel(
				t,
				task.payload.source_file_name,
				task.payload.blob_id,
			);
			return translateWithFallback(
				t,
				"tasks:task_name_media_metadata_extract_blob",
				{
					blob: source,
					kind: formatMediaMetadataKind(t, task.payload.media_kind),
				},
				task.display_name,
			);
		}
		case "trash_purge_all":
			return translateWithFallback(
				t,
				"tasks:task_name_trash_purge_all",
				undefined,
				task.display_name,
			);
		case "storage_policy_temp_cleanup":
			return translateWithFallback(
				t,
				"tasks:task_name_storage_policy_temp_cleanup",
				{
					policy:
						task.payload.policy_name ||
						formatPolicyLabel(t, task.payload.policy_id),
				},
				task.display_name,
			);
		case "storage_policy_migration":
			return translateWithFallback(
				t,
				"tasks:task_name_storage_policy_migration",
				{
					source: formatPolicyLabel(t, task.payload.source_policy_id),
					target: formatPolicyLabel(t, task.payload.target_policy_id),
				},
				task.display_name,
			);
		case "blob_maintenance": {
			const scope =
				task.payload.blob_ids && task.payload.blob_ids.length > 0
					? t("tasks:blob_maintenance_scope_selected", {
							count: task.payload.blob_ids.length,
						})
					: t("tasks:blob_maintenance_scope_all");
			switch (task.payload.action) {
				case "integrity_check":
					return translateWithFallback(
						t,
						"tasks:blob_maintenance_integrity_check_name",
						{ scope },
						task.display_name,
					);
				case "ref_count_reconcile":
					return translateWithFallback(
						t,
						"tasks:blob_maintenance_ref_count_reconcile_name",
						{ scope },
						task.display_name,
					);
				case "orphan_cleanup":
					return translateWithFallback(
						t,
						"tasks:blob_maintenance_orphan_cleanup_name",
						{ scope },
						task.display_name,
					);
			}
			return task.display_name;
		}
		case "system_runtime":
			return formatRuntimeTaskName(
				t,
				task.payload.task_name,
				task.display_name,
			);
		default:
			return formatTaskDisplayNameFromRaw(t, task.kind, task.display_name);
	}
}

export function formatTaskStepStatus(t: TaskTranslate, status: TaskStepStatus) {
	switch (status) {
		case "pending":
			return t("tasks:step_status_pending");
		case "active":
			return t("tasks:step_status_active");
		case "succeeded":
			return t("tasks:step_status_succeeded");
		case "failed":
			return t("tasks:step_status_failed");
		case "skipped":
			return t("tasks:step_status_skipped");
		case "canceled":
			return t("tasks:step_status_canceled");
	}
}

export function formatTaskStepTitle(
	t: TaskTranslate,
	taskKind: BackgroundTaskKind,
	step: TaskStepInfo,
) {
	const key = `tasks:step_${taskKind}_${step.key}`;
	const translated = t(key);
	return translated === key ? step.title : translated;
}

export function formatProgressCounts(current: number, total: number) {
	return `${formatNumber(current)} / ${formatNumber(total)}`;
}

export function taskSummaryTimestamp(t: TaskTranslate, task: TaskInfo) {
	switch (task.status) {
		case "pending":
			return t("tasks:summary_created_at", {
				date: formatDateAbsolute(task.created_at),
			});
		case "processing":
		case "retry":
			if (task.started_at) {
				return t("tasks:summary_started_at", {
					date: formatDateAbsolute(task.started_at),
				});
			}
			return t("tasks:summary_created_at", {
				date: formatDateAbsolute(task.created_at),
			});
		case "succeeded":
			if (task.finished_at) {
				return t("tasks:summary_finished_at", {
					date: formatDateAbsolute(task.finished_at),
				});
			}
			if (task.started_at) {
				return t("tasks:summary_started_at", {
					date: formatDateAbsolute(task.started_at),
				});
			}
			return t("tasks:summary_created_at", {
				date: formatDateAbsolute(task.created_at),
			});
		case "failed":
			if (task.finished_at) {
				return t("tasks:summary_failed_at", {
					date: formatDateAbsolute(task.finished_at),
				});
			}
			if (task.started_at) {
				return t("tasks:summary_started_at", {
					date: formatDateAbsolute(task.started_at),
				});
			}
			return t("tasks:summary_created_at", {
				date: formatDateAbsolute(task.created_at),
			});
		case "canceled":
			if (task.finished_at) {
				return t("tasks:summary_canceled_at", {
					date: formatDateAbsolute(task.finished_at),
				});
			}
			if (task.started_at) {
				return t("tasks:summary_started_at", {
					date: formatDateAbsolute(task.started_at),
				});
			}
			return t("tasks:summary_created_at", {
				date: formatDateAbsolute(task.created_at),
			});
	}
}

export function buildTaskTimeline(t: TaskTranslate, task: TaskInfo) {
	const timeline = [
		{
			label: t("tasks:timeline_created_label"),
			value: formatDateAbsolute(task.created_at),
		},
	];

	if (task.started_at) {
		timeline.push({
			label: t("tasks:timeline_started_label"),
			value: formatDateAbsolute(task.started_at),
		});
	}

	if (task.finished_at) {
		const labelKey =
			task.status === "failed"
				? "tasks:timeline_failed_label"
				: task.status === "canceled"
					? "tasks:timeline_canceled_label"
					: "tasks:timeline_finished_label";
		timeline.push({
			label: t(labelKey),
			value: formatDateAbsolute(task.finished_at),
		});
	}

	return timeline;
}

export function parseTaskResult(task: TaskInfo) {
	if (!task.result) {
		return null;
	}

	switch (task.result.kind) {
		case "archive_compress":
			return {
				target_folder_id: task.result.target_folder_id ?? null,
				target_path: task.result.target_path,
			};
		case "archive_extract":
			return {
				target_folder_id: task.result.target_folder_id,
				target_path: task.result.target_path,
			};
		default:
			return null;
	}
}

export function parseStoragePolicyMigrationResult(task: TaskInfo) {
	if (!task.result || task.result.kind !== "storage_policy_migration") {
		return null;
	}

	return task.result as StoragePolicyMigrationTaskResult & {
		kind: "storage_policy_migration";
	};
}
