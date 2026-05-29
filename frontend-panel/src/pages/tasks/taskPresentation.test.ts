import { describe, expect, it } from "vitest";
import type { TaskInfo } from "@/types/api";
import {
	buildTaskTimeline,
	currentTaskStep,
	formatProgressCounts,
	formatTaskDetail,
	formatTaskDisplayName,
	formatTaskDisplayNameFromRaw,
	formatTaskKind,
	formatTaskStepStatus,
	formatTaskStepTitle,
	parseStoragePolicyMigrationResult,
	parseTaskResult,
	statusBadgeVariant,
	stepCircleClass,
	stepCircleLabel,
	stepConnectorClass,
	stepProgressPercent,
	stepStatusTextClass,
	taskMetaTextClass,
	taskSummaryTimestamp,
} from "./taskPresentation";

function t(key: string, values?: Record<string, number | string>) {
	const translations: Record<string, string> = {
		"tasks:kind_storage_policy_migration": "Storage policy migration",
		"tasks:summary_created_at": `Created ${values?.date}`,
		"tasks:summary_started_at": `Started ${values?.date}`,
		"tasks:summary_finished_at": `Finished ${values?.date}`,
		"tasks:summary_failed_at": `Failed ${values?.date}`,
		"tasks:summary_canceled_at": `Canceled ${values?.date}`,
		"tasks:timeline_created_label": "Created",
		"tasks:timeline_started_label": "Started",
		"tasks:timeline_failed_label": "Failed",
		"tasks:timeline_canceled_label": "Canceled",
		"tasks:timeline_finished_label": "Finished",
		"tasks:blob_maintenance_scope_all": "all blobs",
		"tasks:blob_maintenance_scope_selected": `${values?.count} blob(s)`,
		"tasks:blob_maintenance_integrity_check_name": `Check integrity for ${values?.scope}`,
		"tasks:blob_maintenance_ref_count_reconcile_name": `Reconcile references for ${values?.scope}`,
		"tasks:blob_maintenance_orphan_cleanup_name": `Clean orphan blobs for ${values?.scope}`,
		"tasks:runtime_task_system_health_check": "System health check",
		"tasks:runtime_task_trash_cleanup": "Trash cleanup",
		"tasks:runtime_health_component_database": "Database",
		"tasks:runtime_health_component_remote_nodes": "Remote nodes",
		"tasks:runtime_health_status_degraded": "Degraded",
		"tasks:runtime_health_status_healthy": "Healthy",
		"tasks:runtime_health_status_unhealthy": "Unhealthy",
		"tasks:runtime_health_component_status": `${values?.component} is ${values?.status}`,
		"tasks:runtime_system_health_issue_detail": `Issues: ${values?.components}`,
		"tasks:status_text_deleted_completed_upload_sessions": `Deleted ${values?.count} completed sessions (${values?.broken} broken)`,
		"tasks:status_text_blob_reconcile_finished": `Fixed ${values?.fixed}, deleted ${values?.deleted}`,
		"tasks:status_text_cleaned_expired_auth_sessions": `Cleaned ${values?.count} auth sessions`,
		"tasks:status_text_cleaned_expired_external_auth_flows": `Cleaned ${values?.count} external auth flows`,
		"tasks:status_text_cleaned_expired_locks": `Cleaned ${values?.count} locks`,
		"tasks:status_text_cleaned_expired_mfa_flows": `Cleaned ${values?.count} MFA flows`,
		"tasks:status_text_cleaned_expired_task_artifacts": `Cleaned ${values?.count} task artifacts`,
		"tasks:status_text_cleaned_expired_wopi_sessions": `Cleaned ${values?.count} WOPI sessions`,
		"tasks:status_text_storage_migration_completed":
			"Localized storage migration completed",
		"tasks:status_text_system_healthy": "System healthy",
		"tasks:task_name_archive_compress": `Compress ${values?.name}`,
		"tasks:task_name_archive_extract": `Extract ${values?.name}`,
		"tasks:task_name_archive_preview_generate_file_id": `Preview file ${values?.fileId} blob ${values?.blobId}`,
		"tasks:media_metadata_kind_audio": "audio",
		"tasks:media_processor_storage_native": "Localized storage-native",
		"tasks:task_name_media_metadata_extract_blob": `Extract ${values?.kind} metadata for ${values?.blob}`,
		"tasks:task_name_storage_policy_migration": `Migrate ${values?.source} to ${values?.target}`,
		"tasks:task_name_storage_policy_temp_cleanup_policy_id": `Cleanup ${values?.policy}`,
		"tasks:task_name_thumbnail_generate_blob_with_processor": `Thumbnail ${values?.blob} via ${values?.processor}`,
		"tasks:task_name_trash_purge_all": "Empty trash",
		"tasks:summary_policy_id": `Policy #${values?.id}`,
		"tasks:task_name_blob_id": `Blob #${values?.id}`,
		"tasks:step_storage_policy_migration_prepare_sources":
			"Prepare source policy",
		"tasks:step_storage_policy_migration_scan_blobs": "Scan source blobs",
		"tasks:step_storage_policy_migration_finish": "Finish migration",
		"tasks:step_thumbnail_generate_waiting": "Waiting",
		"tasks:step_thumbnail_generate_inspect_source": "Inspect source file",
		"tasks:step_thumbnail_generate_render_thumbnail": "Render thumbnail",
		"tasks:step_thumbnail_generate_persist_thumbnail": "Save thumbnail",
	};
	return translations[key] ?? key;
}

function createTask(overrides: Partial<TaskInfo> = {}): TaskInfo {
	return {
		attempt_count: 0,
		can_retry: false,
		created_at: "2026-04-17T00:00:00Z",
		creator: null,
		display_name: "Migrate blobs",
		expires_at: "2026-04-18T00:00:00Z",
		finished_at: null,
		id: 31,
		kind: "storage_policy_migration",
		last_error: null,
		max_attempts: 1,
		payload: {
			delete_source_after_success: false,
			kind: "storage_policy_migration",
			plan_hash: "plan-a",
			source_policy_id: 1,
			source_policy_updated_at: "2026-04-17T00:00:00Z",
			target_policy_id: 2,
			target_policy_updated_at: "2026-04-17T00:00:00Z",
		},
		progress_current: 0,
		progress_percent: 0,
		progress_total: 0,
		result: null,
		share_id: null,
		started_at: null,
		status: "pending",
		status_text: null,
		steps: [],
		team_id: null,
		updated_at: "2026-04-17T00:00:00Z",
		...overrides,
	};
}

describe("taskPresentation storage policy migration", () => {
	it("formats the storage policy migration kind", () => {
		expect(formatTaskKind(t, "storage_policy_migration")).toBe(
			"Storage policy migration",
		);
	});

	it("localizes blob maintenance display names from structured payloads", () => {
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Check integrity for all blobs",
					kind: "blob_maintenance",
					payload: {
						action: "integrity_check",
						kind: "blob_maintenance",
					},
				}),
			),
		).toBe("Check integrity for all blobs");
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Reconcile references for 2 blob(s)",
					kind: "blob_maintenance",
					payload: {
						action: "ref_count_reconcile",
						blob_ids: [3, 4],
						kind: "blob_maintenance",
					},
				}),
			),
		).toBe("Reconcile references for 2 blob(s)");
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Clean orphan blobs for 2 blob(s)",
					kind: "blob_maintenance",
					payload: {
						action: "orphan_cleanup",
						blob_ids: [3, 4],
						kind: "blob_maintenance",
					},
				}),
			),
		).toBe("Clean orphan blobs for 2 blob(s)");
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Backend maintenance name",
					kind: "blob_maintenance",
					payload: {
						action: "unexpected_action",
						kind: "blob_maintenance",
					} as never,
				}),
			),
		).toBe("Backend maintenance name");
	});

	it("localizes system runtime names from structured payloads and raw display names", () => {
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Backend trash cleanup",
					kind: "system_runtime",
					payload: {
						kind: "system_runtime",
						task_name: "trash-cleanup",
					},
				}),
			),
		).toBe("Trash cleanup");
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Custom runtime task",
					kind: "system_runtime",
					payload: {
						kind: "system_runtime",
						task_name: "custom-runtime-task",
					},
				}),
			),
		).toBe("Custom runtime task");
		expect(
			formatTaskDisplayNameFromRaw(
				(key, values) =>
					key === "tasks:runtime_task_system_health_check"
						? "Localized system health check"
						: t(key, values),
				"system_runtime",
				"System health check",
			),
		).toBe("Localized system health check");
		expect(
			formatTaskDisplayNameFromRaw(t, "system_runtime", "unknown runtime"),
		).toBe("unknown runtime");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"system_runtime",
				"Task artifact cleanup",
			),
		).toBe("Task artifact cleanup");
	});

	it("localizes media metadata display names from structured payloads", () => {
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Extract audio metadata for blob #12",
					kind: "media_metadata_extract",
					payload: {
						blob_hash: "hash-b",
						blob_id: 12,
						kind: "media_metadata_extract",
						media_kind: "audio",
						source_file_name: "song.flac",
						source_mime_type: "audio/flac",
					},
				}),
			),
		).toBe("Extract audio metadata for song.flac");
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Extract audio metadata for blob #12",
					kind: "media_metadata_extract",
					payload: {
						blob_hash: "hash-b",
						blob_id: 12,
						kind: "media_metadata_extract",
						media_kind: "audio",
						source_file_name: "",
						source_mime_type: "audio/flac",
					},
				}),
			),
		).toBe("Extract audio metadata for Blob #12");
	});

	it("localizes raw task display names from backend naming conventions", () => {
		expect(
			formatTaskDisplayNameFromRaw(t, "archive_extract", "Extract logs.zip"),
		).toBe("Extract logs.zip");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"archive_compress",
				"Compress export.zip",
			),
		).toBe("Compress export.zip");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"archive_preview_generate",
				"Generate archive preview for file #7 blob #9 logs.zip",
			),
		).toBe("Preview file 7 blob 9");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"archive_preview_generate",
				"Generate archive preview for file #7 blob #9",
			),
		).toBe("Preview file 7 blob 9");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"thumbnail_generate",
				"Generate thumbnail for blob #11 via storage_native",
			),
		).toBe("Thumbnail Blob #11 via Localized storage-native");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"thumbnail_generate",
				"Generate thumbnail for blob #11 via native",
			),
		).toBe("Thumbnail Blob #11 via Localized storage-native");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"media_metadata_extract",
				"Extract audio metadata for blob #12",
			),
		).toBe("Extract audio metadata for Blob #12");
		expect(
			formatTaskDisplayNameFromRaw(t, "trash_purge_all", "Empty trash"),
		).toBe("Empty trash");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"storage_policy_temp_cleanup",
				"Clean deleted storage policy #5 temporary uploads",
			),
		).toBe("Cleanup Policy #5");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"storage_policy_temp_cleanup",
				"Clean storage policy #5 temporary uploads",
			),
		).toBe("Cleanup Policy #5");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"storage_policy_migration",
				"Migrate storage policy #1 to #2",
			),
		).toBe("Migrate Policy #1 to Policy #2");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"blob_maintenance",
				"Check integrity for 3 blob(s)",
			),
		).toBe("Check integrity for 3 blob(s)");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"blob_maintenance",
				"Reconcile references for all blobs",
			),
		).toBe("Reconcile references for all blobs");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"blob_maintenance",
				"Clean orphan blobs for custom scope",
			),
		).toBe("Clean orphan blobs for custom scope");
		expect(
			formatTaskDisplayNameFromRaw(
				t,
				"thumbnail_generate",
				"backend custom name",
			),
		).toBe("backend custom name");
	});

	it("localizes stable backend status detail text", () => {
		expect(
			formatTaskDetail(t, createTask({ status_text: "system healthy" })),
		).toBe("System healthy");
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "deleted 14 completed sessions (0 broken)",
				}),
			),
		).toBe("Deleted 14 completed sessions (0 broken)");
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "fixed 3 ref counts, deleted 2 orphan blobs",
				}),
			),
		).toBe("Fixed 3, deleted 2");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 4 expired auth sessions" }),
			),
		).toBe("Cleaned 4 auth sessions");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 5 expired external auth flows" }),
			),
		).toBe("Cleaned 5 external auth flows");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 6 expired MFA flows" }),
			),
		).toBe("Cleaned 6 MFA flows");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 7 expired locks" }),
			),
		).toBe("Cleaned 7 locks");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 8 expired task artifacts" }),
			),
		).toBe("Cleaned 8 task artifacts");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 9 expired WOPI sessions" }),
			),
		).toBe("Cleaned 9 WOPI sessions");
		expect(
			formatTaskDetail(t, createTask({ status_text: "Migration completed" })),
		).toBe("Localized storage migration completed");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "remote_nodes=unhealthy: failed" }),
			),
		).toBe("Issues: Remote nodes is Unhealthy: failed");
		expect(
			formatTaskDetail(t, createTask({ status_text: "database=degraded:" })),
		).toBe("Issues: Database is Degraded");
		expect(
			formatTaskDetail(t, createTask({ status_text: "custom detail" })),
		).toBe("custom detail");
		expect(
			formatTaskDetail(t, createTask({ status_text: "   " }), "empty"),
		).toBe("empty");
	});

	it("formats system runtime health results before generic status text", () => {
		expect(
			formatTaskDetail(
				t,
				createTask({
					kind: "system_runtime",
					payload: { kind: "system_runtime", task_name: "system-health-check" },
					result: {
						kind: "system_runtime",
						system_health: {
							components: [{ name: "database", status: "healthy" }],
							status: "healthy",
						},
						task_name: "system-health-check",
					} as never,
					status_text: "database=unhealthy: stale detail",
				}),
			),
		).toBe("System healthy");
		expect(
			formatTaskDetail(
				t,
				createTask({
					kind: "system_runtime",
					payload: { kind: "system_runtime", task_name: "system-health-check" },
					result: {
						kind: "system_runtime",
						system_health: {
							components: [
								{ name: "database", status: "healthy" },
								{ name: "remote_nodes", status: "degraded" },
							],
							status: "degraded",
						},
						task_name: "system-health-check",
					} as never,
					status_text: "remote_nodes=degraded: slow",
				}),
			),
		).toBe("Issues: Remote nodes is Degraded");
		expect(
			formatTaskDetail(
				t,
				createTask({
					kind: "system_runtime",
					payload: { kind: "system_runtime", task_name: "system-health-check" },
					result: {
						kind: "system_runtime",
						system_health: {
							components: [],
							status: "unhealthy",
						},
						task_name: "system-health-check",
					} as never,
					status_text: "system unhealthy",
				}),
			),
		).toBe("Issues: Unhealthy");
	});

	it("covers task presentation utility branches", () => {
		expect(statusBadgeVariant("retry")).toBe("secondary");
		expect(statusBadgeVariant("succeeded")).toBe("default");
		expect(statusBadgeVariant("failed")).toBe("destructive");
		expect(statusBadgeVariant("canceled")).toBe("outline");
		expect(taskMetaTextClass("processing")).toBe("text-primary");
		expect(taskMetaTextClass("succeeded")).toBe("text-foreground");
		expect(taskMetaTextClass("failed")).toBe("text-destructive");
		expect(taskMetaTextClass("pending")).toBe("text-muted-foreground");
		expect(stepCircleLabel(2, "failed")).toBe("!");
		expect(stepCircleLabel(2, "canceled")).toBe("X");
		expect(stepCircleLabel(2, "pending")).toBe("3");
		expect(stepStatusTextClass("active")).toBe("text-primary");
		expect(stepStatusTextClass("succeeded")).toBe("text-foreground");
		expect(stepStatusTextClass("failed")).toBe("text-destructive");
		expect(stepStatusTextClass("skipped")).toBe("text-muted-foreground");
		expect(stepStatusTextClass("canceled")).toBe("text-muted-foreground");
		expect(stepConnectorClass("succeeded")).toBe("bg-primary/70");
		expect(stepConnectorClass("active")).toBe("bg-primary/35");
		expect(stepConnectorClass("failed")).toBe("bg-destructive/35");
		expect(stepConnectorClass("skipped")).toBe("bg-border/40");
		expect(stepConnectorClass("canceled")).toBe("bg-border/60");
		expect(stepConnectorClass("pending")).toBe("bg-border/40");
		expect(stepCircleClass("active")).toContain("ring-primary");
		expect(stepCircleClass("succeeded")).toContain("border-primary");
		expect(stepCircleClass("failed")).toContain("text-destructive");
		expect(stepCircleClass("skipped")).toContain("text-muted-foreground");
		expect(stepCircleClass("canceled")).toContain("bg-muted");
		expect(stepCircleClass("pending")).toContain("bg-background");
		expect(formatTaskStepStatus(t, "pending")).toBe(
			"tasks:step_status_pending",
		);
		expect(formatTaskStepStatus(t, "active")).toBe("tasks:step_status_active");
		expect(formatTaskStepStatus(t, "succeeded")).toBe(
			"tasks:step_status_succeeded",
		);
		expect(formatTaskStepStatus(t, "failed")).toBe("tasks:step_status_failed");
		expect(formatTaskStepStatus(t, "skipped")).toBe(
			"tasks:step_status_skipped",
		);
		expect(formatTaskStepStatus(t, "canceled")).toBe(
			"tasks:step_status_canceled",
		);
		expect(formatProgressCounts(1200, 3400)).toBe("1,200 / 3,400");
		expect(
			stepProgressPercent({
				key: "done",
				progress_current: 0,
				progress_total: 0,
				status: "succeeded",
				title: "Done",
			}),
		).toBe(100);
		expect(
			stepProgressPercent({
				key: "overflow",
				progress_current: 15,
				progress_total: 10,
				status: "active",
				title: "Overflow",
			}),
		).toBe(100);
		expect(
			stepProgressPercent({
				key: "negative",
				progress_current: -5,
				progress_total: 10,
				status: "active",
				title: "Negative",
			}),
		).toBe(0);
	});

	it("selects the current step by active, failed, last, and empty fallbacks", () => {
		const activeTask = createTask({
			steps: [
				{
					key: "queued",
					progress_current: 0,
					progress_total: 0,
					status: "succeeded",
					title: "Queued",
				},
				{
					key: "copy",
					progress_current: 1,
					progress_total: 2,
					status: "active",
					title: "Copy",
				},
			],
		});
		expect(currentTaskStep(activeTask)?.key).toBe("copy");
		expect(
			currentTaskStep({
				...activeTask,
				steps: [
					{ ...activeTask.steps[0], status: "succeeded" },
					{ ...activeTask.steps[1], status: "failed" },
				],
			})?.key,
		).toBe("copy");
		expect(
			currentTaskStep({
				...activeTask,
				steps: [
					{ ...activeTask.steps[0], status: "succeeded" },
					{ ...activeTask.steps[1], status: "pending" },
				],
			})?.key,
		).toBe("copy");
		expect(currentTaskStep(createTask({ steps: [] }))).toBeNull();
	});

	it("formats task timestamps and timelines using status-specific labels", () => {
		expect(taskSummaryTimestamp(t, createTask())).toMatch(/^Created /);
		expect(
			taskSummaryTimestamp(
				t,
				createTask({
					status: "processing",
					started_at: "2026-04-17T00:01:00Z",
				}),
			),
		).toMatch(/^Started /);
		expect(
			taskSummaryTimestamp(
				t,
				createTask({
					status: "retry",
					started_at: null,
				}),
			),
		).toMatch(/^Created /);
		expect(
			taskSummaryTimestamp(
				t,
				createTask({
					finished_at: null,
					status: "succeeded",
					started_at: "2026-04-17T00:01:00Z",
				}),
			),
		).toMatch(/^Started /);
		expect(
			taskSummaryTimestamp(
				t,
				createTask({
					finished_at: null,
					status: "succeeded",
					started_at: null,
				}),
			),
		).toMatch(/^Created /);
		expect(
			taskSummaryTimestamp(
				t,
				createTask({
					finished_at: "2026-04-17T00:02:00Z",
					status: "failed",
				}),
			),
		).toMatch(/^Failed /);
		expect(
			taskSummaryTimestamp(
				t,
				createTask({
					finished_at: null,
					status: "failed",
					started_at: "2026-04-17T00:01:00Z",
				}),
			),
		).toMatch(/^Started /);
		expect(
			taskSummaryTimestamp(
				t,
				createTask({
					finished_at: null,
					status: "failed",
					started_at: null,
				}),
			),
		).toMatch(/^Created /);
		expect(
			taskSummaryTimestamp(
				t,
				createTask({
					finished_at: "2026-04-17T00:02:00Z",
					status: "canceled",
				}),
			),
		).toMatch(/^Canceled /);
		expect(
			taskSummaryTimestamp(
				t,
				createTask({
					finished_at: null,
					status: "canceled",
					started_at: "2026-04-17T00:01:00Z",
				}),
			),
		).toMatch(/^Started /);
		expect(
			taskSummaryTimestamp(
				t,
				createTask({
					finished_at: null,
					status: "canceled",
					started_at: null,
				}),
			),
		).toMatch(/^Created /);
		expect(
			buildTaskTimeline(
				t,
				createTask({
					finished_at: "2026-04-17T00:02:00Z",
					status: "canceled",
				}),
			).map((entry) => entry.label),
		).toEqual(["Created", "Canceled"]);
	});

	it("translates known storage migration steps and falls back to backend titles", () => {
		expect(
			formatTaskStepTitle(t, "storage_policy_migration", {
				key: "prepare_sources",
				title: "Backend prepare title",
				status: "succeeded",
				progress_current: 1,
				progress_total: 1,
			}),
		).toBe("Prepare source policy");
		expect(
			formatTaskStepTitle(t, "storage_policy_migration", {
				key: "scan_blobs",
				title: "Scan blobs",
				status: "active",
				progress_current: 3,
				progress_total: 10,
			}),
		).toBe("Scan source blobs");
		expect(
			formatTaskStepTitle(t, "storage_policy_migration", {
				key: "finish",
				title: "Backend finish title",
				status: "succeeded",
				progress_current: 1,
				progress_total: 1,
			}),
		).toBe("Finish migration");
		expect(
			formatTaskStepTitle(t, "storage_policy_migration", {
				key: "custom_backend_step",
				title: "Backend custom step",
				status: "pending",
				progress_current: 0,
				progress_total: 0,
			}),
		).toBe("Backend custom step");
	});

	it("translates thumbnail generation steps", () => {
		expect(
			formatTaskStepTitle(t, "thumbnail_generate", {
				key: "waiting",
				title: "step_thumbnail_generate_waiting",
				status: "succeeded",
				progress_current: 1,
				progress_total: 1,
			}),
		).toBe("Waiting");
		expect(
			formatTaskStepTitle(t, "thumbnail_generate", {
				key: "inspect_source",
				title: "step_thumbnail_generate_inspect_source",
				status: "succeeded",
				progress_current: 1,
				progress_total: 1,
			}),
		).toBe("Inspect source file");
		expect(
			formatTaskStepTitle(t, "thumbnail_generate", {
				key: "render_thumbnail",
				title: "step_thumbnail_generate_render_thumbnail",
				status: "succeeded",
				progress_current: 1,
				progress_total: 1,
			}),
		).toBe("Render thumbnail");
		expect(
			formatTaskStepTitle(t, "thumbnail_generate", {
				key: "persist_thumbnail",
				title: "step_thumbnail_generate_persist_thumbnail",
				status: "succeeded",
				progress_current: 1,
				progress_total: 1,
			}),
		).toBe("Save thumbnail");
	});

	it("parses storage migration results and ignores other result shapes", () => {
		const migrationResult = {
			failed_blobs: 0,
			kind: "storage_policy_migration",
			merged_blobs: 1,
			migrated_blobs: 12,
			migrated_bytes: 4096,
			scanned_blobs: 13,
			skipped_blobs: 0,
			source_policy_id: 1,
			target_policy_id: 2,
		} as const;

		expect(
			parseStoragePolicyMigrationResult(
				createTask({
					result: migrationResult,
					status: "succeeded",
				}),
			),
		).toEqual(migrationResult);
		expect(parseStoragePolicyMigrationResult(createTask())).toBeNull();
		expect(
			parseStoragePolicyMigrationResult(
				createTask({
					kind: "archive_extract",
					result: {
						kind: "archive_extract",
						target_folder_id: 2,
						target_path: "/archive",
					},
				}),
			),
		).toBeNull();
	});

	it("parses archive task results and ignores non-archive results", () => {
		expect(
			parseTaskResult(
				createTask({
					kind: "archive_compress",
					result: {
						kind: "archive_compress",
						target_file_id: 90,
						target_file_name: "bundle.zip",
						target_folder_id: undefined,
						target_path: "/bundle.zip",
					} as never,
				}),
			),
		).toEqual({ target_folder_id: null, target_path: "/bundle.zip" });
		expect(
			parseTaskResult(
				createTask({
					kind: "archive_extract",
					result: {
						kind: "archive_extract",
						target_folder_id: 7,
						target_path: "/extract",
					},
				}),
			),
		).toEqual({ target_folder_id: 7, target_path: "/extract" });
		expect(
			parseTaskResult(
				createTask({
					kind: "thumbnail_generate",
					result: {
						blob_id: 1,
						kind: "thumbnail_generate",
						processor: "native",
						reused_existing_thumbnail: false,
						thumbnail_path: "thumb.jpg",
						thumbnail_processor: "native",
						thumbnail_version: "1",
					} as never,
				}),
			),
		).toBeNull();
	});
});
