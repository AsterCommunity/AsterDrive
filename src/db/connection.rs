//! 数据库子模块：`connection`。

use crate::config::DatabaseConfig;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::metrics_core::SharedMetricsRecorder;
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, SqlxSqliteConnector};

#[derive(Clone)]
pub struct DbHandles {
    writer: DatabaseConnection,
    reader: DatabaseConnection,
    sqlite_read_write_split: bool,
}

impl DbHandles {
    pub fn single(db: DatabaseConnection) -> Self {
        Self {
            writer: db.clone(),
            reader: db,
            sqlite_read_write_split: false,
        }
    }

    pub fn writer(&self) -> &DatabaseConnection {
        &self.writer
    }

    pub fn reader(&self) -> &DatabaseConnection {
        &self.reader
    }

    pub fn sqlite_read_write_split(&self) -> bool {
        self.sqlite_read_write_split
    }
}

pub async fn connect(cfg: &DatabaseConfig) -> Result<DatabaseConnection> {
    connect_with_metrics(cfg, crate::metrics_core::NoopMetrics::arc()).await
}

pub async fn connect_with_metrics(
    cfg: &DatabaseConfig,
    metrics: SharedMetricsRecorder,
) -> Result<DatabaseConnection> {
    let retry_config = crate::db::retry::RetryConfig {
        max_retries: cfg.retry_count,
        ..Default::default()
    };
    crate::db::retry::with_retry(&retry_config, || connect_once(cfg, metrics.clone())).await
}

pub async fn connect_handles(cfg: &DatabaseConfig) -> Result<DbHandles> {
    let writer = connect(cfg).await?;
    connect_reader_for_writer(cfg, writer).await
}

pub async fn connect_reader_for_writer(
    cfg: &DatabaseConfig,
    writer: DatabaseConnection,
) -> Result<DbHandles> {
    connect_reader_for_writer_with_metrics(cfg, writer, crate::metrics_core::NoopMetrics::arc())
        .await
}

pub async fn connect_reader_for_writer_with_metrics(
    cfg: &DatabaseConfig,
    writer: DatabaseConnection,
    metrics: SharedMetricsRecorder,
) -> Result<DbHandles> {
    let url = normalize_database_url(&cfg.url);
    if !sqlite_reader_pool_enabled(&url) {
        return Ok(DbHandles::single(writer));
    }

    let reader = connect_sqlite_reader_once(cfg, &url, metrics).await?;
    Ok(DbHandles {
        writer,
        reader,
        sqlite_read_write_split: true,
    })
}

async fn connect_once(
    cfg: &DatabaseConfig,
    metrics: SharedMetricsRecorder,
) -> Result<DatabaseConnection> {
    let url = normalize_database_url(&cfg.url);
    let is_sqlite = url.starts_with("sqlite:");
    // SQLite relies on a single pooled connection so concurrent writers are serialized at
    // connection acquisition; repo-layer "lock" helpers do not emulate row locks there.
    let max_connections = if is_sqlite { 1 } else { cfg.pool_size };

    let mut opt = ConnectOptions::new(&url);
    opt.max_connections(max_connections)
        .min_connections(1)
        .sqlx_logging(false)
        .test_before_acquire(true);

    // SeaORM's generic Database::connect() pre-validates URLs with url::Url::parse(),
    // which rejects Windows-style SQLite paths containing backslashes. Route SQLite
    // through sqlx's dedicated connector instead so platform-native paths keep working.
    let db = if is_sqlite {
        SqlxSqliteConnector::connect(opt)
            .await
            .map_aster_err(AsterError::database_operation)?
    } else {
        Database::connect(opt)
            .await
            .map_aster_err(AsterError::database_operation)?
    };

    let backend = db.get_database_backend();
    tracing::info!(backend = ?backend, "database connected");

    if is_sqlite {
        tracing::info!(max_connections, "applying SQLite PRAGMA optimizations");
        db.execute_unprepared("PRAGMA journal_mode=WAL;")
            .await
            .map_aster_err(AsterError::database_operation)?;
        db.execute_unprepared("PRAGMA busy_timeout=15000;")
            .await
            .map_aster_err(AsterError::database_operation)?;
        db.execute_unprepared("PRAGMA synchronous=NORMAL;")
            .await
            .map_aster_err(AsterError::database_operation)?;
        db.execute_unprepared("PRAGMA foreign_keys=ON;")
            .await
            .map_aster_err(AsterError::database_operation)?;
    }

    let mut db = db;
    install_db_metrics(&mut db, metrics);

    Ok(db)
}

async fn connect_sqlite_reader_once(
    cfg: &DatabaseConfig,
    normalized_writer_url: &str,
    metrics: SharedMetricsRecorder,
) -> Result<DatabaseConnection> {
    let reader_url = sqlite_reader_url(normalized_writer_url);
    let max_connections = cfg.pool_size.max(1);
    let mut opt = ConnectOptions::new(&reader_url);
    opt.max_connections(max_connections)
        .min_connections(1)
        .sqlx_logging(false)
        .test_before_acquire(true)
        .map_sqlx_sqlite_pool_opts(|pool_options| {
            pool_options.after_connect(|conn, _meta| {
                Box::pin(async move {
                    use sea_orm::sqlx::Executor;

                    conn.execute("PRAGMA busy_timeout=15000;").await?;
                    conn.execute("PRAGMA synchronous=NORMAL;").await?;
                    conn.execute("PRAGMA foreign_keys=ON;").await?;
                    conn.execute("PRAGMA query_only=ON;").await?;
                    Ok(())
                })
            })
        });

    let mut db = SqlxSqliteConnector::connect(opt)
        .await
        .map_aster_err(AsterError::database_operation)?;
    install_db_metrics(&mut db, metrics);

    tracing::info!(
        max_connections,
        "SQLite reader pool connected with query_only pragma"
    );
    Ok(db)
}

fn normalize_database_url(database_url: &str) -> String {
    if database_url == "sqlite::memory:" {
        return database_url.to_string();
    }

    if database_url.starts_with("sqlite://") && !database_url.contains('?') {
        return format!("{database_url}?mode=rwc");
    }

    database_url.to_string()
}

fn sqlite_reader_pool_enabled(normalized_url: &str) -> bool {
    normalized_url.starts_with("sqlite:") && !is_sqlite_memory_url(normalized_url)
}

fn is_sqlite_memory_url(normalized_url: &str) -> bool {
    normalized_url == "sqlite::memory:"
        || normalized_url
            .split_once('?')
            .is_some_and(|(_, query)| query.split('&').any(|param| param == "mode=memory"))
}

fn sqlite_reader_url(normalized_writer_url: &str) -> String {
    let Some((base, query)) = normalized_writer_url.split_once('?') else {
        return format!("{normalized_writer_url}?mode=ro");
    };

    let mut saw_mode = false;
    let query = query
        .split('&')
        .filter(|param| !param.is_empty())
        .map(|param| {
            if param.starts_with("mode=") {
                saw_mode = true;
                "mode=ro"
            } else {
                param
            }
        })
        .collect::<Vec<_>>();

    if saw_mode {
        format!("{base}?{}", query.join("&"))
    } else if query.is_empty() {
        format!("{base}?mode=ro")
    } else {
        format!("{base}?mode=ro&{}", query.join("&"))
    }
}

fn install_db_metrics(db: &mut DatabaseConnection, metrics: SharedMetricsRecorder) {
    if !metrics.enabled() {
        return;
    }

    db.set_metric_callback(move |info| {
        metrics.record_db_query(info);
    });
}

#[cfg(test)]
mod tests {
    use super::normalize_database_url;
    use crate::config::DatabaseConfig;
    use sea_orm::{ConnectionTrait, TransactionTrait};

    #[test]
    fn sqlite_urls_without_query_default_to_rwc_mode() {
        assert_eq!(
            normalize_database_url("sqlite:///var/lib/asterdrive/app.db"),
            "sqlite:///var/lib/asterdrive/app.db?mode=rwc"
        );
        assert_eq!(
            normalize_database_url("sqlite://data/asterdrive.db"),
            "sqlite://data/asterdrive.db?mode=rwc"
        );
    }

    #[test]
    fn sqlite_memory_and_existing_queries_are_preserved() {
        assert_eq!(normalize_database_url("sqlite::memory:"), "sqlite::memory:");
        assert_eq!(
            normalize_database_url("sqlite:///var/lib/asterdrive/app.db?mode=ro"),
            "sqlite:///var/lib/asterdrive/app.db?mode=ro"
        );
        assert_eq!(
            normalize_database_url("postgres://user:pass@localhost/asterdrive"),
            "postgres://user:pass@localhost/asterdrive"
        );
    }

    #[test]
    fn sqlite_reader_pool_skips_memory_databases() {
        assert!(!super::sqlite_reader_pool_enabled("sqlite::memory:"));
        assert!(!super::sqlite_reader_pool_enabled(
            "sqlite://memory-test?mode=memory&cache=shared"
        ));
        assert!(super::sqlite_reader_pool_enabled(
            "sqlite:///var/lib/asterdrive/app.db?mode=rwc"
        ));
    }

    #[test]
    fn sqlite_reader_url_forces_read_only_mode() {
        assert_eq!(
            super::sqlite_reader_url("sqlite:///var/lib/asterdrive/app.db?mode=rwc"),
            "sqlite:///var/lib/asterdrive/app.db?mode=ro"
        );
        assert_eq!(
            super::sqlite_reader_url("sqlite:///var/lib/asterdrive/app.db?cache=shared"),
            "sqlite:///var/lib/asterdrive/app.db?mode=ro&cache=shared"
        );
        assert_eq!(
            super::sqlite_reader_url("sqlite:///var/lib/asterdrive/app.db?"),
            "sqlite:///var/lib/asterdrive/app.db?mode=ro"
        );
        assert_eq!(
            super::sqlite_reader_url("sqlite:///var/lib/asterdrive/app.db?&cache=shared"),
            "sqlite:///var/lib/asterdrive/app.db?mode=ro&cache=shared"
        );
        assert_eq!(
            super::sqlite_reader_url("sqlite:///var/lib/asterdrive/app.db?mode=rwc&"),
            "sqlite:///var/lib/asterdrive/app.db?mode=ro"
        );
    }

    #[tokio::test]
    async fn sqlite_connector_accepts_windows_style_urls() {
        let url = format!(
            "sqlite://windows\\sqlite-url-{}?mode=memory&cache=shared",
            uuid::Uuid::new_v4()
        );
        let db = super::connect(&DatabaseConfig {
            url,
            pool_size: 10,
            retry_count: 3,
        })
        .await
        .expect("sqlite connection should succeed for Windows-style URL");

        db.execute_unprepared("SELECT 1;")
            .await
            .expect("sqlite query should succeed");
    }

    #[tokio::test]
    async fn sqlite_memory_handles_use_single_connection() {
        let handles = super::connect_handles(&DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 4,
            retry_count: 0,
        })
        .await
        .expect("sqlite memory handles should connect");

        assert!(!handles.sqlite_read_write_split());
        assert_eq!(
            handles.writer().get_database_backend(),
            handles.reader().get_database_backend()
        );
    }

    #[tokio::test]
    async fn sqlite_reader_pool_is_query_only() {
        let url = format!(
            "sqlite:///tmp/asterdrive-reader-pool-{}.db?mode=rwc",
            uuid::Uuid::new_v4()
        );
        let handles = super::connect_handles(&DatabaseConfig {
            url,
            pool_size: 4,
            retry_count: 0,
        })
        .await
        .expect("sqlite handles should connect");
        assert!(handles.sqlite_read_write_split());

        handles
            .writer()
            .execute_unprepared("CREATE TABLE reader_guard (id INTEGER PRIMARY KEY);")
            .await
            .expect("writer should create table");

        let write_result = handles
            .reader()
            .execute_unprepared("INSERT INTO reader_guard (id) VALUES (1);")
            .await;
        assert!(write_result.is_err(), "reader pool must reject writes");
    }

    #[tokio::test]
    async fn sqlite_reader_pool_reads_while_writer_connection_is_busy() {
        let url = format!(
            "sqlite:///tmp/asterdrive-reader-writer-split-{}.db?mode=rwc",
            uuid::Uuid::new_v4()
        );
        let handles = super::connect_handles(&DatabaseConfig {
            url,
            pool_size: 4,
            retry_count: 0,
        })
        .await
        .expect("sqlite handles should connect");
        assert!(handles.sqlite_read_write_split());

        handles
            .writer()
            .execute_unprepared("CREATE TABLE split_guard (id INTEGER PRIMARY KEY, name TEXT);")
            .await
            .expect("writer should create table");
        handles
            .writer()
            .execute_unprepared("INSERT INTO split_guard (id, name) VALUES (1, 'ready');")
            .await
            .expect("writer should seed row");

        let txn = handles
            .writer()
            .begin()
            .await
            .expect("writer transaction should begin");
        txn.execute_unprepared("UPDATE split_guard SET name = 'held' WHERE id = 1;")
            .await
            .expect("writer transaction should hold a write lock");

        let read = tokio::time::timeout(std::time::Duration::from_millis(250), async {
            handles
                .reader()
                .query_one_raw(sea_orm::Statement::from_string(
                    sea_orm::DbBackend::Sqlite,
                    "SELECT name FROM split_guard WHERE id = 1",
                ))
                .await
        })
        .await
        .expect("reader should not wait on the writer pool queue")
        .expect("reader query should succeed")
        .expect("reader query should return row");
        let name: String = read
            .try_get_by_index(0)
            .expect("reader query should decode name");
        assert_eq!(name, "ready");

        txn.rollback()
            .await
            .expect("writer transaction should roll back");
    }
}
