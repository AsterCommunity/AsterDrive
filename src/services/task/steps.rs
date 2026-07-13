//! Background task step keys and persisted JSON boundary.

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::types::StoredTaskSteps;
use aster_forge_tasks::TaskStepInfo;

pub(super) const TASK_STEP_WAITING: &str = "waiting";
pub(super) const TASK_STEP_PREPARE_SOURCES: &str = "prepare_sources";
pub(super) const TASK_STEP_BUILD_ARCHIVE: &str = "build_archive";
pub(super) const TASK_STEP_STORE_RESULT: &str = "store_result";
pub(super) const TASK_STEP_VALIDATE_SOURCE: &str = "validate_source";
pub(super) const TASK_STEP_DOWNLOAD_SOURCE: &str = "download_source";
pub(super) const TASK_STEP_VERIFY_SOURCE: &str = "verify_source";
pub(super) const TASK_STEP_EXTRACT_ARCHIVE: &str = "extract_archive";
pub(super) const TASK_STEP_IMPORT_RESULT: &str = "import_result";
pub(super) const TASK_STEP_SCAN_ARCHIVE: &str = "scan_archive";
pub(super) const TASK_STEP_PERSIST_MANIFEST: &str = "persist_manifest";
pub(super) const TASK_STEP_INSPECT_SOURCE: &str = "inspect_source";
pub(super) const TASK_STEP_RENDER_THUMBNAIL: &str = "render_thumbnail";
pub(super) const TASK_STEP_PERSIST_THUMBNAIL: &str = "persist_thumbnail";
pub(super) const TASK_STEP_EXTRACT_METADATA: &str = "extract_metadata";
pub(super) const TASK_STEP_PERSIST_METADATA: &str = "persist_metadata";
pub(super) const TASK_STEP_CLEANUP_OBJECTS: &str = "cleanup_objects";
pub(super) const TASK_STEP_PURGE_TRASH: &str = "purge_trash";
pub(super) const TASK_STEP_SCAN_BLOBS: &str = "scan_blobs";
pub(super) const TASK_STEP_MIGRATE_BLOBS: &str = "migrate_blobs";
pub(super) const TASK_STEP_CHECK_BLOBS: &str = "check_blobs";
pub(super) const TASK_STEP_RECONCILE_REFS: &str = "reconcile_refs";
pub(super) const TASK_STEP_FINISH: &str = "finish";

pub(super) fn parse_task_steps_json(steps_json: Option<&str>) -> Result<Vec<TaskStepInfo>> {
    match steps_json {
        Some(raw) if !raw.trim().is_empty() => serde_json::from_str(raw)
            .map_aster_err_ctx("parse task steps json", AsterError::internal_error),
        _ => Ok(Vec::new()),
    }
}

pub(super) fn serialize_task_steps(steps: &[TaskStepInfo]) -> Result<StoredTaskSteps> {
    serde_json::to_string(steps)
        .map(StoredTaskSteps)
        .map_aster_err_ctx("serialize task steps", AsterError::internal_error)
}

#[cfg(test)]
mod tests {
    use super::{parse_task_steps_json, serialize_task_steps};
    use crate::errors::AsterError;
    use aster_forge_tasks::{
        TaskStepInfo, TaskStepSpec, TaskStepStatus, initial_task_steps_from_specs,
        mark_active_step_failed, set_task_step_active, set_task_step_skipped,
        set_task_step_succeeded,
    };

    fn step(key: &str, status: TaskStepStatus) -> TaskStepInfo {
        TaskStepInfo {
            key: key.to_string(),
            title: key.to_string(),
            status,
            progress_current: 2,
            progress_total: 5,
            detail: Some("detail".to_string()),
            started_at: None,
            finished_at: None,
        }
    }

    #[test]
    fn parse_steps_json_accepts_missing_blank_and_valid_json() {
        assert!(parse_task_steps_json(None).unwrap().is_empty());
        assert!(parse_task_steps_json(Some(" \n\t ")).unwrap().is_empty());

        let stored = serialize_task_steps(&[step("prepare", TaskStepStatus::Succeeded)]).unwrap();
        let parsed = parse_task_steps_json(Some(stored.as_ref())).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].key, "prepare");
        assert_eq!(parsed[0].status, TaskStepStatus::Succeeded);
        assert_eq!(parsed[0].progress_current, 2);
        assert_eq!(parsed[0].detail.as_deref(), Some("detail"));
    }

    #[test]
    fn parse_steps_json_preserves_forge_snake_case_statuses() {
        for (json_status, expected) in [
            ("pending", TaskStepStatus::Pending),
            ("active", TaskStepStatus::Active),
            ("succeeded", TaskStepStatus::Succeeded),
            ("failed", TaskStepStatus::Failed),
            ("skipped", TaskStepStatus::Skipped),
            ("canceled", TaskStepStatus::Canceled),
        ] {
            let json = format!(
                r#"[{{"key":"step","title":"Step","status":"{json_status}","progress_current":0,"progress_total":0,"detail":null,"started_at":null,"finished_at":null}}]"#
            );
            let parsed = parse_task_steps_json(Some(&json)).unwrap();
            assert_eq!(parsed[0].status, expected);
        }
    }

    #[test]
    fn parse_steps_json_maps_invalid_json_to_product_error() {
        let error = parse_task_steps_json(Some("not json")).unwrap_err();
        assert!(matches!(error, AsterError::InternalError(_)));
        assert!(error.message().contains("parse task steps json"));
    }

    #[test]
    fn forge_step_helpers_integrate_with_product_error_mapping() {
        let mut steps = initial_task_steps_from_specs(&[
            TaskStepSpec {
                key: "prepare",
                title: "Prepare",
            },
            TaskStepSpec {
                key: "finish",
                title: "Finish",
            },
        ]);
        assert_eq!(steps[0].status, TaskStepStatus::Active);
        assert_eq!(steps[1].status, TaskStepStatus::Pending);

        let error = set_task_step_active(&mut steps, "missing", None, None)
            .map_err(AsterError::from)
            .unwrap_err();
        assert!(matches!(error, AsterError::InternalError(_)));
        assert!(error.message().contains("task step 'missing' not found"));
    }

    #[test]
    fn forge_step_mutations_round_trip_through_product_persistence() {
        let mut steps = initial_task_steps_from_specs(&[
            TaskStepSpec {
                key: "prepare",
                title: "Prepare",
            },
            TaskStepSpec {
                key: "optional",
                title: "Optional",
            },
            TaskStepSpec {
                key: "finish",
                title: "Finish",
            },
        ]);

        set_task_step_active(&mut steps, "prepare", Some("running"), Some((2, 5))).unwrap();
        set_task_step_succeeded(&mut steps, "prepare", Some("done"), None).unwrap();
        set_task_step_skipped(&mut steps, "optional", Some("not needed")).unwrap();
        set_task_step_active(&mut steps, "finish", Some("finishing"), None).unwrap();
        mark_active_step_failed(&mut steps, Some("failed"));

        let stored = serialize_task_steps(&steps).unwrap();
        let parsed = parse_task_steps_json(Some(stored.as_ref())).unwrap();
        assert_eq!(parsed[0].status, TaskStepStatus::Succeeded);
        assert_eq!(parsed[0].progress_current, 5);
        assert!(parsed[0].started_at.is_some());
        assert!(parsed[0].finished_at.is_some());
        assert_eq!(parsed[1].status, TaskStepStatus::Skipped);
        assert_eq!(parsed[1].detail.as_deref(), Some("not needed"));
        assert!(parsed[1].finished_at.is_some());
        assert_eq!(parsed[2].status, TaskStepStatus::Failed);
        assert_eq!(parsed[2].detail.as_deref(), Some("failed"));
        assert!(parsed[2].started_at.is_some());
        assert!(parsed[2].finished_at.is_some());
    }

    #[test]
    fn forge_failure_marker_falls_back_to_last_pending_step() {
        let mut steps = vec![
            step("first", TaskStepStatus::Succeeded),
            step("second", TaskStepStatus::Pending),
            step("third", TaskStepStatus::Pending),
        ];

        mark_active_step_failed(&mut steps, Some("worker failed before activation"));

        assert_eq!(steps[1].status, TaskStepStatus::Pending);
        assert_eq!(steps[2].status, TaskStepStatus::Failed);
        assert_eq!(
            steps[2].detail.as_deref(),
            Some("worker failed before activation")
        );
        assert!(steps[2].started_at.is_some());
        assert!(steps[2].finished_at.is_some());
    }
}
