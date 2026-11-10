//! Diagnostics contain identifiers and counts, never raw product rows or request bodies.

use aster_drive_model::entities::{audit_log, file, file_blob, user};
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Violation {
    pub invariant: &'static str,
    pub entity_id: i64,
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub files: usize,
    pub blobs: usize,
    pub quota_used: i64,
    pub audit_records: usize,
    pub violations: Vec<Violation>,
}

impl Snapshot {
    pub fn require(&mut self, invariant: &'static str, entity_id: i64, holds: bool) {
        if !holds {
            self.violations.push(Violation {
                invariant,
                entity_id,
            });
        }
    }

    pub fn assert_valid(&self) {
        assert!(
            self.violations.is_empty(),
            "{}",
            serde_json::to_string(self).unwrap()
        );
    }
}

pub async fn inspect(db: &DatabaseConnection, secrets: &[&str]) -> Snapshot {
    let files = file::Entity::find()
        .all(db)
        .await
        .expect("load file invariants");
    let blobs = file_blob::Entity::find()
        .all(db)
        .await
        .expect("load blob invariants");
    let users = user::Entity::find()
        .all(db)
        .await
        .expect("load quota invariants");
    let audits = audit_log::Entity::find()
        .all(db)
        .await
        .expect("load audit invariants");
    let mut snapshot = Snapshot {
        files: files.len(),
        blobs: blobs.len(),
        quota_used: users.iter().map(|u| u.storage_used).sum(),
        audit_records: audits.len(),
        violations: vec![],
    };
    for file in &files {
        snapshot.require(
            "file.references_existing_blob",
            file.id,
            blobs
                .iter()
                .any(|b| b.id == file.blob_id && b.size == file.size),
        );
    }
    for blob in &blobs {
        let refs = aster_drive::db::repository::file_repo::count_blob_refs_from_files_for_blob(
            db, blob.id,
        )
        .await
        .unwrap()
            + aster_drive::db::repository::revision_repo::count_non_current_blob_refs_for_blob(
                db, blob.id,
            )
            .await
            .unwrap();
        snapshot.require(
            "blob.reference_count",
            blob.id,
            i64::from(blob.ref_count) == refs,
        );
        snapshot.require(
            "blob.backing_contract",
            blob.id,
            blob.validate_backing().is_ok(),
        );
    }
    for owner in &users {
        let used: i64 = files
            .iter()
            .filter(|f| f.owner_user_id == Some(owner.id) && f.team_id.is_none())
            .map(|f| f.size)
            .sum();
        snapshot.require("quota.exactly_once", owner.id, used == owner.storage_used);
    }
    for audit in &audits {
        // Never include the serialized row in an error or artifact.
        let raw = serde_json::to_string(audit).unwrap();
        let mut marker_row = audit.clone();
        if let Some(details) = &audit.details
            && let Ok(mut details) = serde_json::from_str::<serde_json::Value>(details)
        {
            // Login records a boolean policy flag, never a password value.
            if details
                .get("password_change_required")
                .is_some_and(serde_json::Value::is_boolean)
            {
                details
                    .as_object_mut()
                    .unwrap()
                    .remove("password_change_required");
            }
            marker_row.details = Some(details.to_string());
        }
        let marker_raw = serde_json::to_string(&marker_row).unwrap();
        snapshot.require(
            "audit.no_secrets",
            audit.id,
            !secrets.iter().any(|s| !s.is_empty() && raw.contains(s))
                && !contains_secret_marker(&marker_raw),
        );
        if audit.entity_type == "file"
            && audit.action == aster_drive_model::types::AuditAction::FileUpload
        {
            snapshot.require(
                "audit.no_premature_success",
                audit.id,
                audit
                    .entity_id
                    .is_some_and(|id| files.iter().any(|f| f.id == id)),
            );
        }
    }
    snapshot
}

fn contains_secret_marker(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    [
        "password",
        "secret",
        "bearer ",
        "access_token",
        "refresh_token",
        "x-amz-signature",
        "upload_url",
        "processing_token",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

pub fn redact_logs(raw: &str, secrets: &[&str]) -> String {
    raw.lines()
        .map(|line| {
            if contains_secret_marker(line)
                || line.contains("://")
                || secrets.iter().any(|s| !s.is_empty() && line.contains(s))
            {
                "[redacted log line]".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn violations_identify_invariant_without_disclosing_secret() {
    let mut snapshot = Snapshot {
        files: 0,
        blobs: 0,
        quota_used: 0,
        audit_records: 0,
        violations: vec![],
    };
    snapshot.require("audit.no_secrets", 42, false);
    let serialized = serde_json::to_string(&snapshot).unwrap();
    assert!(serialized.contains("audit.no_secrets"));
    assert!(serialized.contains("42"));
    let secret = "fixture-sensitive-value";
    let logs = [
        "ready",
        secret,
        "https://provider/upload?token=private",
        "password=unknown",
        "checkpoint reached",
    ]
    .join("\n");
    let redacted = redact_logs(&logs, &[secret]);
    assert!(!redacted.contains(secret));
    assert!(!redacted.contains("provider"));
    assert!(!redacted.contains("unknown"));
    assert!(redacted.contains("checkpoint reached"));
}

#[tokio::test]
async fn persisted_audit_violation_reports_only_invariant_and_identifier() {
    use aster_drive_migration::Migrator;
    use sea_orm::{ActiveModelTrait, Database, Set};
    let db = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    let secret = "sensitive-audit-fixture-value";
    audit_log::ActiveModel {
        user_id: Set(1),
        action: Set(aster_drive_model::types::AuditAction::UserLogin),
        entity_type: Set("auth_session".into()),
        details: Set(Some(r#"{"password_change_required":false}"#.into())),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    inspect(&db, &[secret]).await.assert_valid();
    let row = audit_log::ActiveModel {
        user_id: Set(1),
        action: Set(aster_drive_model::types::AuditAction::FileUpload),
        entity_type: Set("file".into()),
        entity_id: Set(Some(999)),
        details: Set(Some(secret.into())),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let snapshot = inspect(&db, &[secret]).await;
    assert!(
        snapshot
            .violations
            .iter()
            .any(|v| v.invariant == "audit.no_secrets" && v.entity_id == row.id)
    );
    assert!(
        snapshot
            .violations
            .iter()
            .any(|v| v.invariant == "audit.no_premature_success" && v.entity_id == row.id)
    );
    assert!(!serde_json::to_string(&snapshot).unwrap().contains(secret));
}
