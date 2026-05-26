//! 仓储模块：`upload_session_repo`。

use crate::entities::upload_session::{self, Entity as UploadSession};
use crate::errors::{AsterError, Result};
use crate::types::UploadSessionStatus;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, SqlErr, sea_query::Expr,
};

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: &str) -> Result<upload_session::Model> {
    UploadSession::find_by_id(id.to_string())
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::upload_session_not_found(format!("session {id}")))
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
        .column(upload_session::Column::S3TempKey)
        .filter(upload_session::Column::PolicyId.eq(policy_id))
        .filter(upload_session::Column::S3TempKey.is_not_null())
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
