//! 仓储模块：`upload_session_repo`。

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::upload_session::{self, Entity as UploadSession};
use aster_drive_model::types::{UploadSessionKind, UploadSessionStatus};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, SqlErr, sea_query::Expr,
};

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: &str) -> Result<upload_session::Model> {
    UploadSession::find_by_id(id.to_string())
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::upload_session_not_found(format!("session {id}")))
}

pub async fn lock_by_id<C: ConnectionTrait>(db: &C, id: &str) -> Result<upload_session::Model> {
    match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => UploadSession::find_by_id(id.to_string())
            .lock_exclusive()
            .one(db)
            .await
            .map_err(AsterError::from)?
            .ok_or_else(|| AsterError::upload_session_not_found(format!("session {id}"))),
        _ => find_by_id(db, id).await,
    }
}

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: upload_session::ActiveModel,
) -> Result<upload_session::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn try_create<C: ConnectionTrait>(
    db: &C,
    model: upload_session::ActiveModel,
) -> Result<bool> {
    let id =
        model.id.try_as_ref().cloned().ok_or_else(|| {
            AsterError::internal_error("upload session id must be set before insert")
        })?;

    match UploadSession::insert(model)
        .exec_without_returning(db)
        .await
    {
        Ok(1) => Ok(true),
        Ok(rows) => Err(AsterError::internal_error(format!(
            "upload session insert affected {rows} rows"
        ))),
        Err(err) => {
            if is_unique_conflict_db_err(&err) && upload_session_id_exists(db, &id).await? {
                Ok(false)
            } else {
                Err(AsterError::from(err))
            }
        }
    }
}

fn is_unique_conflict_db_err(err: &sea_orm::DbErr) -> bool {
    matches!(err.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

async fn upload_session_id_exists<C: ConnectionTrait>(db: &C, id: &str) -> Result<bool> {
    let found = UploadSession::find_by_id(id.to_string())
        .select_only()
        .column(upload_session::Column::Id)
        .into_tuple::<String>()
        .one(db)
        .await
        .map_err(AsterError::from)?;
    Ok(found.is_some())
}

pub async fn update<C: ConnectionTrait>(
    db: &C,
    model: upload_session::ActiveModel,
) -> Result<upload_session::Model> {
    model.update(db).await.map_err(AsterError::from)
}

pub async fn delete<C: ConnectionTrait>(db: &C, id: &str) -> Result<()> {
    UploadSession::delete_by_id(id.to_string())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn increment_received_count_if_uploading<C: ConnectionTrait>(
    db: &C,
    id: &str,
) -> Result<bool> {
    let result = UploadSession::update_many()
        .col_expr(
            upload_session::Column::ReceivedCount,
            Expr::col(upload_session::Column::ReceivedCount).add(1),
        )
        .col_expr(
            upload_session::Column::UpdatedAt,
            Expr::value(chrono::Utc::now()),
        )
        .filter(upload_session::Column::Id.eq(id))
        .filter(upload_session::Column::Status.eq(UploadSessionStatus::Uploading))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn advance_provider_relay_received_count<C: ConnectionTrait>(
    db: &C,
    id: &str,
    expected_received_count: i32,
) -> Result<bool> {
    let result = UploadSession::update_many()
        .col_expr(
            upload_session::Column::ReceivedCount,
            Expr::col(upload_session::Column::ReceivedCount).add(1),
        )
        .col_expr(
            upload_session::Column::UpdatedAt,
            Expr::value(chrono::Utc::now()),
        )
        .filter(upload_session::Column::Id.eq(id))
        .filter(upload_session::Column::SessionKind.eq(UploadSessionKind::ProviderRelayResumable))
        .filter(upload_session::Column::Status.eq(UploadSessionStatus::Uploading))
        .filter(upload_session::Column::ReceivedCount.eq(expected_received_count))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn complete_if_assembling<C: ConnectionTrait>(
    db: &C,
    id: &str,
    file_id: i64,
) -> Result<bool> {
    use sea_orm::ActiveEnum;

    let result = UploadSession::update_many()
        .col_expr(
            upload_session::Column::Status,
            Expr::value(UploadSessionStatus::Completed.to_value()),
        )
        .col_expr(upload_session::Column::FileId, Expr::value(Some(file_id)))
        .col_expr(
            upload_session::Column::UpdatedAt,
            Expr::value(chrono::Utc::now()),
        )
        .filter(upload_session::Column::Id.eq(id))
        .filter(upload_session::Column::Status.eq(UploadSessionStatus::Assembling))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

/// 原子状态转换：只有当前状态匹配 expected 时才更新为 new_status。
/// 返回转换是否成功（false = 状态已被其他请求抢占）。
pub async fn try_transition_status<C: ConnectionTrait>(
    db: &C,
    id: &str,
    expected: UploadSessionStatus,
    new_status: UploadSessionStatus,
) -> Result<bool> {
    use sea_orm::ActiveEnum;
    let result = UploadSession::update_many()
        .col_expr(
            upload_session::Column::Status,
            sea_orm::sea_query::Expr::value(new_status.to_value()),
        )
        .col_expr(
            upload_session::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now()),
        )
        .filter(upload_session::Column::Id.eq(id))
        .filter(upload_session::Column::Status.eq(expected))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected > 0)
}

/// 原子终止仍处于指定 active 状态的 session，并把 expiry 缩短到 cleanup 重试窗口。
///
/// upload-stage 请求可能并发执行；状态条件保证已经进入 assembling/completed/failed
/// 的 session 不会被较晚返回的错误反向覆盖。
pub async fn try_fail_with_expiration<C: ConnectionTrait>(
    db: &C,
    id: &str,
    expected: UploadSessionStatus,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<bool> {
    use sea_orm::ActiveEnum;
    let now = chrono::Utc::now();
    let result = UploadSession::update_many()
        .col_expr(
            upload_session::Column::Status,
            sea_orm::sea_query::Expr::value(UploadSessionStatus::Failed.to_value()),
        )
        .col_expr(
            upload_session::Column::ExpiresAt,
            sea_orm::sea_query::Expr::value(expires_at),
        )
        .col_expr(
            upload_session::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(now),
        )
        .filter(upload_session::Column::Id.eq(id))
        .filter(upload_session::Column::Status.eq(expected))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected > 0)
}

/// 原子状态转换：只有状态匹配且 session 尚未过期时才更新。
pub async fn try_transition_status_before_expiry<C: ConnectionTrait>(
    db: &C,
    id: &str,
    expected: UploadSessionStatus,
    new_status: UploadSessionStatus,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<bool> {
    use sea_orm::ActiveEnum;
    let result = UploadSession::update_many()
        .col_expr(
            upload_session::Column::Status,
            sea_orm::sea_query::Expr::value(new_status.to_value()),
        )
        .col_expr(
            upload_session::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(now),
        )
        .filter(upload_session::Column::Id.eq(id))
        .filter(upload_session::Column::Status.eq(expected))
        .filter(upload_session::Column::ExpiresAt.gt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected > 0)
}

/// 查找所有过期且未完成的 session
pub async fn find_expired<C: ConnectionTrait>(db: &C) -> Result<Vec<upload_session::Model>> {
    let now = chrono::Utc::now();
    UploadSession::find()
        .filter(upload_session::Column::ExpiresAt.lt(now))
        .filter(upload_session::Column::Status.is_in([
            UploadSessionStatus::Uploading,
            UploadSessionStatus::Presigned,
            UploadSessionStatus::Failed,
        ]))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_team<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
) -> Result<Vec<upload_session::Model>> {
    UploadSession::find()
        .filter(upload_session::Column::TeamId.eq(team_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<Vec<upload_session::Model>> {
    UploadSession::find()
        .filter(upload_session::Column::PolicyId.eq(policy_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn count_by_policy<C: ConnectionTrait>(db: &C, policy_id: i64) -> Result<u64> {
    UploadSession::find()
        .filter(upload_session::Column::PolicyId.eq(policy_id))
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn count_active_by_policy<C: ConnectionTrait>(db: &C, policy_id: i64) -> Result<u64> {
    let now = chrono::Utc::now();
    UploadSession::find()
        .filter(upload_session::Column::PolicyId.eq(policy_id))
        .filter(upload_session::Column::ExpiresAt.gt(now))
        .filter(upload_session::Column::Status.is_in([
            UploadSessionStatus::Uploading,
            UploadSessionStatus::Assembling,
            UploadSessionStatus::Presigned,
        ]))
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_recoverable_by_owner<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    team_id: Option<i64>,
    frontend_client_id: Option<&str>,
    limit: u64,
) -> Result<Vec<upload_session::Model>> {
    let now = chrono::Utc::now();
    let mut query = UploadSession::find()
        .filter(upload_session::Column::UserId.eq(user_id))
        .filter(upload_session::Column::ExpiresAt.gt(now))
        .filter(upload_session::Column::Status.is_in([
            UploadSessionStatus::Uploading,
            UploadSessionStatus::Assembling,
            UploadSessionStatus::Presigned,
        ]))
        .order_by_desc(upload_session::Column::UpdatedAt)
        .order_by_desc(upload_session::Column::Id)
        .limit(limit);

    query = match team_id {
        Some(team_id) => query.filter(upload_session::Column::TeamId.eq(team_id)),
        None => query.filter(upload_session::Column::TeamId.is_null()),
    };

    if let Some(frontend_client_id) = frontend_client_id {
        query = query.filter(upload_session::Column::FrontendClientId.eq(frontend_client_id));
    }

    query.all(db).await.map_err(AsterError::from)
}

pub async fn list_temp_keys_by_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<Vec<String>> {
    let keys = UploadSession::find()
        .select_only()
        .column(upload_session::Column::ObjectTempKey)
        .filter(upload_session::Column::PolicyId.eq(policy_id))
        .filter(upload_session::Column::ObjectTempKey.is_not_null())
        .into_tuple::<Option<String>>()
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok(keys.into_iter().flatten().collect())
}

/// 批量删除用户的所有上传会话
pub async fn delete_all_by_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let res = UploadSession::delete_many()
        .filter(upload_session::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

pub async fn delete_all_by_team<C: ConnectionTrait>(db: &C, team_id: i64) -> Result<u64> {
    let res = UploadSession::delete_many()
        .filter(upload_session::Column::TeamId.eq(team_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

/// 批量查询已完成且已过期的 upload session（cursor 分页，id 升序）
pub async fn find_expired_completed_paginated<C: ConnectionTrait>(
    db: &C,
    now: chrono::DateTime<chrono::Utc>,
    after_id: Option<&str>,
    limit: u64,
) -> Result<Vec<upload_session::Model>> {
    let mut query = UploadSession::find()
        .filter(upload_session::Column::ExpiresAt.lt(now))
        .filter(upload_session::Column::Status.eq(UploadSessionStatus::Completed))
        .order_by_asc(upload_session::Column::Id)
        .limit(limit);
    if let Some(last_id) = after_id {
        query = query.filter(upload_session::Column::Id.gt(last_id.to_string()));
    }
    query.all(db).await.map_err(AsterError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use sea_orm::{ActiveModelTrait, ConnectOptions, Database, DbBackend, IntoActiveModel, Schema};

    async fn build_test_db() -> sea_orm::DatabaseConnection {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.max_connections(1);
        let db = Database::connect(options)
            .await
            .expect("upload session repo test DB should connect");
        db.execute_unprepared("PRAGMA foreign_keys = OFF")
            .await
            .expect("upload session repo test DB should disable unrelated foreign keys");
        let schema = Schema::new(DbBackend::Sqlite);
        db.execute(&schema.create_table_from_entity(upload_session::Entity))
            .await
            .expect("upload session test table should be created");
        db
    }

    fn session(id: &str, status: UploadSessionStatus) -> upload_session::Model {
        let now = Utc::now();
        upload_session::Model {
            id: id.to_string(),
            user_id: 7,
            team_id: None,
            frontend_client_id: Some("frontend-1".to_string()),
            filename: format!("{id}.bin"),
            mime_type: "application/octet-stream".to_string(),
            total_size: 10,
            chunk_size: 5,
            total_chunks: 2,
            received_count: 0,
            folder_id: None,
            policy_id: 1,
            placement_profile_id: None,
            placement_rule_id: None,
            placement_revision: None,
            placement_execution_preference: "automatic".to_string(),
            status,
            session_kind: UploadSessionKind::OffsetStaging,
            object_temp_key: None,
            object_multipart_id: None,
            provider_session_ciphertext: None,
            file_id: None,
            created_at: now,
            expires_at: now + Duration::hours(1),
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn fail_with_expiration_hides_active_session_and_blocks_late_receipts() {
        for initial_status in [
            UploadSessionStatus::Uploading,
            UploadSessionStatus::Presigned,
        ] {
            let db = build_test_db().await;
            let model = session("terminal", initial_status);
            model
                .clone()
                .into_active_model()
                .insert(&db)
                .await
                .expect("active upload session should insert");

            let recoverable = find_recoverable_by_owner(
                &db,
                model.user_id,
                None,
                model.frontend_client_id.as_deref(),
                10,
            )
            .await
            .expect("active session lookup should succeed");
            assert_eq!(recoverable.len(), 1);

            let short_expiry = Utc::now() + Duration::seconds(15);
            assert!(
                try_fail_with_expiration(&db, &model.id, initial_status, short_expiry)
                    .await
                    .expect("terminal transition should execute")
            );

            let failed = find_by_id(&db, &model.id)
                .await
                .expect("failed session should remain available for cleanup");
            assert_eq!(failed.status, UploadSessionStatus::Failed);
            assert_eq!(failed.expires_at, short_expiry);
            assert!(
                find_recoverable_by_owner(
                    &db,
                    model.user_id,
                    None,
                    model.frontend_client_id.as_deref(),
                    10,
                )
                .await
                .expect("recoverable lookup should succeed")
                .is_empty()
            );
            assert!(
                !increment_received_count_if_uploading(&db, &model.id)
                    .await
                    .expect("late receipt guard should execute")
            );
            assert_eq!(find_by_id(&db, &model.id).await.unwrap().received_count, 0);
        }
    }

    #[tokio::test]
    async fn fail_with_expiration_does_not_overwrite_a_concurrent_state_change() {
        let db = build_test_db().await;
        let model = session("assembling", UploadSessionStatus::Assembling);
        model
            .clone()
            .into_active_model()
            .insert(&db)
            .await
            .expect("assembling session should insert");
        let original_expiry = model.expires_at;

        assert!(
            !try_fail_with_expiration(
                &db,
                &model.id,
                UploadSessionStatus::Uploading,
                Utc::now() + Duration::seconds(15),
            )
            .await
            .expect("stale terminal transition should execute")
        );

        let unchanged = find_by_id(&db, &model.id).await.unwrap();
        assert_eq!(unchanged.status, UploadSessionStatus::Assembling);
        assert_eq!(unchanged.expires_at, original_expiry);
    }
}
