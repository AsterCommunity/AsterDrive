//! Drive database configuration adapter backed by AsterForge.

use crate::config::DatabaseConfig;
use crate::errors::Result;
use aster_drive_metrics::SharedMetricsRecorder;
use sea_orm::DatabaseConnection;

fn forge_database_config(cfg: &DatabaseConfig) -> aster_forge_db::DatabaseConfig {
    aster_forge_db::DatabaseConfig {
        url: cfg.url.clone(),
        pool_size: cfg.pool_size,
        retry_count: cfg.retry_count,
    }
}

/// Connects to the configured database and installs the shared Forge metrics callback.
pub async fn connect_with_metrics(
    cfg: &DatabaseConfig,
    metrics: SharedMetricsRecorder,
) -> Result<DatabaseConnection> {
    aster_forge_db::connect_with_metrics(&forge_database_config(cfg), metrics.forge_recorder())
        .await
        .map_err(Into::into)
}

/// Creates reader/writer handles for an existing writer connection.
pub async fn connect_reader_for_writer_with_metrics(
    cfg: &DatabaseConfig,
    writer: DatabaseConnection,
    metrics: SharedMetricsRecorder,
) -> Result<aster_forge_db::DbHandles> {
    aster_forge_db::connect_reader_for_writer_with_metrics(
        &forge_database_config(cfg),
        writer,
        metrics.forge_recorder(),
    )
    .await
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::forge_database_config;
    use crate::config::DatabaseConfig;
    use sea_orm::ConnectionTrait;

    #[test]
    fn forge_config_preserves_drive_database_settings() {
        let config = forge_database_config(&DatabaseConfig {
            url: "sqlite://drive.db".into(),
            pool_size: 12,
            retry_count: 4,
        });

        assert_eq!(config.url.as_url(), Some("sqlite://drive.db"));
        assert_eq!(config.pool_size, 12);
        assert_eq!(config.retry_count, 4);
    }

    #[test]
    fn forge_config_preserves_structured_database_credentials() {
        let raw_username = "drive user";
        let raw_password = "raw#[]{}^+=*@:/?%secret";
        let config = forge_database_config(&DatabaseConfig {
            url: aster_forge_db::DatabaseUrl::credentials(
                "postgres://database.internal:5432/asterdrive",
                Some(raw_username.to_string()),
                Some(raw_password.to_string()),
            ),
            pool_size: 12,
            retry_count: 4,
        });

        assert_eq!(
            config.url,
            aster_forge_db::DatabaseUrl::credentials(
                "postgres://database.internal:5432/asterdrive",
                Some(raw_username.to_string()),
                Some(raw_password.to_string()),
            )
        );
        let debug = format!("{config:?}");
        assert!(!debug.contains(raw_username));
        assert!(!debug.contains(raw_password));
    }

    #[tokio::test]
    async fn drive_adapter_accepts_windows_style_sqlite_urls() {
        let url = format!(
            "sqlite://windows\\sqlite-url-{}?mode=memory&cache=shared",
            uuid::Uuid::new_v4()
        );
        let db = super::connect_with_metrics(
            &DatabaseConfig {
                url: url.into(),
                pool_size: 10,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("Forge-backed connection should accept Windows-style SQLite URLs");

        db.execute_unprepared("SELECT 1;")
            .await
            .expect("SQLite query should succeed");
    }

    #[tokio::test]
    async fn drive_adapter_uses_single_handle_for_sqlite_memory() {
        let cfg = DatabaseConfig {
            url: "sqlite::memory:".into(),
            pool_size: 4,
            retry_count: 0,
        };
        let writer = super::connect_with_metrics(&cfg, aster_drive_metrics::NoopMetrics::arc())
            .await
            .expect("SQLite memory writer should connect");
        let handles = super::connect_reader_for_writer_with_metrics(
            &cfg,
            writer,
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("SQLite memory handles should connect");

        assert!(!handles.sqlite_read_write_split());
        assert_eq!(
            handles.writer().get_database_backend(),
            handles.reader().get_database_backend()
        );
    }

    #[tokio::test]
    async fn drive_adapter_creates_query_only_sqlite_reader_pool() {
        let url = format!(
            "sqlite:///tmp/asterdrive-forge-reader-pool-{}.db?mode=rwc",
            uuid::Uuid::new_v4()
        );
        let cfg = DatabaseConfig {
            url: url.into(),
            pool_size: 4,
            retry_count: 0,
        };
        let writer = super::connect_with_metrics(&cfg, aster_drive_metrics::NoopMetrics::arc())
            .await
            .expect("SQLite writer should connect");
        let handles = super::connect_reader_for_writer_with_metrics(
            &cfg,
            writer,
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("SQLite handles should connect");

        assert!(handles.sqlite_read_write_split());
        handles
            .writer()
            .execute_unprepared("CREATE TABLE reader_guard (id INTEGER PRIMARY KEY);")
            .await
            .expect("Writer should create table");

        let write_result = handles
            .reader()
            .execute_unprepared("INSERT INTO reader_guard (id) VALUES (1);")
            .await;
        assert!(write_result.is_err(), "Reader pool must reject writes");
    }
}
