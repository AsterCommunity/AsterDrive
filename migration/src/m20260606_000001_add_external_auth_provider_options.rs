//! Add provider-specific options JSON for external auth providers.

use crate::column::{json_text_column_for_final_schema, nullable_json_text_column_for_backfill};
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend};

const MICROSOFT_LOGIN_HOST: &str = "login.microsoftonline.com";
const MICROSOFT_DEFAULT_TENANT: &str = "common";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_provider_options_column(manager).await?;

        backfill_default_provider_options(manager).await?;
        backfill_microsoft_provider_options(manager).await?;
        enforce_provider_options_not_null(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ExternalAuthProviders::Table)
                    .drop_column(ExternalAuthProviders::Options)
                    .to_owned(),
            )
            .await
    }
}

async fn add_provider_options_column(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let options = if manager.get_database_backend() == DbBackend::MySql {
        nullable_json_text_column_for_backfill(ExternalAuthProviders::Options)
    } else {
        json_text_column_for_final_schema(manager, ExternalAuthProviders::Options)
    };

    manager
        .alter_table(
            Table::alter()
                .table(ExternalAuthProviders::Table)
                .add_column(options)
                .to_owned(),
        )
        .await
}

async fn backfill_default_provider_options(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(
            &Query::update()
                .table(ExternalAuthProviders::Table)
                .value(ExternalAuthProviders::Options, "{}")
                .and_where(Expr::col(ExternalAuthProviders::Options).is_null())
                .to_owned(),
        )
        .await
        .map_err(|error| {
            DbErr::Migration(format!(
                "failed to backfill default external auth provider options: {error}"
            ))
        })?;
    Ok(())
}

async fn enforce_provider_options_not_null(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::MySql {
        return Ok(());
    }

    manager
        .alter_table(
            Table::alter()
                .table(ExternalAuthProviders::Table)
                .modify_column(
                    ColumnDef::new(ExternalAuthProviders::Options)
                        .text()
                        .not_null()
                        .to_owned(),
                )
                .to_owned(),
        )
        .await
}

async fn backfill_microsoft_provider_options(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let select = Query::select()
        .column(ExternalAuthProviders::Id)
        .column(ExternalAuthProviders::IssuerUrl)
        .from(ExternalAuthProviders::Table)
        .and_where(Expr::col(ExternalAuthProviders::ProviderKind).eq("microsoft"))
        .to_owned();
    let rows = manager
        .get_connection()
        .query_all(&select)
        .await
        .map_err(|error| {
            DbErr::Migration(format!(
                "failed to load Microsoft external auth providers for options backfill: {error}"
            ))
        })?;

    for row in rows {
        let id = row.try_get_by_index::<i64>(0).map_err(|error| {
            DbErr::Migration(format!(
                "failed to decode Microsoft external auth provider id during options backfill: {error}"
            ))
        })?;
        let issuer_url = row.try_get_by_index::<Option<String>>(1).map_err(|error| {
            DbErr::Migration(format!(
                "failed to decode Microsoft external auth provider #{id} issuer_url during options backfill: {error}"
            ))
        })?;
        let tenant = microsoft_tenant_for_options_backfill(id, issuer_url.as_deref());
        let options = serde_json::json!({
            "microsoft": {
                "tenant": tenant,
            },
        })
        .to_string();

        manager
            .get_connection()
            .execute(
                &Query::update()
                    .table(ExternalAuthProviders::Table)
                    .value(ExternalAuthProviders::Options, options)
                    .value(ExternalAuthProviders::IssuerUrl, Option::<String>::None)
                    .and_where(Expr::col(ExternalAuthProviders::Id).eq(id))
                    .to_owned(),
            )
            .await
            .map_err(|error| {
                DbErr::Migration(format!(
                    "failed to backfill Microsoft external auth provider #{id} options: {error}"
                ))
            })?;
    }

    Ok(())
}

fn microsoft_tenant_for_options_backfill(id: i64, issuer_url: Option<&str>) -> String {
    match microsoft_tenant_from_issuer_url(issuer_url) {
        Some(tenant) => tenant,
        None => {
            tracing::warn!(
                provider_id = id,
                issuer_url = issuer_url.unwrap_or("<null>"),
                "failed to parse Microsoft external auth issuer URL during options backfill; defaulting tenant to common"
            );
            MICROSOFT_DEFAULT_TENANT.to_string()
        }
    }
}

fn microsoft_tenant_from_issuer_url(value: Option<&str>) -> Option<String> {
    let value = value.map(str::trim).filter(|value| !value.is_empty())?;
    let value = value.to_ascii_lowercase();
    if value == MICROSOFT_DEFAULT_TENANT
        || value == "organizations"
        || value == "consumers"
        || is_microsoft_tenant_id(&value)
    {
        return Some(value);
    }

    let path = value
        .strip_prefix("https://")
        .and_then(|value| value.strip_prefix(MICROSOFT_LOGIN_HOST))
        .and_then(|value| value.strip_prefix('/'))?;
    if path.contains('?') || path.contains('#') {
        return None;
    }
    let path = path.trim_end_matches('/');
    let segments = path.split('/').collect::<Vec<_>>();
    if segments.len() != 2 || segments[1] != "v2.0" {
        return None;
    }
    let tenant = segments[0];
    if tenant == MICROSOFT_DEFAULT_TENANT
        || tenant == "organizations"
        || tenant == "consumers"
        || is_microsoft_tenant_id(tenant)
    {
        Some(tenant.to_string())
    } else {
        None
    }
}

fn is_microsoft_tenant_id(value: &str) -> bool {
    const LEN: usize = 36;
    const HYPHEN_POSITIONS: [usize; 4] = [8, 13, 18, 23];

    value.len() == LEN
        && value.chars().enumerate().all(|(index, ch)| {
            if HYPHEN_POSITIONS.contains(&index) {
                ch == '-'
            } else {
                ch.is_ascii_hexdigit()
            }
        })
}

#[derive(DeriveIden)]
enum ExternalAuthProviders {
    Table,
    Id,
    ProviderKind,
    Options,
    IssuerUrl,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microsoft_tenant_from_issuer_url_canonicalizes_supported_values() {
        for (input, expected) in [
            ("common", "common"),
            (" COMMON ", "common"),
            ("Organizations", "organizations"),
            ("Consumers", "consumers"),
            (
                "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE",
                "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            ),
            (
                "HTTPS://LOGIN.MICROSOFTONLINE.COM/Organizations/V2.0/",
                "organizations",
            ),
            (
                "https://login.microsoftonline.com/AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE/v2.0",
                "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            ),
        ] {
            assert_eq!(
                microsoft_tenant_from_issuer_url(Some(input)).as_deref(),
                Some(expected),
                "{input} should backfill as {expected}",
            );
        }
    }

    #[test]
    fn microsoft_tenant_from_issuer_url_rejects_unsupported_boundaries() {
        for input in [
            None,
            Some(""),
            Some(" "),
            Some("tenant.example.com"),
            Some("http://login.microsoftonline.com/common/v2.0"),
            Some("https://example.com/common/v2.0"),
            Some("https://login.microsoftonline.com/common"),
            Some("https://login.microsoftonline.com/common/v1.0"),
            Some("https://login.microsoftonline.com/common/v2.0/extra"),
            Some("https://login.microsoftonline.com/common/v2.0?x=1"),
            Some("https://login.microsoftonline.com/common/v2.0#fragment"),
            Some("common/v2.0"),
        ] {
            assert!(
                microsoft_tenant_from_issuer_url(input).is_none(),
                "{input:?} should not be backfilled",
            );
        }
    }

    #[test]
    fn microsoft_tenant_for_options_backfill_defaults_invalid_issuer_to_common() {
        assert_eq!(
            microsoft_tenant_for_options_backfill(
                42,
                Some("https://login.example.com/common/v2.0")
            ),
            MICROSOFT_DEFAULT_TENANT
        );
        assert_eq!(
            microsoft_tenant_for_options_backfill(42, None),
            MICROSOFT_DEFAULT_TENANT
        );
        assert_eq!(
            microsoft_tenant_for_options_backfill(42, Some("Organizations")),
            "organizations"
        );
    }
}
