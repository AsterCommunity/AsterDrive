import { describe, expect, it } from "vitest";
import type { TaskInfo } from "@/types/api";
import {
	buildTaskTimeline,
	currentTaskStep,
	formatProgressCounts,
	formatTaskDetail,
	formatTaskDisplayName,
	formatTaskKind,
	formatTaskStatus,
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
		"tasks:blob_maintenance_integrity_check_name": "Check blob integrity",
		"tasks:blob_maintenance_ref_count_reconcile_name":
			"Reconcile blob references",
		"tasks:blob_maintenance_orphan_cleanup_name": "Clean orphan blobs",
		"tasks:runtime_task_system_health_check": "System health check",
		"tasks:runtime_task_trash_cleanup": "Trash cleanup",
		"tasks:runtime_system_health_issue_detail": `Issues: ${values?.components}`,
		"tasks:runtime_health_component_database": "Database",
		"tasks:runtime_health_component_remote_nodes": "Remote nodes",
		"tasks:runtime_health_component_status": `${values?.component} ${values?.status}`,
		"tasks:runtime_health_status_degraded": "degraded",
		"tasks:runtime_health_status_unhealthy": "unhealthy",
		"tasks:status_text_storage_migration_completed":
			"Localized storage migration completed",
		"tasks:status_text_system_healthy": "System healthy",
		"tasks:status_text_archive_ready": `Archive ready: ${values?.name}`,
		"tasks:status_text_archive_extracted": `Extracted to ${values?.name}`,
		"tasks:status_text_thumbnail_ready": "Thumbnail ready",
		"tasks:status_text_image_preview_ready": "Image preview ready",
		"tasks:status_text_waiting_presigned_url_expiry":
			"Waiting for presigned URLs to expire",
		"tasks:task_name_archive_compress": `Compress ${values?.name}`,
		"tasks:task_name_archive_extract": `Extract ${values?.name}`,
		"tasks:task_name_archive_preview_generate_file_id": `Preview file ${values?.fileId} blob ${values?.blobId}`,
		"tasks:task_name_archive_preview_generate": `Preview ${values?.name}`,
		"tasks:task_name_media_metadata_extract_blob": `Extract ${values?.kind} metadata for Blob #${values?.blobId}`,
		"tasks:task_name_media_metadata_extract_source": `Extract ${values?.kind} metadata for ${values?.source}`,
		"tasks:task_name_offline_download_target_folder_with_engine": `Import from ${values?.source} to folder #${values?.targetFolderId} via ${values?.engine}`,
		"tasks:task_name_storage_policy_migration": `Migrate Policy #${values?.sourcePolicyId} to Policy #${values?.targetPolicyId}`,
		"tasks:task_name_storage_policy_temp_cleanup": `Cleanup ${values?.policy}`,
		"tasks:task_name_storage_policy_temp_cleanup_policy_id": `Cleanup Policy #${values?.policyId}`,
		"tasks:task_name_thumbnail_generate": `Thumbnail ${values?.source} via ${values?.processor}`,
		"tasks:task_name_thumbnail_generate_blob_with_processor": `Thumbnail Blob #${values?.blobId} via ${values?.processor}`,
		"tasks:task_name_image_preview_generate": `Image preview ${values?.source} via ${values?.processor}`,
		"tasks:task_name_image_preview_generate_blob_with_processor": `Image preview Blob #${values?.blobId} via ${values?.processor}`,
		"tasks:task_name_trash_purge_all": "Empty trash",
		"tasks:step_storage_policy_migration_prepare_sources":
			"Prepare source policy",
		"tasks:step_storage_policy_migration_scan_blobs": "Scan source blobs",
		"tasks:step_storage_policy_migration_finish": "Finish migration",
		"tasks:step_thumbnail_generate_waiting": "Waiting",
		"tasks:step_thumbnail_generate_inspect_source": "Inspect source file",
		"tasks:step_thumbnail_generate_render_thumbnail": "Render thumbnail",
		"tasks:step_thumbnail_generate_persist_thumbnail": "Save thumbnail",
		"tasks:kind_image_preview_generate": "Image preview generation",
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

describe("taskPresentation structured presentation", () => {
	it("uses structured title messages before payloads or backend text", () => {
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Backend renamed title",
					kind: "thumbnail_generate",
					payload: {
						blob_hash: "hash-old",
						blob_id: 11,
						kind: "thumbnail_generate",
						processor: "images",
						source_file_name: "old.png",
						source_mime_type: "image/png",
					},
					presentation: {
						title: {
							code: "task_name_thumbnail_generate_blob_with_processor",
							params: {
								blobId: 42,
								processor: "storage_native",
							},
						},
					},
				}),
			),
		).toBe("Thumbnail Blob #42 via storage_native");
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Backend image preview title",
					kind: "image_preview_generate" as never,
					payload: {
						blob_hash: "hash-old",
						blob_id: 11,
						kind: "image_preview_generate",
						processor: "images",
						source_file_name: "old.png",
						source_mime_type: "image/png",
					} as never,
					presentation: {
						title: {
							code: "task_name_image_preview_generate_blob_with_processor" as never,
							params: {
								blobId: 42,
								processor: "images",
							},
						},
					},
				}),
			),
		).toBe("Image preview Blob #42 via images");
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Backend preview fallback",
					kind: "archive_preview_generate",
					payload: {
						blob_hash: "hash-preview",
						file_id: 5,
						kind: "archive_preview_generate",
						limit_signature: "sig",
						source_blob_id: 6,
						source_file_name: "old.zip",
					},
					presentation: {
						title: {
							code: "task_name_archive_preview_generate",
							params: {
								name: "logs.zip",
							},
						},
					},
				}),
			),
		).toBe("Preview logs.zip");
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Backend cleanup fallback",
					kind: "storage_policy_temp_cleanup",
					payload: {
						driver_type: "local",
						kind: "storage_policy_temp_cleanup",
						multipart_upload_count: 0,
						policy_id: 7,
						policy_name: "old policy",
						temp_key_count: 1,
					},
					presentation: {
						title: {
							code: "task_name_storage_policy_temp_cleanup",
							params: {
								policy: "Archive policy",
								policyId: 9,
							},
						},
					},
				}),
			),
		).toBe("Cleanup Archive policy");
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Import from https://example.com/file.bin via aria2",
					kind: "offline_download",
					payload: {
						kind: "offline_download",
						source_display_url: "https://example.com/file.bin",
						target_folder_id: 34,
					},
					presentation: {
						title: {
							code: "task_name_offline_download_target_folder_with_engine",
							params: {
								engine: "aria2",
								source: "https://example.com/file.bin",
								targetFolderId: 34,
							},
						},
					},
				}),
			),
		).toBe("Import from https://example.com/file.bin to folder #34 via aria2");
	});

	it("uses structured status messages before status_text parsing", () => {
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "backend changed this sentence",
					presentation: {
						status: {
							code: "status_text_archive_ready",
							params: {
								name: "structured.zip",
							},
						},
					},
				}),
			),
		).toBe("Archive ready: structured.zip");
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "backend renamed archive result",
					presentation: {
						status: {
							code: "status_text_thumbnail_ready",
							params: {
								name: "bundle.zip",
							},
						},
					},
				}),
			),
		).toBe("Thumbnail ready");
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "backend changed this image preview sentence",
					presentation: {
						status: {
							code: "status_text_image_preview_ready" as never,
							params: {},
						},
					},
				}),
			),
		).toBe("Image preview ready");
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "remote_nodes=unhealthy: stale detail",
					presentation: {
						status: {
							code: "runtime_system_health_issue_detail",
							params: {
								components: [
									{
										message: "lagging",
										name: "database",
										status: "degraded",
									},
								],
								status: "degraded",
							},
						},
					},
				}),
			),
		).toBe("Issues: Database degraded: lagging");
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "system unhealthy",
					presentation: {
						status: {
							code: "runtime_system_health_issue_detail",
							params: {
								components: [],
								status: "unhealthy",
							},
						},
					},
				}),
			),
		).toBe("Issues: unhealthy");
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "backend fallback detail",
					presentation: {
						status: {
							code: "runtime_system_health_issue_detail",
							params: {
								components: [
									{
										message: "ignored because status is missing",
										name: "database",
									},
								],
							},
						},
					},
				}),
			),
		).toBe("backend fallback detail");
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "backend fallback detail",
					presentation: {
						status: {
							code: "runtime_system_health_issue_detail",
							params: {
								components: [
									{
										message: "ignored because name is not a string",
										name: 42,
										status: "degraded",
									},
								],
								status: 7,
							},
						},
					},
				}),
			),
		).toBe("backend fallback detail");
	});

	it("falls back safely for unknown or missing structured messages", () => {
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Backend custom title",
					presentation: {
						title: {
							code: "future_task_title" as never,
							params: {
								anything: 1,
							},
						},
					},
				}),
			),
		).toBe("Backend custom title");
		expect(
			formatTaskDetail(
				t,
				createTask({
					presentation: {
						status: {
							code: "future_status_code" as never,
							params: {
								anything: 1,
							},
						},
					},
					status_text: "custom backend status",
				}),
			),
		).toBe("custom backend status");
		expect(
			formatTaskDetail(
				t,
				createTask({
					display_name: "Backend title",
					presentation: {
						title: {
							code: "task_name_thumbnail_generate_blob_with_processor",
							params: {
								processor: "storage_native",
							},
						},
					},
					status_text: "Migration completed",
				}),
			),
		).toBe("Migration completed");
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Backend raw title",
					presentation: {
						title: {
							code: "task_name_archive_compress",
							params: {
								name: { nested: "not accepted" },
							},
						},
					},
				}),
			),
		).toBe("Backend raw title");
	});
});

describe("taskPresentation storage policy migration", () => {
	it("formats the storage policy migration kind", () => {
		expect(formatTaskKind(t, "storage_policy_migration")).toBe(
			"Storage policy migration",
		);
		expect(formatTaskKind(t, "image_preview_generate" as never)).toBe(
			"Image preview generation",
		);
	});

	it("keeps raw display names when presentation is absent", () => {
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

	it("localizes system runtime names only from structured presentation", () => {
		expect(
			formatTaskDisplayName(
				t,
				createTask({
					display_name: "Backend system health fallback",
					kind: "system_runtime",
					payload: {
						kind: "system_runtime",
						task_name: "system-health-check",
					},
					presentation: {
						title: {
							code: "runtime_task_system_health_check",
						},
					},
				}),
			),
		).toBe("System health check");
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
		).toBe("Backend trash cleanup");
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
	});

	it("keeps raw media metadata display names without structured presentation", () => {
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
		).toBe("Extract audio metadata for blob #12");
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
		).toBe("Extract audio metadata for blob #12");
	});

	it("falls back to raw backend status detail text", () => {
		expect(
			formatTaskDetail(t, createTask({ status_text: "system healthy" })),
		).toBe("system healthy");
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "deleted 14 completed sessions (0 broken)",
				}),
			),
		).toBe("deleted 14 completed sessions (0 broken)");
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "fixed 3 ref counts, deleted 2 orphan blobs",
				}),
			),
		).toBe("fixed 3 ref counts, deleted 2 orphan blobs");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 4 expired auth sessions" }),
			),
		).toBe("cleaned up 4 expired auth sessions");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 5 expired external auth flows" }),
			),
		).toBe("cleaned up 5 expired external auth flows");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 6 expired MFA flows" }),
			),
		).toBe("cleaned up 6 expired MFA flows");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 7 expired locks" }),
			),
		).toBe("cleaned up 7 expired locks");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 8 expired task artifacts" }),
			),
		).toBe("cleaned up 8 expired task artifacts");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "cleaned up 9 expired WOPI sessions" }),
			),
		).toBe("cleaned up 9 expired WOPI sessions");
		expect(
			formatTaskDetail(t, createTask({ status_text: "Migration completed" })),
		).toBe("Migration completed");
		expect(
			formatTaskDetail(
				t,
				createTask({ status_text: "remote_nodes=unhealthy: failed" }),
			),
		).toBe("remote_nodes=unhealthy: failed");
		expect(
			formatTaskDetail(t, createTask({ status_text: "database=degraded:" })),
		).toBe("database=degraded:");
		expect(
			formatTaskDetail(t, createTask({ status_text: "custom detail" })),
		).toBe("custom detail");
		expect(
			formatTaskDetail(t, createTask({ status_text: "   " }), "empty"),
		).toBe("empty");
	});

	it("does not derive system runtime health text from result payloads", () => {
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "  backend detail  ",
				}),
			),
		).toBe("backend detail");
		expect(
			formatTaskDetail(
				t,
				createTask({
					status_text: "remote_nodes=degraded: slow",
				}),
			),
		).toBe("remote_nodes=degraded: slow");
	});

	it("covers task presentation utility branches", () => {
		expect(statusBadgeVariant("retry")).toBe("secondary");
		expect(statusBadgeVariant("succeeded")).toBe("default");
		expect(statusBadgeVariant("failed")).toBe("destructive");
		expect(statusBadgeVariant("canceled")).toBe("outline");
		expect(statusBadgeVariant("pending")).toBe("secondary");
		expect(statusBadgeVariant("processing")).toBe("secondary");
		expect(taskMetaTextClass("processing")).toBe("text-primary");
		expect(taskMetaTextClass("retry")).toBe("text-primary");
		expect(taskMetaTextClass("succeeded")).toBe("text-foreground");
		expect(taskMetaTextClass("failed")).toBe("text-destructive");
		expect(taskMetaTextClass("pending")).toBe("text-muted-foreground");
		expect(taskMetaTextClass("canceled")).toBe("text-muted-foreground");
		expect(stepCircleLabel(2, "failed")).toBe("!");
		expect(stepCircleLabel(2, "skipped")).toBe("3");
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
		expect(formatTaskStatus(t, "pending")).toBe("tasks:status_pending");
		expect(formatTaskStatus(t, "processing")).toBe("tasks:status_processing");
		expect(formatTaskStatus(t, "retry")).toBe("tasks:status_retry");
		expect(formatTaskStatus(t, "succeeded")).toBe("tasks:status_succeeded");
		expect(formatTaskStatus(t, "failed")).toBe("tasks:status_failed");
		expect(formatTaskStatus(t, "canceled")).toBe("tasks:status_canceled");
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
					finished_at: "2026-04-17T00:02:00Z",
					status: "succeeded",
				}),
			),
		).toMatch(/^Finished /);
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
		expect(
			buildTaskTimeline(
				t,
				createTask({
					finished_at: "2026-04-17T00:02:00Z",
					started_at: "2026-04-17T00:01:00Z",
					status: "failed",
				}),
			).map((entry) => entry.label),
		).toEqual(["Created", "Started", "Failed"]);
		expect(
			buildTaskTimeline(
				t,
				createTask({
					finished_at: "2026-04-17T00:02:00Z",
					status: "succeeded",
				}),
			).map((entry) => entry.label),
		).toEqual(["Created", "Finished"]);
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
		expect(parseTaskResult(createTask())).toBeNull();
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
		expect(
			parseTaskResult(
				createTask({
					kind: "offline_download",
					result: {
						content_length: 512,
						file_id: 70,
						file_name: "file.bin",
						file_path: "/Incoming/file.bin",
						folder_id: 9,
						kind: "offline_download",
						sha256: "abc123",
						source_display_url: "https://example.com/file.bin",
					},
				}),
			),
		).toEqual({
			target_folder_id: 9,
			target_path: "/Incoming/file.bin",
		});
	});
});
