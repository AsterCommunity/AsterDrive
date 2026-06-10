use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, sea_query::Expr,
};

use crate::entities::file::{self, Entity as File};
use crate::errors::{AsterError, Result};

use crate::db::repository::file_repo::common::{
    FileScope, active_scope_condition, apply_folder_condition, scope_condition,
};

pub(crate) type FileIdSize = (i64, i64);

fn sum_as_i64_expr(
    backend: DbBackend,
    column: impl sea_orm::sea_query::IntoColumnRef + Copy,
) -> sea_orm::sea_query::SimpleExpr {
    let type_name = match backend {
        DbBackend::Postgres => "bigint",
        DbBackend::MySql => "signed",
        _ => "integer",
    };
    Expr::col(column).sum().cast_as(type_name)
}

/// 统计未删除文件总数
pub async fn count_live_files(db: &DatabaseConnection) -> Result<u64> {
    File::find()
        .filter(file::Column::DeletedAt.is_null())
        .count(db)
        .await
        .map_err(AsterError::from)
}

/// 统计未删除文件总字节数
pub async fn sum_live_file_bytes(db: &DatabaseConnection) -> Result<i64> {
    Ok(File::find()
        .select_only()
        .column_as(
            sum_as_i64_expr(db.get_database_backend(), file::Column::Size),
            "sum",
        )
        .filter(file::Column::DeletedAt.is_null())
        .into_tuple::<Option<i64>>()
        .one(db)
        .await?
        .flatten()
        .unwrap_or(0))
}

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<file::Model> {
    File::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::file_not_found(format!("file #{id}")))
}

/// 以排他锁读取文件记录，用于防止并发操作同一文件时的竞态。
///
/// - Postgres/MySQL：使用 `SELECT ... FOR UPDATE`，有真正的行锁保障。
/// - SQLite：`FOR UPDATE` 不被支持，fallback 到普通读。SQLite 的写操作本身依赖 WAL 写锁，
///   对于 AsterDrive 的写入场景（覆盖上传等）已有 blob ref_count 原子操作兜底，
///   此函数在 SQLite 上的并发保护能力有限，设计上接受这一限制。
pub async fn lock_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<file::Model> {
    match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => File::find_by_id(id)
            .lock_exclusive()
            .one(db)
            .await
            .map_err(AsterError::from)?
            .ok_or_else(|| AsterError::file_not_found(format!("file #{id}"))),
        DbBackend::Sqlite => find_by_id(db, id).await,
        _ => find_by_id(db, id).await,
    }
}

pub async fn find_by_ids<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<Vec<file::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    File::find()
        .filter(file::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub(super) async fn find_by_ids_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    ids: &[i64],
) -> Result<Vec<file::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    File::find()
        .filter(scope_condition(scope))
        .filter(file::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_ids_in_personal_scope<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    ids: &[i64],
) -> Result<Vec<file::Model>> {
    find_by_ids_in_scope(db, FileScope::Personal { user_id }, ids).await
}

pub async fn find_by_ids_in_team_scope<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    ids: &[i64],
) -> Result<Vec<file::Model>> {
    find_by_ids_in_scope(db, FileScope::Team { team_id }, ids).await
}

/// 批量查询多个文件夹下的未删除文件
pub async fn find_by_folders<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    folder_ids: &[i64],
) -> Result<Vec<file::Model>> {
    find_by_folders_in_scope(db, FileScope::Personal { user_id }, folder_ids).await
}

pub async fn find_by_team_folders<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    folder_ids: &[i64],
) -> Result<Vec<file::Model>> {
    find_by_folders_in_scope(db, FileScope::Team { team_id }, folder_ids).await
}

/// 详情占用统计专用的轻量 cursor 查询。
///
/// 只返回 `(file_id, size)`，并按 `files.id` 做 cursor 分页；调用方应逐页累加并释放
/// `file_ids`，避免打开大目录详情时把整棵树的文件记录或所有文件 ID 留在内存里。
pub(crate) async fn find_id_size_by_folders<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_ids: &[i64],
    after_id: Option<i64>,
    limit: u64,
) -> Result<Vec<FileIdSize>> {
    if folder_ids.is_empty() || limit == 0 {
        return Ok(vec![]);
    }
    let mut query = File::find()
        .select_only()
        .column(file::Column::Id)
        .column(file::Column::Size)
        .filter(active_scope_condition(scope))
        .filter(file::Column::FolderId.is_in(folder_ids.iter().copied()))
        .order_by_asc(file::Column::Id)
        .limit(limit);
    if let Some(after_id) = after_id {
        query = query.filter(file::Column::Id.gt(after_id));
    }
    query
        .into_tuple::<FileIdSize>()
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 批量查询多个文件夹下的文件（含已删除）
pub async fn find_all_in_folders<C: ConnectionTrait>(
    db: &C,
    folder_ids: &[i64],
) -> Result<Vec<file::Model>> {
    if folder_ids.is_empty() {
        return Ok(vec![]);
    }
    File::find()
        .filter(file::Column::FolderId.is_in(folder_ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查询文件夹下的文件（排除已删除）
pub async fn find_by_folder<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    folder_id: Option<i64>,
) -> Result<Vec<file::Model>> {
    find_by_folder_in_scope(db, FileScope::Personal { user_id }, folder_id).await
}

pub async fn find_by_team_folder<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    folder_id: Option<i64>,
) -> Result<Vec<file::Model>> {
    find_by_folder_in_scope(db, FileScope::Team { team_id }, folder_id).await
}

pub(super) async fn find_by_folders_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_ids: &[i64],
) -> Result<Vec<file::Model>> {
    if folder_ids.is_empty() {
        return Ok(vec![]);
    }
    File::find()
        .filter(active_scope_condition(scope))
        .filter(file::Column::FolderId.is_in(folder_ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub(super) async fn find_by_folder_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_id: Option<i64>,
) -> Result<Vec<file::Model>> {
    File::find()
        .filter(apply_folder_condition(
            active_scope_condition(scope),
            folder_id,
        ))
        .order_by_asc(file::Column::Name)
        .all(db)
        .await
        .map_err(AsterError::from)
}
