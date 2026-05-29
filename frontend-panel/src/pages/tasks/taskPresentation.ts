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
		case "trash_purge_all":
			return t("tasks:kind_trash_purge_all");
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
		case "blob_maintenance": {
			const scope =
				task.payload.blob_ids && task.payload.blob_ids.length > 0
					? t("tasks:blob_maintenance_scope_selected", {
							count: task.payload.blob_ids.length,
						})
					: t("tasks:blob_maintenance_scope_all");
			switch (task.payload.action) {
				case "integrity_check":
					return t("tasks:blob_maintenance_integrity_check_name", { scope });
				case "ref_count_reconcile":
					return t("tasks:blob_maintenance_ref_count_reconcile_name", {
						scope,
					});
				case "orphan_cleanup":
					return t("tasks:blob_maintenance_orphan_cleanup_name", { scope });
			}
			return task.display_name;
		}
		default:
			return task.display_name;
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
