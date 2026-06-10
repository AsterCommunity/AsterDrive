//! `doctor --deep` 的存储扫描检查。
//!
//! 这里负责把底层对象存储审计结果转换成 CLI 友好的检查项，
//! 包括已追踪对象缺失、未追踪对象和孤儿缩略图三类问题。

use sea_orm::DatabaseConnection;

use crate::config::{RuntimeConfig, operations};
use crate::errors::Result;
use crate::services::integrity_service::{self, StorageObjectAudit};

use super::{DoctorCheck, DoctorStatus, doctor_check};

/// Runs storage object auditing and maps the findings into doctor checks.
pub(super) async fn doctor_storage_scan_checks(
    db: &DatabaseConnection,
    runtime_config: &RuntimeConfig,
    policy_id: Option<i64>,
) -> Result<Vec<DoctorCheck>> {
    let driver_registry = crate::storage::DriverRegistry::noop();
    let report = integrity_service::audit_storage_objects(
        db,
        &driver_registry,
        policy_id,
        operations::thumbnail_max_dimension(runtime_config),
        operations::image_preview_max_dimension(runtime_config),
    )
    .await?;
    let scan_meta = scan_meta_details(&report);

    Ok(vec![
        tracked_blob_check(&report, policy_id, &scan_meta),
        untracked_storage_check(&report, policy_id, &scan_meta),
        thumbnail_storage_check(&report, policy_id, &scan_meta),
    ])
}

fn scan_meta_details(report: &StorageObjectAudit) -> Vec<String> {
    vec![
        format!("policies={}", report.scanned_policies),
        format!("objects={}", report.scanned_objects),
        format!("ignored_paths={}", report.ignored_paths),
    ]
}

fn tracked_blob_check(
    report: &StorageObjectAudit,
    policy_id: Option<i64>,
    scan_meta: &[String],
) -> DoctorCheck {
    if report.missing_blob_objects.is_empty() {
        return doctor_check(
            "tracked_blob_objects",
            "Tracked blob objects",
            DoctorStatus::Ok,
            match policy_id {
                Some(policy_id) => {
                    format!("all tracked blobs exist in storage for policy #{policy_id}")
                }
                None => "all tracked blobs exist in storage".to_string(),
            },
            scan_meta.to_vec(),
            None,
        );
    }

    let mut details = scan_meta.to_vec();
    details.extend(
        report
            .missing_blob_objects
            .iter()
            .map(|issue| match issue.blob_id {
                Some(blob_id) => format!(
                    "blob#{} policy_id={} missing path={}",
                    blob_id, issue.policy_id, issue.path
                ),
                None => format!("policy_id={} missing path={}", issue.policy_id, issue.path),
            }),
    );

    doctor_check(
        "tracked_blob_objects",
        "Tracked blob objects",
        DoctorStatus::Fail,
        match policy_id {
            Some(policy_id) => format!(
                "{} tracked blob object(s) are missing from storage for policy #{}",
                report.missing_blob_objects.len(),
                policy_id
            ),
            None => format!(
                "{} tracked blob object(s) are missing from storage",
                report.missing_blob_objects.len()
            ),
        },
        details,
        Some(if report.scanned_objects == 0 {
            "No storage objects were listed. Check the storage policy base path / bucket / prefix first; for local policies, relative base_path values are resolved from the current working directory.".to_string()
        } else {
            "Check for missing objects in the underlying storage, bad migrations, or manual file deletion.".to_string()
        }),
    )
}

fn untracked_storage_check(
    report: &StorageObjectAudit,
    policy_id: Option<i64>,
    scan_meta: &[String],
) -> DoctorCheck {
    if report.untracked_objects.is_empty() {
        return doctor_check(
            "untracked_storage_objects",
            "Untracked storage objects",
            DoctorStatus::Ok,
            match policy_id {
                Some(policy_id) => {
                    format!("no extra storage objects were found for policy #{policy_id}")
                }
                None => "no extra storage objects were found".to_string(),
            },
            scan_meta.to_vec(),
            None,
        );
    }

    let mut details = scan_meta.to_vec();
    details.extend(report.untracked_objects.iter().map(|issue| {
        format!(
            "policy_id={} untracked path={}",
            issue.policy_id, issue.path
        )
    }));

    doctor_check(
        "untracked_storage_objects",
        "Untracked storage objects",
        DoctorStatus::Warn,
        match policy_id {
            Some(policy_id) => format!(
                "{} untracked storage object(s) were found for policy #{}",
                report.untracked_objects.len(),
                policy_id
            ),
            None => format!(
                "{} untracked storage object(s) were found",
                report.untracked_objects.len()
            ),
        },
        details,
        Some(
            "Clean up orphaned storage objects if they are not expected temporary artifacts."
                .to_string(),
        ),
    )
}

fn thumbnail_storage_check(
    report: &StorageObjectAudit,
    policy_id: Option<i64>,
    scan_meta: &[String],
) -> DoctorCheck {
    if report.orphan_thumbnails.is_empty() {
        return doctor_check(
            "thumbnail_objects",
            "Thumbnail objects",
            DoctorStatus::Ok,
            match policy_id {
                Some(policy_id) => {
                    format!("no orphan thumbnails were found for policy #{policy_id}")
                }
                None => "no orphan thumbnails were found".to_string(),
            },
            scan_meta.to_vec(),
            None,
        );
    }

    let mut details = scan_meta.to_vec();
    details.extend(report.orphan_thumbnails.iter().map(|issue| {
        format!(
            "policy_id={} orphan thumbnail={}",
            issue.policy_id, issue.path
        )
    }));

    doctor_check(
        "thumbnail_objects",
        "Thumbnail objects",
        DoctorStatus::Warn,
        match policy_id {
            Some(policy_id) => format!(
                "{} orphan thumbnail(s) were found for policy #{}",
                report.orphan_thumbnails.len(),
                policy_id
            ),
            None => format!(
                "{} orphan thumbnail(s) were found",
                report.orphan_thumbnails.len()
            ),
        },
        details,
        Some(
            "Delete orphan thumbnails if they are not expected leftover cache artifacts."
                .to_string(),
        ),
    )
}
