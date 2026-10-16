//! Drive policy at the explicitly owned Forge audit boundary.

use crate::common;
use actix_web::test;
use aster_drive::{api::pagination::AdminAuditLogSortBy, services::ops::audit};
use aster_drive_model::entities::audit_log;
use aster_forge_api::SortOrder;
use aster_forge_audit::{AuditLogBufferConfig, AuditWriter};
use aster_forge_runtime::{BufferedWriterState, OverflowPolicy};
use sea_orm::{ConnectionTrait, EntityTrait, PaginatorTrait};
use std::{sync::Arc, time::Duration};
use tokio::time::Instant;

async fn record(state: &aster_drive::runtime::PrimaryAppState) {
    audit::log_with_details(
        state,
        &audit::AuditContext::system(),
        audit::AuditAction::FileUpload,
        audit::AuditEntityType::File,
        Some(7),
        Some("report.txt"),
        || Some(serde_json::json!({"size": 12})),
    )
    .await;
}

#[tokio::test]
async fn writers_and_follower_views_have_explicit_instance_ownership() {
    let first = common::setup().await;
    let second = common::setup().await;
    assert!(!Arc::ptr_eq(&first.audit_writer, &second.audit_writer));
    assert!(Arc::ptr_eq(
        &first.audit_writer,
        &first.follower_view().audit_writer
    ));
    assert_eq!(
        first.audit_writer.start().await,
        Err(BufferedWriterState::Running)
    );
    record(&first).await;
    assert_eq!(first.audit_writer.snapshot().queued, 1);
    assert_eq!(second.audit_writer.snapshot().queued, 0);
    audit::flush(&second).await.unwrap();
    assert_eq!(
        audit_log::Entity::find()
            .count(second.writer_db())
            .await
            .unwrap(),
        0
    );
    audit::flush(&first).await.unwrap();
    record(&first).await;
    audit::flush(&first).await.unwrap();
    assert_eq!(
        audit_log::Entity::find()
            .count(first.writer_db())
            .await
            .unwrap(),
        2
    );
    first
        .audit_writer
        .stop(Instant::now() + Duration::from_secs(5))
        .await;
    record(&first).await;
    assert_eq!(first.audit_writer.snapshot().persisted, 2);
    record(&second).await;
    audit::flush(&second).await.unwrap();
    assert_eq!(second.audit_writer.snapshot().persisted, 1);
    second
        .audit_writer
        .stop(Instant::now() + Duration::from_secs(5))
        .await;
}

#[actix_web::test]
async fn best_effort_admission_rejection_preserves_folder_creation() {
    let mut state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    audit::flush(&state).await.unwrap();
    state
        .audit_writer
        .stop(Instant::now() + Duration::from_secs(5))
        .await;
    // A one-record rejecting buffer exercises Full as well as NotRunning/Stopped.
    let writer = Arc::new(AuditWriter::with_config(
        state.writer_db().clone(),
        AuditLogBufferConfig::new(1, 100, Duration::from_secs(3600)),
        OverflowPolicy::Reject,
        None,
    ));
    for running in [false, true] {
        state.audit_writer = writer.clone();
        if running {
            writer.start().await.unwrap();
            record(&state).await; // Fill the sole queue slot before the product mutation.
        }
        let app = create_test_app!(state.clone());
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/folders")
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(serde_json::json!({"name": format!("best-effort-{running}")}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 201);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert!(body["data"]["id"].as_i64().is_some());
        assert_eq!(writer.snapshot().queued, usize::from(running));
    }
    let report = writer.stop(Instant::now() + Duration::from_secs(5)).await;
    assert_eq!(report.snapshot.persisted, 1);
}

#[tokio::test]
async fn query_export_and_cleanup_reject_incomplete_persistence_without_exposing_payload() {
    let mut state = common::setup().await;
    state.audit_writer.stop(Instant::now()).await;
    // A deterministic DB-level rejection is portable across supported databases.
    state
        .writer_db()
        .execute_unprepared("DROP TABLE audit_logs")
        .await
        .unwrap();
    state.audit_writer = Arc::new(AuditWriter::new(state.writer_db().clone()));
    state.audit_writer.start().await.unwrap();
    record(&state).await;
    let error = audit::query(
        &state,
        Default::default(),
        10,
        0,
        AdminAuditLogSortBy::CreatedAt,
        SortOrder::Desc,
    )
    .await
    .unwrap_err();
    assert_eq!(error.message(), "audit persistence is incomplete");
    let export = audit::prepare_csv_export(
        state.clone(),
        audit::AuditExportKind::System,
        Default::default(),
        AdminAuditLogSortBy::CreatedAt,
        SortOrder::Desc,
    )
    .await;
    assert!(export.is_err());
    assert!(audit::cleanup_expired(&state).await.is_err());
    record(&state).await; // Best effort still returns normally; no silent replay occurs.
    let report = state
        .audit_writer
        .stop(Instant::now() + Duration::from_secs(5))
        .await;
    assert_eq!(report.snapshot.state, BufferedWriterState::Stopped);
    assert_eq!(state.audit_writer.unconfirmed().len(), 2);
    assert_eq!(report.snapshot.persisted, 0);
}

#[tokio::test]
async fn mandatory_audit_failure_rolls_back_the_product_transaction() {
    use sea_orm::TransactionTrait;
    let state = common::setup().await;
    state
        .writer_db()
        .execute_unprepared("CREATE TABLE audit_atomic_probe (id BIGINT PRIMARY KEY)")
        .await
        .unwrap();
    let transaction = state.writer_db().begin().await.unwrap();
    transaction
        .execute_unprepared("INSERT INTO audit_atomic_probe (id) VALUES (1)")
        .await
        .unwrap();
    // Invalid request is rejected by the DB helper, without admission into memory.
    let error = audit::log_with_transaction(
        &transaction,
        &state.runtime_config,
        audit::AuditLogInput {
            ctx: &audit::AuditContext::system(),
            action: audit::AuditAction::FileUpload,
            entity_type: audit::AuditEntityType::File,
            entity_id: Some(1),
            entity_name: Some(&"x".repeat(256)),
        },
        || None,
    )
    .await;
    assert!(error.is_err());
    transaction.rollback().await.unwrap();
    assert!(
        state
            .writer_db()
            .query_one_raw(sea_orm::Statement::from_string(
                state.writer_db().get_database_backend(),
                "SELECT id FROM audit_atomic_probe".to_string()
            ))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(state.audit_writer.snapshot().queued, 0);
    state.audit_writer.stop(Instant::now()).await;
}

#[tokio::test]
async fn direct_failure_stays_owned_and_does_not_replay_successful_batch_records() {
    use aster_forge_runtime::WriteFailureClass;
    let mut state = common::setup().await;
    state.audit_writer.stop(Instant::now()).await;
    state.audit_writer = Arc::new(AuditWriter::with_config(
        state.writer_db().clone(),
        AuditLogBufferConfig::new(1, 100, Duration::from_secs(3600)),
        OverflowPolicy::DirectWrite,
        None,
    ));
    state.audit_writer.start().await.unwrap();
    record(&state).await;
    // Overflow uses the direct lane. Validation fails, but best effort returns normally.
    audit::log(
        &state,
        &audit::AuditContext::system(),
        audit::AuditAction::FileUpload,
        audit::AuditEntityType::File,
        Some(8),
        Some(&"x".repeat(256)),
        None,
    )
    .await;
    let snapshot = state.audit_writer.snapshot();
    assert_eq!(snapshot.queued, 1);
    assert_eq!(snapshot.direct_pending, 1);
    assert_eq!(
        snapshot.direct_failure.unwrap().class,
        WriteFailureClass::Permanent
    );
    record(&state).await; // Both bounded slots are occupied: reject without a DB fallback.
    let report = state
        .audit_writer
        .stop(Instant::now() + Duration::from_secs(5))
        .await;
    assert_eq!(report.snapshot.persisted, 1);
    assert_eq!(report.snapshot.queued, 0);
    assert_eq!(report.snapshot.direct_pending, 1);
    assert_eq!(state.audit_writer.unconfirmed()[0].entity_id, Some(8));
    state.audit_writer.retry_failed();
    state.audit_writer.stop(Instant::now()).await;
    assert_eq!(
        audit_log::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn query_and_export_deadlines_leave_records_owned_and_join_before_database_release() {
    use sea_orm::TransactionTrait;
    let state = common::setup().await;
    if state.writer_db().get_database_backend() != sea_orm::DbBackend::Sqlite {
        return; // This injection specifically holds the sole SQLite pool connection.
    }
    let transaction = state.writer_db().begin().await.unwrap();
    record(&state).await;
    tokio::time::pause();
    let start = Instant::now();
    let error = audit::query(
        &state,
        Default::default(),
        10,
        0,
        AdminAuditLogSortBy::CreatedAt,
        SortOrder::Desc,
    )
    .await
    .unwrap_err();
    assert_eq!(error.message(), "audit persistence is incomplete");
    assert!(Instant::now().duration_since(start) >= Duration::from_secs(5));
    assert!(
        audit::prepare_csv_export(
            state.clone(),
            audit::AuditExportKind::System,
            Default::default(),
            AdminAuditLogSortBy::CreatedAt,
            SortOrder::Desc
        )
        .await
        .is_err()
    );
    let report = state.audit_writer.stop(Instant::now()).await;
    assert_eq!(report.snapshot.state, BufferedWriterState::Stopped);
    assert_eq!(state.audit_writer.unconfirmed().len(), 1);
    tokio::time::resume();
    transaction.rollback().await.unwrap();
    assert_eq!(
        audit_log::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn best_effort_failures_log_safe_action_and_reason_without_request_details() {
    use tracing::instrument::WithSubscriber;
    let mut state = common::setup().await;
    state.audit_writer.stop(Instant::now()).await;
    state.audit_writer = Arc::new(AuditWriter::with_config(
        state.writer_db().clone(),
        AuditLogBufferConfig::new(1, 100, Duration::from_secs(3600)),
        OverflowPolicy::Reject,
        None,
    ));
    let directory = aster_forge_test::temp::TestTempDir::new("audit-failure-logs");
    let path = directory.join("events.log");
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(std::fs::File::create(&path).unwrap()))
        .finish();
    let sensitive = "DO_NOT_LOG_AUDIT_REQUEST".repeat(32);
    async {
        record(&state).await; // NotRunning returns the unaccepted request to best effort.
        state.audit_writer.start().await.unwrap();
        record(&state).await;
        record(&state).await; // Full also rejects without direct fallback.
        audit::log_with_db_and_config(
            state.writer_db(),
            &state.runtime_config,
            audit::AuditLogInput {
                ctx: &audit::AuditContext::system(),
                action: audit::AuditAction::FileUpload,
                entity_type: audit::AuditEntityType::File,
                entity_id: Some(1),
                entity_name: Some(&sensitive),
            },
            || Some(serde_json::json!({"password": sensitive})),
        )
        .await;
    }
    .with_subscriber(subscriber)
    .await;
    let logs = std::fs::read_to_string(path).unwrap();
    for expected in [
        "writer not running",
        "writer full",
        "best-effort direct audit write failed",
        "file_upload",
    ] {
        assert!(
            logs.contains(expected),
            "missing diagnostic {expected}: {logs}"
        );
    }
    assert!(!logs.contains("DO_NOT_LOG_AUDIT_REQUEST"));
    let report = state
        .audit_writer
        .stop(Instant::now() + Duration::from_secs(5))
        .await;
    assert_eq!(report.snapshot.persisted, 1);
    assert_eq!(
        audit_log::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1
    );
}
