//! 集成测试：检查 migration 建出的表列与 SeaORM entity 映射没有漂移。

#[macro_use]
mod common;

use std::collections::BTreeSet;

use aster_drive::entities::{
    audit_log, auth_session, background_task, blob_media_metadata, contact_verification_token,
    entity_property, external_auth_email_verification_flow, external_auth_identity,
    external_auth_login_flow, external_auth_provider, file, file_blob, file_version, folder,
    follower_enrollment_session, mail_outbox, managed_follower, master_binding, mfa_email_code,
    mfa_factor, mfa_login_flow, mfa_recovery_code, mfa_totp_setup_flow, passkey,
    remote_storage_target, resource_lock, share, storage_migration_checkpoint, storage_policy,
    storage_policy_group, storage_policy_group_item, system_config, tag, team, team_member,
    upload_session, upload_session_part, user, user_invitation, user_profile, webdav_account,
    wopi_session,
};
use aster_drive::runtime::SharedRuntimeState;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, EntityName, EntityTrait, IdenStatic, Iterable,
    Statement,
};

#[derive(Debug)]
struct EntitySchema {
    table_name: &'static str,
    columns: BTreeSet<&'static str>,
}

macro_rules! entity_schema {
    ($entity:path) => {{
        type Entity = $entity;
        EntitySchema {
            table_name: Entity::default().table_name(),
            columns: <Entity as EntityTrait>::Column::iter()
                .map(|column| column.as_str())
                .collect(),
        }
    }};
}

// FIXME: ⚠️ 添加新 entity 时必须同步更新此列表。
// Keep every SeaORM Entity here via entity_schema!(module::Entity), such as
// user::Entity, file::Entity, folder::Entity, and any newly added entity module,
// so the schema drift test continues to cover the full migration/entity surface.
fn all_entity_schemas() -> Vec<EntitySchema> {
    vec![
        entity_schema!(audit_log::Entity),
        entity_schema!(auth_session::Entity),
        entity_schema!(background_task::Entity),
        entity_schema!(blob_media_metadata::Entity),
        entity_schema!(contact_verification_token::Entity),
        entity_schema!(entity_property::Entity),
        entity_schema!(external_auth_email_verification_flow::Entity),
        entity_schema!(external_auth_identity::Entity),
        entity_schema!(external_auth_login_flow::Entity),
        entity_schema!(external_auth_provider::Entity),
        entity_schema!(file::Entity),
        entity_schema!(file_blob::Entity),
        entity_schema!(file_version::Entity),
        entity_schema!(folder::Entity),
        entity_schema!(follower_enrollment_session::Entity),
        entity_schema!(mail_outbox::Entity),
        entity_schema!(managed_follower::Entity),
        entity_schema!(remote_storage_target::Entity),
        entity_schema!(master_binding::Entity),
        entity_schema!(mfa_email_code::Entity),
        entity_schema!(mfa_factor::Entity),
        entity_schema!(mfa_login_flow::Entity),
        entity_schema!(mfa_recovery_code::Entity),
        entity_schema!(mfa_totp_setup_flow::Entity),
        entity_schema!(passkey::Entity),
        entity_schema!(resource_lock::Entity),
        entity_schema!(share::Entity),
        entity_schema!(storage_migration_checkpoint::Entity),
        entity_schema!(storage_policy::Entity),
        entity_schema!(storage_policy_group::Entity),
        entity_schema!(storage_policy_group_item::Entity),
        entity_schema!(system_config::Entity),
        entity_schema!(tag::Entity),
        entity_schema!(team::Entity),
        entity_schema!(team_member::Entity),
        entity_schema!(upload_session::Entity),
        entity_schema!(upload_session_part::Entity),
        entity_schema!(user::Entity),
        entity_schema!(user_invitation::Entity),
        entity_schema!(user_profile::Entity),
        entity_schema!(webdav_account::Entity),
        entity_schema!(wopi_session::Entity),
    ]
}

async fn database_columns(
    db: &DatabaseConnection,
    table_name: &str,
) -> Result<BTreeSet<String>, sea_orm::DbErr> {
    let backend = db.get_database_backend();
    let rows = match backend {
        DbBackend::Sqlite => {
            let table_name = quote_sqlite_identifier(table_name);
            db.query_all_raw(Statement::from_string(
                backend,
                format!("PRAGMA table_info({table_name})"),
            ))
            .await?
        }
        DbBackend::Postgres => {
            let table_name = sql_string_literal(table_name);
            db.query_all_raw(Statement::from_string(
                backend,
                format!(
                    "SELECT column_name \
                     FROM information_schema.columns \
                     WHERE table_schema = 'public' AND table_name = {table_name} \
                     ORDER BY ordinal_position"
                ),
            ))
            .await?
        }
        DbBackend::MySql => {
            let table_name = sql_string_literal(table_name);
            db.query_all_raw(Statement::from_string(
                backend,
                format!(
                    "SELECT column_name \
                     FROM information_schema.columns \
                     WHERE table_schema = DATABASE() AND table_name = {table_name} \
                     ORDER BY ordinal_position"
                ),
            ))
            .await?
        }
        other => panic!("unsupported test database backend for schema drift check: {other:?}"),
    };

    let column_index = if backend == DbBackend::Sqlite { 1 } else { 0 };
    rows.into_iter()
        .map(|row| row.try_get_by_index::<String>(column_index))
        .collect()
}

fn quote_sqlite_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn format_set<T>(values: T) -> String
where
    T: IntoIterator,
    T::Item: std::fmt::Display,
{
    values
        .into_iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[actix_web::test]
async fn test_entity_columns_match_migrated_database_schema() {
    let state = common::setup().await;
    let db = state.writer_db();
    let mut mismatches = Vec::new();

    for entity in all_entity_schemas() {
        let actual = database_columns(db, entity.table_name)
            .await
            .unwrap_or_else(|err| panic!("failed to inspect table {}: {err}", entity.table_name));
        let expected = entity
            .columns
            .iter()
            .map(|column| (*column).to_string())
            .collect::<BTreeSet<_>>();

        if actual != expected {
            let missing = expected.difference(&actual).collect::<Vec<_>>();
            let extra = actual.difference(&expected).collect::<Vec<_>>();
            mismatches.push(format!(
                "{}: missing=[{}], extra=[{}]",
                entity.table_name,
                format_set(missing),
                format_set(extra)
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "migration/entity schema drift detected:\n{}",
        mismatches.join("\n")
    );
}
