//! CLI 子命令共用的数据库辅助函数。
//!
//! 这里放置和迁移、doctor 等命令都需要的数据库层小工具，避免每个子模块
//! 各自维护后端命名、迁移历史、标识符转义和连接字符串脱敏逻辑。

use std::path::Path;

use migration::{MigrationHistory, MigrationTrack, inspect_migration_history};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use url::Url;

use crate::errors::{AsterError, MapAsterErr, Result};

pub(super) fn join_strings(values: &[String]) -> String {
    values.join(", ")
}

pub(super) fn backend_name(backend: DbBackend) -> &'static str {
    match backend {
        DbBackend::MySql => "mysql",
        DbBackend::Postgres => "postgres",
        DbBackend::Sqlite => "sqlite",
        _ => "unknown",
    }
}

pub(super) async fn pending_migrations<C>(db: &C) -> Result<Vec<String>>
where
    C: ConnectionTrait,
{
    let history = inspect_migration_history(db)
        .await
        .map_aster_err(AsterError::database_operation)?;
    if history.track == MigrationTrack::Unknown {
        return Err(AsterError::validation_error(format!(
            "database contains unknown migration versions: {}",
            unsupported_migration_versions_label(&history)
        )));
    }

    Ok(history.effective_pending().to_vec())
}

fn unsupported_migration_versions_label(history: &MigrationHistory) -> String {
    if !history.unknown_applied.is_empty() {
        join_strings(&history.unknown_applied)
    } else if history.applied.is_empty() {
        "<empty migration history with existing schema objects>".to_string()
    } else {
        "<non-prefix migration history>".to_string()
    }
}

pub(super) async fn scalar_i64<C>(db: &C, backend: DbBackend, sql: &str) -> Result<i64>
where
    C: ConnectionTrait,
{
    let row = db
        .query_one_raw(Statement::from_string(backend, sql))
        .await
        .map_aster_err(AsterError::database_operation)?
        .ok_or_else(|| AsterError::database_operation(format!("query returned no rows: {sql}")))?;

    if let Ok(value) = row.try_get_by_index::<i64>(0) {
        return Ok(value);
    }
    if let Ok(value) = row.try_get_by_index::<i32>(0) {
        return Ok(i64::from(value));
    }
    if let Ok(value) = row.try_get_by_index::<bool>(0) {
        return Ok(if value { 1 } else { 0 });
    }

    Err(AsterError::database_operation(format!(
        "failed to decode scalar query result as integer: {sql}"
    )))
}

pub(super) fn quote_ident(backend: DbBackend, ident: &str) -> String {
    match backend {
        DbBackend::MySql => format!("`{}`", ident.replace('`', "``")),
        DbBackend::Postgres | DbBackend::Sqlite => {
            format!("\"{}\"", ident.replace('"', "\"\""))
        }
        _ => format!("\"{}\"", ident.replace('"', "\"\"")),
    }
}

pub(super) fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(super) fn quote_sqlite_literal(value: &str) -> String {
    quote_literal(value)
}

pub(super) fn redact_database_url(database_url: &str) -> String {
    if database_url == "sqlite::memory:" {
        return database_url.to_string();
    }

    if database_url.starts_with("sqlite:") {
        return redact_sqlite_database_url(database_url);
    }

    if let Ok(mut url) = Url::parse(database_url) {
        if !url.username().is_empty() || url.password().is_some() {
            let _ = url.set_username("***");
            let _ = url.set_password(None);
        }
        redact_url_query(&mut url);
        return url.to_string();
    }

    redact_unparsed_url(database_url)
}

fn redact_sqlite_database_url(database_url: &str) -> String {
    let Some(path_and_query) = database_url.strip_prefix("sqlite://") else {
        let Some((base, query)) = database_url.split_once('?') else {
            return database_url.to_string();
        };
        return format!("{base}?{}", redact_query_string(query));
    };
    let (path, query) = path_and_query
        .split_once('?')
        .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
    let redacted_path = redact_sqlite_path(path);

    match query {
        Some(query) => format!("sqlite://{redacted_path}?{}", redact_query_string(query)),
        None => format!("sqlite://{redacted_path}"),
    }
}

fn redact_url_query(url: &mut Url) {
    let Some(query) = url.query() else {
        return;
    };
    let redacted = redact_query_string(query);
    url.set_query(Some(&redacted));
}

fn redact_query_string(query: &str) -> String {
    url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| {
            let value = if is_sensitive_query_key(&key) {
                "***"
            } else {
                value.as_ref()
            };
            (key.into_owned(), value.to_string())
        })
        .fold(
            url::form_urlencoded::Serializer::new(String::new()),
            |mut serializer, (key, value)| {
                serializer.append_pair(&key, &value);
                serializer
            },
        )
        .finish()
}

fn is_sensitive_query_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();

    matches!(
        normalized.as_str(),
        "pass"
            | "password"
            | "token"
            | "accesstoken"
            | "refreshtoken"
            | "secret"
            | "apikey"
            | "key"
            | "credential"
            | "credentials"
    ) || normalized.contains("token")
        || normalized.contains("password")
        || normalized.contains("secret")
        || normalized.contains("credential")
}

fn redact_unparsed_url(database_url: &str) -> String {
    let (without_fragment, fragment) = database_url
        .split_once('#')
        .map_or((database_url, None), |(value, fragment)| {
            (value, Some(fragment))
        });
    let (base, query) = without_fragment
        .split_once('?')
        .map_or((without_fragment, None), |(base, query)| {
            (base, Some(query))
        });
    let base = if let Some((scheme, rest)) = base.split_once("://") {
        if let Some((_authority, suffix)) = rest.rsplit_once('@') {
            format!("{scheme}://***@{suffix}")
        } else {
            base.to_string()
        }
    } else {
        base.to_string()
    };
    let mut redacted = match query {
        Some(query) => format!("{base}?{}", redact_query_string(query)),
        None => base,
    };
    if let Some(fragment) = fragment {
        redacted.push('#');
        redacted.push_str(fragment);
    }
    redacted
}

fn redact_sqlite_path(path: &str) -> String {
    if path == ":memory:" {
        return path.to_string();
    }

    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "***".to_string();
    }

    let Some(file_name) = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
    else {
        return "***".to_string();
    };

    if path.starts_with('/') {
        format!("/.../{file_name}")
    } else {
        format!(".../{file_name}")
    }
}

#[cfg(test)]
mod tests {
    use super::redact_database_url;

    #[test]
    fn redact_database_url_masks_network_credentials() {
        assert_eq!(
            redact_database_url("postgres://postgres:postgres@127.0.0.1:5432/asterdrive"),
            "postgres://***@127.0.0.1:5432/asterdrive"
        );
        assert_eq!(
            redact_database_url("mysql://aster@db.internal:3306/asterdrive"),
            "mysql://***@db.internal:3306/asterdrive"
        );
        let redacted = redact_database_url(
            "postgres://us%40er:p%40ss@db.internal:5432/asterdrive?password=p%40ss&%61ccess_token=a%2Fb&sslmode=require",
        );
        assert!(redacted.starts_with("postgres://***@db.internal:5432/asterdrive?"));
        assert!(redacted.contains("password=***"));
        assert!(redacted.contains("access_token=***"));
        assert!(redacted.contains("sslmode=require"));
        assert!(!redacted.contains("us%40er"));
        assert!(!redacted.contains("p%40ss"));
        assert!(!redacted.contains("a%2Fb"));
    }

    #[test]
    fn redact_database_url_masks_query_variants_without_authority() {
        let redacted = redact_database_url(
            "postgres://db.internal:5432/asterdrive?token=first&password=&client_secret=second&api-key=third&credential_file=fourth&monkey=safe&mode=rwc&token=fifth",
        );

        assert!(redacted.contains("token=***"));
        assert!(redacted.contains("password=***"));
        assert!(redacted.contains("client_secret=***"));
        assert!(redacted.contains("api-key=***"));
        assert!(redacted.contains("credential_file=***"));
        assert!(redacted.contains("monkey=safe"));
        assert!(redacted.contains("mode=rwc"));
        assert_eq!(redacted.matches("token=***").count(), 2);
        assert!(!redacted.contains("first"));
        assert!(!redacted.contains("second"));
        assert!(!redacted.contains("third"));
        assert!(!redacted.contains("fourth"));
        assert!(!redacted.contains("fifth"));
    }

    #[test]
    fn redact_database_url_masks_sqlite_paths_but_preserves_filename() {
        assert_eq!(
            redact_database_url(
                "sqlite:///Users/esap/Desktop/Github/AsterDrive/data/asterdrive.db?mode=rwc"
            ),
            "sqlite:///.../asterdrive.db?mode=rwc"
        );
        assert_eq!(
            redact_database_url("sqlite://data/asterdrive.db?mode=rwc"),
            "sqlite://.../asterdrive.db?mode=rwc"
        );
        assert_eq!(redact_database_url("sqlite::memory:"), "sqlite::memory:");
        assert_eq!(
            redact_database_url("sqlite://:memory:"),
            "sqlite://:memory:"
        );
        let redacted = redact_database_url(
            "sqlite://data/asterdrive.db?mode=rwc&password=secret%2Fvalue&DB_TOKEN=abc",
        );
        assert!(redacted.contains("mode=rwc"));
        assert!(redacted.contains("password=***"));
        assert!(redacted.contains("DB_TOKEN=***"));
        assert!(!redacted.contains("secret%2Fvalue"));
        assert!(!redacted.contains("abc"));
        assert_eq!(
            redact_database_url("sqlite::memory:?token=encoded%2Fsecret&mode=memory"),
            "sqlite::memory:?token=***&mode=memory"
        );

        let redacted = redact_database_url("postgres://user:secret@db host/db?token=abc");
        assert!(redacted.starts_with("postgres://***@db host/db?"));
        assert!(redacted.contains("token=***"));
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("abc"));
    }
}
