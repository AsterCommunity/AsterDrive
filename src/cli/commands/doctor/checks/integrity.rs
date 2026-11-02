//! Findings from service-owned consistency audits and deterministic repairs.
use super::super::report::{DoctorCheck, DoctorStatus};
use crate::errors::Result;
use crate::services::ops::integrity;
pub(in crate::cli::commands::doctor) async fn doctor_storage_usage_check(
    db: &sea_orm::DatabaseConnection,
    fix: bool,
) -> Result<DoctorCheck> {
    const SAMPLE_LIMIT: usize = 256;
    let mut drifts = integrity::audit_storage_usage_with_limit(db, SAMPLE_LIMIT).await?;
    let detected = drifts.total;
    let mut fixed = 0usize;

    if fix && detected > 0 {
        fixed = integrity::fix_storage_usage_all(db).await?;
        drifts = integrity::audit_storage_usage_with_limit(db, SAMPLE_LIMIT).await?;
    }

    if drifts.total == 0 {
        let summary = if fixed > 0 {
            format!("fixed {fixed} storage usage mismatch(es)")
        } else {
            "storage usage counters match logical file sizes".to_string()
        };
        let mut details = Vec::new();
        if detected > 0 {
            details.push(format!("detected_before_fix={detected}"));
        }
        return Ok(DoctorCheck {
            name: "storage_usage_consistency",
            label: "Storage usage counters",
            status: DoctorStatus::Ok,
            summary,
            details,
            suggestion: None,
        });
    }

    let details = drifts
        .samples
        .into_iter()
        .map(|drift| {
            format!(
                "{}#{} recorded={} actual={} delta={}",
                match drift.owner_kind {
                    integrity::StorageOwnerKind::User => "user",
                    integrity::StorageOwnerKind::Team => "team",
                },
                drift.owner_id,
                drift.recorded_bytes,
                drift.actual_bytes,
                drift.delta_bytes
            )
        })
        .collect();

    Ok(DoctorCheck {
        name: "storage_usage_consistency",
        label: "Storage usage counters",
        status: DoctorStatus::Warn,
        summary: format!("{} storage usage mismatch(es)", detected.max(fixed)),
        details,
        suggestion: Some(
            "Run doctor --deep --fix to write back users.storage_used and teams.storage_used."
                .to_string(),
        ),
    })
}

pub(in crate::cli::commands::doctor) async fn doctor_blob_ref_count_check(
    db: &sea_orm::DatabaseConnection,
    fix: bool,
    policy_id: Option<i64>,
) -> Result<DoctorCheck> {
    const SAMPLE_LIMIT: usize = 256;
    let mut drifts =
        integrity::audit_blob_ref_counts_with_limit(db, policy_id, SAMPLE_LIMIT).await?;
    let detected = drifts.total;
    let mut fixed = 0usize;

    if fix && detected > 0 {
        fixed = integrity::fix_blob_ref_counts_all(db, policy_id).await?;
        drifts = integrity::audit_blob_ref_counts_with_limit(db, policy_id, SAMPLE_LIMIT).await?;
    }

    if drifts.total == 0 {
        let summary = if fixed > 0 {
            format!("fixed {fixed} blob ref_count mismatch(es)")
        } else {
            if let Some(policy_id) = policy_id {
                format!("blob ref_count values match file references for policy #{policy_id}")
            } else {
                "blob ref_count values match file references".to_string()
            }
        };
        let mut details = Vec::new();
        if detected > 0 {
            details.push(format!("detected_before_fix={detected}"));
        }
        return Ok(DoctorCheck {
            name: "blob_ref_counts",
            label: "Blob reference counters",
            status: DoctorStatus::Ok,
            summary,
            details,
            suggestion: None,
        });
    }

    let details = drifts
        .samples
        .into_iter()
        .map(|drift| {
            format!(
                "blob#{} recorded={} actual={} policy_id={} path={:?}",
                drift.blob_id,
                drift.recorded_ref_count,
                drift.actual_ref_count,
                drift.policy_id,
                drift.storage_path
            )
        })
        .collect();

    Ok(DoctorCheck {
        name: "blob_ref_counts",
        label: "Blob reference counters",
        status: DoctorStatus::Warn,
        summary: match policy_id {
            Some(policy_id) => format!(
                "{} blob ref_count mismatch(es) for policy #{}",
                detected.max(fixed),
                policy_id
            ),
            None => format!("{} blob ref_count mismatch(es)", detected.max(fixed)),
        },
        details,
        suggestion: Some("Run doctor --deep --fix to write back file_blobs.ref_count.".to_string()),
    })
}

pub(in crate::cli::commands::doctor) async fn doctor_folder_tree_check(
    db: &sea_orm::DatabaseConnection,
) -> Result<DoctorCheck> {
    let issues = integrity::audit_folder_tree_with_limit(db, 256).await?;
    if issues.total == 0 {
        return Ok(DoctorCheck {
            name: "folder_tree_integrity",
            label: "Folder tree integrity",
            status: DoctorStatus::Ok,
            summary: "folder parent chains are internally consistent".to_string(),
            details: Vec::new(),
            suggestion: None,
        });
    }

    let has_cycle = issues
        .samples
        .iter()
        .any(|issue| issue.kind == integrity::FolderTreeIssueKind::Cycle);
    let samples_truncated = issues.samples.len() < issues.total;
    let details = issues
        .samples
        .into_iter()
        .map(|issue| {
            format!(
                "{} folder#{} {}",
                match issue.kind {
                    integrity::FolderTreeIssueKind::MissingParent => "missing_parent",
                    integrity::FolderTreeIssueKind::CrossScopeParent => "cross_scope_parent",
                    integrity::FolderTreeIssueKind::Cycle => "cycle",
                },
                issue.folder_id,
                issue.detail
            )
        })
        .collect();

    Ok(DoctorCheck {
        name: "folder_tree_integrity",
        label: "Folder tree integrity",
        status: DoctorStatus::Fail,
        summary: format!(
            "{} folder issue(s) detected ({}); showing up to 256 samples",
            issues.total,
            if samples_truncated {
                "sampled issue types (full type breakdown unavailable)"
            } else if has_cycle {
                "including cycles"
            } else {
                "invalid parent references"
            }
        ),
        details,
        suggestion: Some(
            "Fix dangling parent_id values or folder cycles before continuing with bulk move or delete operations."
                .to_string(),
        ),
    })
}
