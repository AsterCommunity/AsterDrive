//! Periodic projections. Scrape only reads the metrics provider's in-memory state.

use crate::{
    db::repository::observability_repo,
    errors::Result,
    runtime::{PrimaryAppState, SharedRuntimeState},
};
use aster_drive_model::types::{UploadSessionKind, UploadSessionStatus};
use chrono::Utc;
use sea_orm::Iterable;

pub(crate) async fn sample_primary(state: &PrimaryAppState) {
    if !state.metrics().enabled() {
        return;
    }
    let now = Utc::now();
    let result: Result<()> = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let uploads = observability_repo::upload_backlog(state.writer_db(), now).await?;
        let blobs = observability_repo::blob_backlog(state.writer_db()).await?;
        let nodes = observability_repo::remote_node_counts(state.writer_db()).await?;
        let metrics = state.metrics();
        let mut grouped =
            std::collections::BTreeMap::<(&'static str, &'static str), (u64, f64)>::new();
        let mut reserved = 0_u64;
        let mut staged = 0_u64;
        for row in uploads {
            let status = if row.expired {
                "expired"
            } else {
                match row.status {
                    UploadSessionStatus::Uploading => "uploading",
                    UploadSessionStatus::Assembling => "assembling",
                    UploadSessionStatus::Failed => "failed",
                    UploadSessionStatus::Presigned => "presigned",
                    UploadSessionStatus::Completed => continue,
                }
            };
            let count = u64::try_from(row.count)
                .map_err(|_| crate::errors::AsterError::internal_error("negative upload count"))?;
            let age = row
                .oldest
                .map_or(0.0, |created| (now - created).num_seconds().max(0) as f64);
            let group = grouped
                .entry((row.session_kind.as_str(), status))
                .or_default();
            group.0 = group.0.saturating_add(count);
            group.1 = group.1.max(age);
            let bytes = u64::try_from(row.reserved.unwrap_or(0)).map_err(|_| {
                crate::errors::AsterError::internal_error("negative upload reservation")
            })?;
            reserved = reserved.saturating_add(bytes);
            if matches!(
                row.session_kind,
                UploadSessionKind::OffsetStaging | UploadSessionKind::StreamStaging
            ) {
                staged = staged.saturating_add(bytes);
            }
        }
        if blobs.count > 0 && blobs.oldest.is_none() {
            return Err(crate::errors::AsterError::internal_error(
                "missing blob cleanup observation timestamp",
            ));
        }
        let blob_count = u64::try_from(blobs.count)
            .map_err(|_| crate::errors::AsterError::internal_error("negative blob count"))?;
        let nodes = nodes
            .into_iter()
            .map(|row| {
                u64::try_from(row.count)
                    .map(|count| (row.status, count))
                    .map_err(|_| {
                        crate::errors::AsterError::internal_error("negative remote node count")
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        // Publish only after every query and conversion succeeds. Clear all
        // finite combinations so removed rows cannot survive a successful tick.
        for mode in UploadSessionKind::iter() {
            for status in ["uploading", "assembling", "failed", "presigned", "expired"] {
                metrics.upload_backlog(mode.as_str(), status, 0, 0.0);
            }
        }
        for ((mode, status), (count, age)) in grouped {
            metrics.upload_backlog(mode, status, count, age);
        }
        metrics.staging_snapshot("declared_reservations", reserved);
        metrics.staging_snapshot("staged_reservations", staged);
        metrics.blob_cleanup_backlog(
            blob_count,
            blobs
                .oldest
                .map_or(0.0, |updated| (now - updated).num_seconds().max(0) as f64),
        );
        for status in ["disabled", "unknown", "probe_failed", "probe_ok"] {
            metrics.remote_nodes(status, 0);
        }
        for (status, count) in nodes {
            let status = match status.as_str() {
                "disabled" => "disabled",
                "probe_failed" => "probe_failed",
                "probe_ok" => "probe_ok",
                _ => "unknown",
            };
            metrics.remote_nodes(status, count);
        }
        Ok(())
    })
    .await
    .map_err(|_| crate::errors::AsterError::internal_error("metrics snapshot timed out"))
    .and_then(std::convert::identity);
    state
        .metrics()
        .snapshot_status("database", result.is_ok(), now.timestamp() as f64);
    if result.is_err() {
        tracing::warn!("metrics database sampling failed");
    }
    sample_capacity(state).await;
}

pub(crate) async fn sample_capacity(state: &impl SharedRuntimeState) {
    if !state.metrics().enabled() {
        return;
    }
    let [available, total, used] = state.driver_registry().observed_capacity_minima();
    state
        .metrics()
        .capacity_snapshot("observed_target_minimum", available, total, used);
    let root = std::path::PathBuf::from(&state.config().server.upload_temp_dir);
    let sample = tokio::task::spawn_blocking(move || {
        let mut path = root.as_path();
        while !path.exists() {
            path = path
                .parent()
                .ok_or_else(|| std::io::Error::other("missing staging ancestor"))?;
        }
        aster_fs::statvfs(path)
    })
    .await;
    let success = match sample {
        Ok(Ok(stats)) => {
            state.metrics().capacity_snapshot(
                "staging_filesystem",
                Some(stats.available_space()),
                Some(stats.total_space()),
                Some(stats.total_space().saturating_sub(stats.free_space())),
            );
            state.metrics().staging_snapshot(
                "safety_floor",
                state.config().server.upload_temp_min_free_bytes,
            );
            true
        }
        _ => {
            state
                .metrics()
                .capacity_snapshot("staging_filesystem", None, None, None);
            false
        }
    };
    state
        .metrics()
        .snapshot_status("staging_filesystem", success, Utc::now().timestamp() as f64);
    if !success {
        tracing::warn!("metrics staging capacity sampling failed");
    }
}

#[cfg(all(test, feature = "metrics"))]
mod tests {
    use super::*;
    use sea_orm::{ConnectionTrait, Database, DbBackend, MockDatabase};
    use std::sync::Arc;

    async fn state() -> (PrimaryAppState, aster_forge_metrics::MetricsProvider) {
        let mut state = crate::runtime::tasks::test_support::setup_primary_state()
            .await
            .get_ref()
            .clone();
        let db = Database::connect("sqlite::memory:").await.unwrap();
        for sql in [
            "CREATE TABLE upload_sessions (session_kind TEXT, status TEXT, total_size BIGINT, created_at TEXT, expires_at TEXT)",
            "CREATE TABLE file_blobs (ref_count INTEGER, backing TEXT, cleanup_pending_since TEXT)",
            "CREATE TABLE managed_followers (is_enabled BOOLEAN, last_probe_at TEXT, last_probe_error TEXT)",
        ] {
            db.execute_unprepared(sql).await.unwrap();
        }
        state.db_handles = aster_forge_db::DbHandles::single(db);
        let provider = aster_forge_metrics::MetricsProvider::builder()
            .with_prometheus()
            .unwrap()
            .build()
            .unwrap();
        state.metrics = aster_drive_metrics::create_metrics_recorder(provider.clone()).unwrap();
        (state, provider)
    }

    async fn value(
        provider: &aster_forge_metrics::MetricsProvider,
        name: &str,
        labels: &[&str],
    ) -> f64 {
        let text = provider
            .export_prometheus(std::time::Duration::from_secs(1))
            .await
            .unwrap();
        text.lines()
            .find(|line| {
                line.starts_with(name)
                    && matches!(line.as_bytes().get(name.len()), Some(b'{' | b' '))
                    && labels.iter().all(|label| line.contains(label))
            })
            .unwrap_or_else(|| panic!("missing {name} {labels:?}: {text}"))
            .split_whitespace()
            .last()
            .unwrap()
            .parse()
            .unwrap()
    }

    #[tokio::test]
    async fn successful_sample_resets_deleted_rows_and_failed_sample_preserves_last_snapshot() {
        let (state, provider) = state().await;
        state.writer_db().execute_unprepared("INSERT INTO upload_sessions VALUES ('offset_staging', 'uploading', 42, '2000-01-01 00:00:00+00:00', '2100-01-01 00:00:00+00:00')").await.unwrap();
        state
            .writer_db()
            .execute_unprepared("INSERT INTO managed_followers VALUES (false, NULL, '')")
            .await
            .unwrap();
        sample_primary(&state).await;
        assert_eq!(
            value(
                &provider,
                "upload_sessions_active",
                &["mode=\"offset_staging\"", "state=\"uploading\""]
            )
            .await,
            1.0
        );
        assert_eq!(
            value(&provider, "remote_nodes", &["status=\"disabled\""]).await,
            1.0
        );
        assert_eq!(
            value(
                &provider,
                "observability_snapshot_valid",
                &["source=\"database\""]
            )
            .await,
            1.0
        );
        let timestamp = value(
            &provider,
            "observability_snapshot_last_success_timestamp_seconds",
            &["source=\"database\""],
        )
        .await;
        state
            .writer_db()
            .execute_unprepared("DELETE FROM upload_sessions")
            .await
            .unwrap();
        state
            .writer_db()
            .execute_unprepared("DROP TABLE managed_followers")
            .await
            .unwrap();
        sample_primary(&state).await;
        assert_eq!(
            value(
                &provider,
                "upload_sessions_active",
                &["mode=\"offset_staging\"", "state=\"uploading\""]
            )
            .await,
            1.0
        );
        assert_eq!(
            value(
                &provider,
                "observability_snapshot_valid",
                &["source=\"database\""]
            )
            .await,
            0.0
        );
        assert_eq!(
            value(
                &provider,
                "observability_snapshot_last_success_timestamp_seconds",
                &["source=\"database\""]
            )
            .await,
            timestamp
        );
        state.writer_db().execute_unprepared("CREATE TABLE managed_followers (is_enabled BOOLEAN, last_probe_at TEXT, last_probe_error TEXT)").await.unwrap();
        sample_primary(&state).await;
        assert_eq!(
            value(
                &provider,
                "upload_sessions_active",
                &["mode=\"offset_staging\"", "state=\"uploading\""]
            )
            .await,
            0.0
        );
        assert_eq!(
            value(&provider, "remote_nodes", &["status=\"disabled\""]).await,
            0.0
        );
        assert_eq!(
            value(
                &provider,
                "observability_snapshot_valid",
                &["source=\"database\""]
            )
            .await,
            1.0
        );
        provider
            .shutdown(std::time::Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn invalid_cleanup_projection_does_not_publish_partial_database_snapshot() {
        let (state, provider) = state().await;
        state
            .metrics
            .upload_backlog("offset_staging", "uploading", 7, 42.0);
        state
            .writer_db()
            .execute_unprepared("INSERT INTO file_blobs VALUES (0, 'stored', NULL)")
            .await
            .unwrap();
        sample_primary(&state).await;
        assert_eq!(
            value(
                &provider,
                "upload_sessions_active",
                &["mode=\"offset_staging\"", "state=\"uploading\""]
            )
            .await,
            7.0
        );
        assert_eq!(
            value(
                &provider,
                "observability_snapshot_valid",
                &["source=\"database\""]
            )
            .await,
            0.0
        );
        provider
            .shutdown(std::time::Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn staging_failure_marks_unknown_and_preserves_last_success() {
        let (mut state, provider) = state().await;
        let mut config = (*state.config).clone();
        config.server.upload_temp_dir = std::env::temp_dir().to_string_lossy().into_owned();
        state.config = Arc::new(config.clone());
        sample_capacity(&state).await;
        assert_eq!(
            value(
                &provider,
                "storage_capacity_known",
                &["scope=\"staging_filesystem\"", "kind=\"available\""]
            )
            .await,
            1.0
        );
        let timestamp = value(
            &provider,
            "observability_snapshot_last_success_timestamp_seconds",
            &["source=\"staging_filesystem\""],
        )
        .await;
        config.server.upload_temp_dir = "\0".into();
        state.config = Arc::new(config);
        sample_capacity(&state).await;
        assert_eq!(
            value(
                &provider,
                "storage_capacity_known",
                &["scope=\"staging_filesystem\"", "kind=\"available\""]
            )
            .await,
            0.0
        );
        assert_eq!(
            value(
                &provider,
                "observability_snapshot_valid",
                &["source=\"staging_filesystem\""]
            )
            .await,
            0.0
        );
        assert_eq!(
            value(
                &provider,
                "observability_snapshot_last_success_timestamp_seconds",
                &["source=\"staging_filesystem\""]
            )
            .await,
            timestamp
        );
        provider
            .shutdown(std::time::Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn disabled_recorder_performs_no_database_sampling() {
        let (mut state, provider) = state().await;
        let db = MockDatabase::new(DbBackend::Sqlite).into_connection();
        state.db_handles = aster_forge_db::DbHandles::single(db);
        state.metrics = aster_drive_metrics::NoopMetrics::arc();
        sample_primary(&state).await;
        let state_db = state.writer_db().clone();
        drop(state);
        assert!(state_db.into_transaction_log().is_empty());
        provider
            .shutdown(std::time::Duration::from_secs(1))
            .await
            .unwrap();
    }
}
