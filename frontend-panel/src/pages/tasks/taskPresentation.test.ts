import { describe, expect, it } from "vitest";
import type { TaskInfo } from "@/types/api";
import {
	buildTaskTimeline,
	currentTaskStep,
	formatProgressCounts,
	formatTaskDisplayName,
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
