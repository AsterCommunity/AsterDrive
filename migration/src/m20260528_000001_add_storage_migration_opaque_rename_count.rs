//! 数据库迁移：记录存储迁移中因 opaque key 冲突而重命名的 blob 数量。

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

const STORAGE_POLICY_MIGRATION_KIND: &str = "storage_policy_migration";
const RENAMED_OPAQUE_BLOBS_FIELD: &str = "renamed_opaque_blobs";
const BACKFILL_BATCH_SIZE: u64 = 500;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(StorageMigrationCheckpoints::Table)
                    .add_column(
                        ColumnDef::new(StorageMigrationCheckpoints::RenamedOpaqueBlobs)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        backfill_storage_migration_task_results(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(StorageMigrationCheckpoints::Table)
                    .drop_column(StorageMigrationCheckpoints::RenamedOpaqueBlobs)
                    .to_owned(),
            )
            .await
    }
}

async fn backfill_storage_migration_task_results(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut last_id = 0_i64;

    loop {
        let select = Query::select()
            .column(BackgroundTasks::Id)
            .column(BackgroundTasks::ResultJson)
            .from(BackgroundTasks::Table)
            .and_where(Expr::col(BackgroundTasks::Kind).eq(STORAGE_POLICY_MIGRATION_KIND))
            .and_where(Expr::col(BackgroundTasks::ResultJson).is_not_null())
            .and_where(Expr::col(BackgroundTasks::Id).gt(last_id))
            .order_by(BackgroundTasks::Id, Order::Asc)
            .limit(BACKFILL_BATCH_SIZE)
            .to_owned();
        let rows = manager
            .get_connection()
            .query_all(&select)
            .await
            .map_err(|error| {
                DbErr::Migration(format!(
                    "failed to load storage migration task results for opaque rename backfill: {error}"
                ))
            })?;
        if rows.is_empty() {
            break;
        }

        for row in rows {
            let id = row.try_get_by_index::<i64>(0).map_err(|error| {
                DbErr::Migration(format!(
                    "failed to decode storage migration task id during opaque rename backfill: {error}"
                ))
            })?;
            last_id = id;
            let Some(raw_result) = row.try_get_by_index::<Option<String>>(1).map_err(|error| {
                DbErr::Migration(format!(
                    "failed to decode storage migration task #{id} result during opaque rename backfill: {error}"
                ))
            })? else {
                continue;
            };

            let mut result =
                serde_json::from_str::<serde_json::Value>(&raw_result).map_err(|error| {
                    DbErr::Migration(format!(
                        "failed to parse storage migration task #{id} result during opaque rename backfill: {error}"
                    ))
                })?;
            let Some(object) = result.as_object_mut() else {
                return Err(DbErr::Migration(format!(
                    "storage migration task #{id} result is not a JSON object"
                )));
            };

            if object.contains_key(RENAMED_OPAQUE_BLOBS_FIELD) {
                continue;
            }

            object.insert(
                RENAMED_OPAQUE_BLOBS_FIELD.to_string(),
                serde_json::Value::Number(0.into()),
            );
            let updated_result = serde_json::to_string(&result).map_err(|error| {
                DbErr::Migration(format!(
                    "failed to serialize storage migration task #{id} result during opaque rename backfill: {error}"
                ))
            })?;

            manager
                .get_connection()
                .execute(
                    &Query::update()
                        .table(BackgroundTasks::Table)
                        .value(BackgroundTasks::ResultJson, updated_result)
                        .and_where(Expr::col(BackgroundTasks::Id).eq(id))
                        .to_owned(),
                )
                .await
                .map_err(|error| {
                    DbErr::Migration(format!(
                        "failed to update storage migration task #{id} result during opaque rename backfill: {error}"
                    ))
                })?;
        }
    }

    Ok(())
}

#[derive(DeriveIden)]
enum StorageMigrationCheckpoints {
    Table,
    RenamedOpaqueBlobs,
}

#[derive(DeriveIden)]
enum BackgroundTasks {
    Table,
    Id,
    Kind,
    ResultJson,
}
