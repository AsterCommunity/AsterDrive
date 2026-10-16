//! Preserve unconfirmed audit requests for operator reconciliation on failed drain.

use std::{io, path::Path};

use aster_forge_audit::AuditWriter;
use tokio::io::AsyncWriteExt;

/// The owned runtime has completed cleanup before this is called. Never replay these
/// requests automatically: an interrupted commit may already be present in the DB.
pub(super) async fn finish_audit(
    result: io::Result<()>,
    writer: &AuditWriter,
    directory: &Path,
) -> io::Result<()> {
    // Also covers validation/startup failures before the owner component started.
    let report = writer.stop(tokio::time::Instant::now()).await;
    let remaining = report.snapshot.queued + report.snapshot.direct_pending;
    if remaining == 0 {
        return result;
    }
    match preserve_unconfirmed(writer, directory).await {
        Ok(path) => {
            tracing::error!(remaining, path = %path.display(),
                "audit drain incomplete; reconcile recovery file against database before replay");
            Err(io::Error::other(format!(
                "{}; {remaining} unconfirmed audit records saved to {} for reconciliation",
                result
                    .err()
                    .map_or_else(|| "audit drain failed".into(), |error| error.to_string()),
                path.display(),
            )))
        }
        Err(error) => {
            tracing::error!(remaining, %error, "failed to preserve unconfirmed audit records");
            Err(io::Error::other(format!(
                "{}; failed to preserve {remaining} unconfirmed audit records: {error}",
                result
                    .err()
                    .map_or_else(|| "audit drain failed".into(), |error| error.to_string()),
            )))
        }
    }
}

async fn preserve_unconfirmed(
    writer: &AuditWriter,
    directory: &Path,
) -> io::Result<std::path::PathBuf> {
    let snapshot = writer.snapshot();
    let records: Vec<_> = writer
        .unconfirmed()
        .into_iter()
        .map(|request| {
            serde_json::json!({
                "user_id": request.user_id,
                "action": request.action,
                "entity_type": request.entity_type,
                "entity_id": request.entity_id,
                "entity_name": request.entity_name,
                "details": request.details,
                "ip_address": request.ip_address,
                "user_agent": request.user_agent,
                "created_at": request.created_at,
            })
        })
        .collect();
    // Raw driver errors may contain SQL values. Persist only safe classification metadata.
    let payload = serde_json::json!({
        "version": 1,
        "requires_database_reconciliation": true,
        "queued": snapshot.queued,
        "direct_pending": snapshot.direct_pending,
        "batch_failure_records": snapshot.batch_failure_records,
        "batch_failure_class": snapshot.batch_failure.map(|failure| format!("{:?}", failure.class)),
        "direct_failure_class": snapshot.direct_failure.map(|failure| format!("{:?}", failure.class)),
        "records": records,
    }).to_string();
    let mut builder = tokio::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(directory).await?;
    let id = uuid::Uuid::new_v4();
    let temporary_path = directory.join(format!("audit-{id}.json.tmp"));
    let path = directory.join(format!("audit-{id}.json"));
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temporary_path).await?;
    let write_result = async {
        file.write_all(payload.as_bytes()).await?;
        file.sync_all().await
    }
    .await;
    drop(file);
    if let Err(error) = write_result {
        let _ = tokio::fs::remove_file(&temporary_path).await;
        return Err(error);
    }
    if let Err(error) = tokio::fs::rename(&temporary_path, &path).await {
        let _ = tokio::fs::remove_file(&temporary_path).await;
        return Err(error);
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_forge_db::AuditLogCreate;
    use aster_forge_runtime::{BufferedWriterState, WriteFailureClass};

    async fn failed_writer() -> AuditWriter {
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        let writer = AuditWriter::new(db);
        writer.start().await.unwrap();
        writer
            .record(AuditLogCreate {
                user_id: 0,
                action: "server_shutdown".into(),
                entity_type: "system_config".into(),
                entity_id: None,
                entity_name: Some("original name".into()),
                details: Some(r#"{"safe":true}"#.into()),
                ip_address: None,
                user_agent: None,
                created_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
        assert!(
            writer
                .flush(tokio::time::Instant::now() + std::time::Duration::from_secs(5))
                .await
                .is_err()
        );
        writer
    }

    #[tokio::test]
    async fn failed_drain_preserves_originals_and_classification_in_private_unique_files() {
        let directory = aster_forge_test::temp::TestTempDir::new("audit-recovery");
        let writer = failed_writer().await;
        for _ in 0..2 {
            let error = finish_audit(
                Err(io::Error::other("runtime drain failed")),
                &writer,
                directory.path(),
            )
            .await
            .unwrap_err();
            assert!(error.to_string().contains("runtime drain failed"));
            assert!(
                error
                    .to_string()
                    .contains("1 unconfirmed audit records saved")
            );
        }
        let snapshot = writer.snapshot();
        assert_eq!(snapshot.state, BufferedWriterState::Stopped);
        assert_eq!(
            snapshot.batch_failure.unwrap().class,
            WriteFailureClass::Permanent
        );
        let files: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|file| file.unwrap().path())
            .collect();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        }));
        for path in files {
            let saved: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(saved["requires_database_reconciliation"], true);
            assert_eq!(saved["batch_failure_class"], "Permanent");
            assert_eq!(saved["records"][0]["entity_name"], "original name");
            assert_eq!(saved["records"][0]["details"], r#"{"safe":true}"#);
            assert_eq!(
                saved["records"][0]["action"],
                writer.unconfirmed()[0].action
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
    }

    #[tokio::test]
    async fn recovery_io_failure_is_reported_without_losing_in_memory_originals() {
        let directory = aster_forge_test::temp::TestTempDir::new("audit-recovery");
        let blocked = directory.path().join("blocked");
        std::fs::write(&blocked, "existing data").unwrap();
        let writer = failed_writer().await;
        let error = finish_audit(
            Err(io::Error::other("original runtime error")),
            &writer,
            &blocked,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("original runtime error"));
        assert!(
            error
                .to_string()
                .contains("failed to preserve 1 unconfirmed")
        );
        assert_eq!(writer.unconfirmed().len(), 1);
        assert_eq!(std::fs::read_to_string(blocked).unwrap(), "existing data");
    }

    #[tokio::test]
    async fn clean_or_unstarted_writer_preserves_result_without_creating_recovery_directory() {
        let directory = aster_forge_test::temp::TestTempDir::new("audit-recovery");
        let path = directory.path().join("unused");
        let writer = AuditWriter::new(sea_orm::Database::connect("sqlite::memory:").await.unwrap());
        assert!(finish_audit(Ok(()), &writer, &path).await.is_ok());
        assert_eq!(
            finish_audit(Err(io::Error::other("product error")), &writer, &path)
                .await
                .unwrap_err()
                .to_string(),
            "product error"
        );
        assert!(!path.exists());
    }
}
