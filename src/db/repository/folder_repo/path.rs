//! `folder_repo` 仓储子模块：`path`。

use sea_orm::{
    Condition, ConnectionTrait, DbBackend, EntityTrait, ExprTrait, FromQueryResult, Statement,
    entity::prelude::DeriveIden,
    sea_query::{Asterisk, CommonTableExpression, Expr, Order, Query, UnionType, WithClause},
};

use crate::entities::folder::{self, Entity as Folder};
use crate::errors::{AsterError, Result};
use crate::utils::numbers::usize_to_i64;

use super::query::{find_by_id, find_by_name_in_parent};

#[derive(Debug, Clone, FromQueryResult)]
struct ResolvedPathFolderRow {
    segment_index: i64,
    id: i64,
    name: String,
    parent_id: Option<i64>,
    team_id: Option<i64>,
    owner_user_id: Option<i64>,
    created_by_user_id: Option<i64>,
    created_by_username: String,
    policy_id: Option<i64>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    is_locked: bool,
}

impl From<ResolvedPathFolderRow> for folder::Model {
    fn from(row: ResolvedPathFolderRow) -> Self {
        let _ = row.segment_index;
        Self {
            id: row.id,
            name: row.name,
            parent_id: row.parent_id,
            team_id: row.team_id,
            owner_user_id: row.owner_user_id,
            created_by_user_id: row.created_by_user_id,
            created_by_username: row.created_by_username,
            policy_id: row.policy_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
            deleted_at: row.deleted_at,
            is_locked: row.is_locked,
        }
    }
}

#[derive(Debug, Clone, FromQueryResult)]
struct AncestorFolderRow {
    depth: i64,
    id: i64,
    name: String,
    parent_id: Option<i64>,
    team_id: Option<i64>,
    owner_user_id: Option<i64>,
    created_by_user_id: Option<i64>,
    created_by_username: String,
    policy_id: Option<i64>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    is_locked: bool,
}

impl From<AncestorFolderRow> for folder::Model {
    fn from(row: AncestorFolderRow) -> Self {
        let _ = row.depth;
        Self {
            id: row.id,
            name: row.name,
            parent_id: row.parent_id,
            team_id: row.team_id,
            owner_user_id: row.owner_user_id,
            created_by_user_id: row.created_by_user_id,
            created_by_username: row.created_by_username,
            policy_id: row.policy_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
            deleted_at: row.deleted_at,
            is_locked: row.is_locked,
        }
    }
}

#[derive(DeriveIden)]
enum RequestedSegments {
    Table,
    Column1,
    Column2,
}

#[derive(DeriveIden)]
enum RequestedValues {
    Table,
}

#[derive(DeriveIden)]
enum FolderChain {
    Table,
    SegmentIndex,
    Id,
    Name,
    ParentId,
    TeamId,
    OwnerUserId,
    CreatedByUserId,
    CreatedByUsername,
    PolicyId,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
    IsLocked,
}

#[derive(Clone, Copy)]
enum AncestorScope {
    Owner { user_id: i64 },
    Team { team_id: i64 },
}

fn build_find_ancestors_statement(
    backend: DbBackend,
    scope: AncestorScope,
    folder_id: i64,
) -> Statement {
    let root_depth_expr = match backend {
        DbBackend::Postgres => "CAST(0 AS BIGINT)",
        DbBackend::MySql => "CAST(0 AS SIGNED)",
        _ => "CAST(0 AS INTEGER)",
    };
    let (sql, values) = match (backend, scope) {
        (DbBackend::Postgres, AncestorScope::Owner { user_id }) => (
            format!(
                "WITH RECURSIVE folder_ancestors ( \
                 depth, id, name, parent_id, team_id, owner_user_id, created_by_user_id, created_by_username, policy_id, created_at, updated_at, deleted_at, is_locked \
             ) AS ( \
                 SELECT \
                     {root_depth_expr} AS depth, \
                     f.id, \
                     f.name, \
                     f.parent_id, \
                     f.team_id, \
                     f.owner_user_id, \
                     f.created_by_user_id, \
                     f.created_by_username, \
                     f.policy_id, \
                     f.created_at, \
                     f.updated_at, \
                     f.deleted_at, \
                     f.is_locked \
                 FROM folders f \
                 WHERE f.id = $1 \
                   AND f.owner_user_id = $2 \
                   AND f.deleted_at IS NULL \
                 UNION ALL \
                 SELECT \
                     fa.depth + 1, \
                     parent.id, \
                     parent.name, \
                     parent.parent_id, \
                     parent.team_id, \
                     parent.owner_user_id, \
                     parent.created_by_user_id, \
                     parent.created_by_username, \
                     parent.policy_id, \
                     parent.created_at, \
                     parent.updated_at, \
                     parent.deleted_at, \
                     parent.is_locked \
                 FROM folders parent \
                 JOIN folder_ancestors fa ON fa.parent_id = parent.id \
                 WHERE parent.owner_user_id = $2 \
                   AND parent.deleted_at IS NULL \
             ) \
             SELECT \
                 depth, id, name, parent_id, team_id, owner_user_id, created_by_user_id, created_by_username, policy_id, created_at, updated_at, deleted_at, is_locked \
             FROM folder_ancestors \
             ORDER BY depth DESC"
            ),
            vec![folder_id.into(), user_id.into()],
        ),
        (DbBackend::Postgres, AncestorScope::Team { team_id }) => (
            format!(
                "WITH RECURSIVE folder_ancestors ( \
                 depth, id, name, parent_id, team_id, owner_user_id, created_by_user_id, created_by_username, policy_id, created_at, updated_at, deleted_at, is_locked \
             ) AS ( \
                 SELECT \
                     {root_depth_expr} AS depth, \
                     f.id, \
                     f.name, \
                     f.parent_id, \
                     f.team_id, \
                     f.owner_user_id, \
                     f.created_by_user_id, \
                     f.created_by_username, \
                     f.policy_id, \
                     f.created_at, \
                     f.updated_at, \
                     f.deleted_at, \
                     f.is_locked \
                 FROM folders f \
                 WHERE f.id = $1 \
                   AND f.team_id = $2 \
                   AND f.deleted_at IS NULL \
                 UNION ALL \
                 SELECT \
                     fa.depth + 1, \
                     parent.id, \
                     parent.name, \
                     parent.parent_id, \
                     parent.team_id, \
                     parent.owner_user_id, \
                     parent.created_by_user_id, \
                     parent.created_by_username, \
                     parent.policy_id, \
                     parent.created_at, \
                     parent.updated_at, \
                     parent.deleted_at, \
                     parent.is_locked \
                 FROM folders parent \
                 JOIN folder_ancestors fa ON fa.parent_id = parent.id \
                 WHERE parent.team_id = $2 \
                   AND parent.deleted_at IS NULL \
             ) \
             SELECT \
                 depth, id, name, parent_id, team_id, owner_user_id, created_by_user_id, created_by_username, policy_id, created_at, updated_at, deleted_at, is_locked \
             FROM folder_ancestors \
             ORDER BY depth DESC"
            ),
            vec![folder_id.into(), team_id.into()],
        ),
        (_, AncestorScope::Owner { user_id }) => (
            format!(
                "WITH RECURSIVE folder_ancestors ( \
                 depth, id, name, parent_id, team_id, owner_user_id, created_by_user_id, created_by_username, policy_id, created_at, updated_at, deleted_at, is_locked \
             ) AS ( \
                 SELECT \
                     {root_depth_expr} AS depth, \
                     f.id, \
                     f.name, \
                     f.parent_id, \
                     f.team_id, \
                     f.owner_user_id, \
                     f.created_by_user_id, \
                     f.created_by_username, \
                     f.policy_id, \
                     f.created_at, \
                     f.updated_at, \
                     f.deleted_at, \
                     f.is_locked \
                 FROM folders f \
                 WHERE f.id = ? \
                   AND f.owner_user_id = ? \
                   AND f.deleted_at IS NULL \
                 UNION ALL \
                 SELECT \
                     fa.depth + 1, \
                     parent.id, \
                     parent.name, \
                     parent.parent_id, \
                     parent.team_id, \
                     parent.owner_user_id, \
                     parent.created_by_user_id, \
                     parent.created_by_username, \
                     parent.policy_id, \
                     parent.created_at, \
                     parent.updated_at, \
                     parent.deleted_at, \
                     parent.is_locked \
                 FROM folders parent \
                 JOIN folder_ancestors fa ON fa.parent_id = parent.id \
                 WHERE parent.owner_user_id = ? \
                   AND parent.deleted_at IS NULL \
             ) \
             SELECT \
                 depth, id, name, parent_id, team_id, owner_user_id, created_by_user_id, created_by_username, policy_id, created_at, updated_at, deleted_at, is_locked \
             FROM folder_ancestors \
             ORDER BY depth DESC"
            ),
            vec![folder_id.into(), user_id.into(), user_id.into()],
        ),
        (_, AncestorScope::Team { team_id }) => (
            format!(
                "WITH RECURSIVE folder_ancestors ( \
                 depth, id, name, parent_id, team_id, owner_user_id, created_by_user_id, created_by_username, policy_id, created_at, updated_at, deleted_at, is_locked \
             ) AS ( \
                 SELECT \
                     {root_depth_expr} AS depth, \
                     f.id, \
                     f.name, \
                     f.parent_id, \
                     f.team_id, \
                     f.owner_user_id, \
                     f.created_by_user_id, \
                     f.created_by_username, \
                     f.policy_id, \
                     f.created_at, \
                     f.updated_at, \
                     f.deleted_at, \
                     f.is_locked \
                 FROM folders f \
                 WHERE f.id = ? \
                   AND f.team_id = ? \
                   AND f.deleted_at IS NULL \
                 UNION ALL \
                 SELECT \
                     fa.depth + 1, \
                     parent.id, \
                     parent.name, \
                     parent.parent_id, \
                     parent.team_id, \
                     parent.owner_user_id, \
                     parent.created_by_user_id, \
                     parent.created_by_username, \
                     parent.policy_id, \
                     parent.created_at, \
                     parent.updated_at, \
                     parent.deleted_at, \
                     parent.is_locked \
                 FROM folders parent \
                 JOIN folder_ancestors fa ON fa.parent_id = parent.id \
                 WHERE parent.team_id = ? \
                   AND parent.deleted_at IS NULL \
             ) \
             SELECT \
                 depth, id, name, parent_id, team_id, owner_user_id, created_by_user_id, created_by_username, policy_id, created_at, updated_at, deleted_at, is_locked \
             FROM folder_ancestors \
             ORDER BY depth DESC"
            ),
            vec![folder_id.into(), team_id.into(), team_id.into()],
        ),
    };

    Statement::from_sql_and_values(backend, sql, values)
}

async fn find_ancestor_models_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: AncestorScope,
    folder_id: i64,
) -> Result<Vec<folder::Model>> {
    let rows = AncestorFolderRow::find_by_statement(build_find_ancestors_statement(
        db.get_database_backend(),
        scope,
        folder_id,
    ))
    .all(db)
    .await
    .map_err(AsterError::from)?;

    Ok(rows.into_iter().map(Into::into).collect())
}

fn requested_segments_subquery(segments: &[String]) -> Result<sea_orm::sea_query::SelectStatement> {
    Ok(Query::select()
        .column(Asterisk)
        .from_values(
            segments
                .iter()
                .enumerate()
                .map(|(idx, segment)| {
                    Ok((
                        usize_to_i64(idx.saturating_add(1), "requested path segment index")?,
                        segment.clone(),
                    ))
                })
                .collect::<Result<Vec<_>>>()?,
            RequestedValues::Table,
        )
        .to_owned())
}

fn build_resolve_path_chain_query(
    user_id: i64,
    root_parent_id: Option<i64>,
    segments: &[String],
) -> Result<sea_orm::sea_query::WithQuery> {
    let base_requested = requested_segments_subquery(segments)?;
    let recursive_requested = requested_segments_subquery(segments)?;

    let mut base_select = Query::select();
    base_select
        .column((RequestedSegments::Table, RequestedSegments::Column1))
        .column((folder::Entity, folder::Column::Id))
        .column((folder::Entity, folder::Column::Name))
        .column((folder::Entity, folder::Column::ParentId))
        .column((folder::Entity, folder::Column::TeamId))
        .column((folder::Entity, folder::Column::OwnerUserId))
        .column((folder::Entity, folder::Column::CreatedByUserId))
        .column((folder::Entity, folder::Column::CreatedByUsername))
        .column((folder::Entity, folder::Column::PolicyId))
        .column((folder::Entity, folder::Column::CreatedAt))
        .column((folder::Entity, folder::Column::UpdatedAt))
        .column((folder::Entity, folder::Column::DeletedAt))
        .column((folder::Entity, folder::Column::IsLocked))
        .from(folder::Entity)
        .join_subquery(
            sea_orm::JoinType::InnerJoin,
            base_requested,
            RequestedSegments::Table,
            Condition::all()
                .add(Expr::col((RequestedSegments::Table, RequestedSegments::Column1)).eq(1))
                .add(
                    Expr::col((folder::Entity, folder::Column::Name))
                        .equals((RequestedSegments::Table, RequestedSegments::Column2)),
                ),
        )
        .and_where(Expr::col((folder::Entity, folder::Column::OwnerUserId)).eq(user_id))
        .and_where(Expr::col((folder::Entity, folder::Column::TeamId)).is_null())
        .and_where(Expr::col((folder::Entity, folder::Column::DeletedAt)).is_null());

    base_select = match root_parent_id {
        Some(root_parent_id) => base_select
            .and_where(Expr::col((folder::Entity, folder::Column::ParentId)).eq(root_parent_id))
            .to_owned(),
        None => base_select
            .and_where(Expr::col((folder::Entity, folder::Column::ParentId)).is_null())
            .to_owned(),
    };

    let recursive_select = Query::select()
        .column((RequestedSegments::Table, RequestedSegments::Column1))
        .column((folder::Entity, folder::Column::Id))
        .column((folder::Entity, folder::Column::Name))
        .column((folder::Entity, folder::Column::ParentId))
        .column((folder::Entity, folder::Column::TeamId))
        .column((folder::Entity, folder::Column::OwnerUserId))
        .column((folder::Entity, folder::Column::CreatedByUserId))
        .column((folder::Entity, folder::Column::CreatedByUsername))
        .column((folder::Entity, folder::Column::PolicyId))
        .column((folder::Entity, folder::Column::CreatedAt))
        .column((folder::Entity, folder::Column::UpdatedAt))
        .column((folder::Entity, folder::Column::DeletedAt))
        .column((folder::Entity, folder::Column::IsLocked))
        .from(folder::Entity)
        .join(
            sea_orm::JoinType::InnerJoin,
            FolderChain::Table,
            Expr::col((folder::Entity, folder::Column::ParentId))
                .equals((FolderChain::Table, FolderChain::Id)),
        )
        .join_subquery(
            sea_orm::JoinType::InnerJoin,
            recursive_requested,
            RequestedSegments::Table,
            Condition::all()
                .add(
                    Expr::col((RequestedSegments::Table, RequestedSegments::Column1))
                        .eq(Expr::col((FolderChain::Table, FolderChain::SegmentIndex)).add(1)),
                )
                .add(
                    Expr::col((folder::Entity, folder::Column::Name))
                        .equals((RequestedSegments::Table, RequestedSegments::Column2)),
                ),
        )
        .and_where(Expr::col((folder::Entity, folder::Column::OwnerUserId)).eq(user_id))
        .and_where(Expr::col((folder::Entity, folder::Column::TeamId)).is_null())
        .and_where(Expr::col((folder::Entity, folder::Column::DeletedAt)).is_null())
        .to_owned();

    let folder_chain_cte = CommonTableExpression::new()
        .table_name(FolderChain::Table)
        .columns([
            FolderChain::SegmentIndex,
            FolderChain::Id,
            FolderChain::Name,
            FolderChain::ParentId,
            FolderChain::TeamId,
            FolderChain::OwnerUserId,
            FolderChain::CreatedByUserId,
            FolderChain::CreatedByUsername,
            FolderChain::PolicyId,
            FolderChain::CreatedAt,
            FolderChain::UpdatedAt,
            FolderChain::DeletedAt,
            FolderChain::IsLocked,
        ])
        .query(
            base_select
                .union(UnionType::All, recursive_select)
                .to_owned(),
        )
        .to_owned();

    let final_select = Query::select()
        .column((FolderChain::Table, FolderChain::SegmentIndex))
        .column((FolderChain::Table, FolderChain::Id))
        .column((FolderChain::Table, FolderChain::Name))
        .column((FolderChain::Table, FolderChain::ParentId))
        .column((FolderChain::Table, FolderChain::TeamId))
        .column((FolderChain::Table, FolderChain::OwnerUserId))
        .column((FolderChain::Table, FolderChain::CreatedByUserId))
        .column((FolderChain::Table, FolderChain::CreatedByUsername))
        .column((FolderChain::Table, FolderChain::PolicyId))
        .column((FolderChain::Table, FolderChain::CreatedAt))
        .column((FolderChain::Table, FolderChain::UpdatedAt))
        .column((FolderChain::Table, FolderChain::DeletedAt))
        .column((FolderChain::Table, FolderChain::IsLocked))
        .from(FolderChain::Table)
        .order_by((FolderChain::Table, FolderChain::SegmentIndex), Order::Asc)
        .to_owned();

    let with_clause = WithClause::new()
        .recursive(true)
        .cte(folder_chain_cte)
        .to_owned();

    Ok(with_clause.query(final_select))
}

async fn resolve_path_chain_iteratively<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    root_parent_id: Option<i64>,
    segments: &[String],
) -> Result<Vec<folder::Model>> {
    let mut resolved = Vec::with_capacity(segments.len());
    let mut current_parent = root_parent_id;

    for segment in segments {
        let Some(folder) = find_by_name_in_parent(db, user_id, current_parent, segment).await?
        else {
            break;
        };
        current_parent = Some(folder.id);
        resolved.push(folder);
    }

    Ok(resolved)
}

/// 批量解析路径前缀中的文件夹链，避免逐段 round-trip。
///
/// `resolve_path_chain` only resolves personal user-space folders.
/// `build_resolve_path_chain_query` applies `TeamId IS NULL`, so team-root
/// paths will not match here. Callers resolving team folders must use a
/// different team-aware code path instead of this helper.
///
/// 返回已成功匹配的文件夹链；如果中途断开，只返回前缀中已匹配的部分。
pub async fn resolve_path_chain<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    root_parent_id: Option<i64>,
    segments: &[String],
) -> Result<Vec<folder::Model>> {
    if segments.is_empty() {
        return Ok(vec![]);
    }

    if matches!(db.get_database_backend(), sea_orm::DbBackend::MySql) {
        // SeaQuery builds this walk with a recursive CTE over an inline VALUES list.
        // PostgreSQL / SQLite accept that shape, but MySQL does not reliably support it.
        // WebDAV only needs path resolution here, so fall back to indexed per-segment lookups.
        return resolve_path_chain_iteratively(db, user_id, root_parent_id, segments).await;
    }

    // The recursive walk keeps hitting idx_folders_owner_deleted_parent_name instead of issuing
    // one query per path segment.
    let rows = Folder::find()
        .from_raw_sql(
            db.get_database_backend()
                .build(&build_resolve_path_chain_query(
                    user_id,
                    root_parent_id,
                    segments,
                )?),
        )
        .into_model::<ResolvedPathFolderRow>()
        .all(db)
        .await
        .map_err(AsterError::from)?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn find_ancestor_models<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    folder_id: i64,
) -> Result<Vec<folder::Model>> {
    find_ancestor_models_in_scope(db, AncestorScope::Owner { user_id }, folder_id).await
}

pub(crate) async fn find_team_ancestor_models<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    folder_id: i64,
) -> Result<Vec<folder::Model>> {
    find_ancestor_models_in_scope(db, AncestorScope::Team { team_id }, folder_id).await
}

/// 查找文件夹的祖先链（从根下第一层到当前文件夹），校验归属与未删除
pub async fn find_ancestors<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    folder_id: i64,
) -> Result<Vec<(i64, String)>> {
    let folder = find_by_id(db, folder_id).await?;
    crate::utils::verify_optional_owner(folder.owner_user_id, user_id, "folder")?;
    if folder.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "folder #{folder_id} is in trash"
        )));
    }

    let ancestors = find_ancestor_models(db, user_id, folder_id).await?;
    Ok(ancestors
        .into_iter()
        .map(|folder| (folder.id, folder.name))
        .collect())
}
