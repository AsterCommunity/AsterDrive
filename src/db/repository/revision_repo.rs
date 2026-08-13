//! Repository for the canonical immutable file revision ledger.

use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, ExprTrait, ModelTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait, Set, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::{
    entity_property, file,
    file_revision::{self, Entity as FileRevision},
    file_revision_history::{self, Entity as FileRevisionHistory},
    file_revision_property,
};
use aster_drive_model::types::EntityType;

#[derive(Clone, Copy, Debug)]
pub enum RevisionReason {
    Create,
    Overwrite,
    Restore,
    Copy,
}

impl RevisionReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Overwrite => "overwrite",
            Self::Restore => "restore",
            Self::Copy => "copy",
        }
    }
}

pub struct NewRevision<'a> {
    pub blob_id: i64,
    pub logical_size: i64,
    pub mime_type: &'a str,
    pub content_sha256: Option<&'a str>,
    pub creator_user_id: Option<i64>,
    pub creator_display_name: &'a str,
    pub comment: Option<&'a str>,
    pub reason: RevisionReason,
    pub created_at: chrono::DateTime<Utc>,
}

fn new_etag() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

async fn history_public_id_exists<C: ConnectionTrait>(db: &C, public_id: &str) -> Result<bool> {
    FileRevisionHistory::find()
        .select_only()
        .column(file_revision_history::Column::Id)
        .filter(file_revision_history::Column::PublicId.eq(public_id))
        .into_tuple::<i64>()
        .one(db)
        .await
        .map(|row| row.is_some())
        .map_err(AsterError::from)
}

async fn revision_public_id_exists<C: ConnectionTrait>(db: &C, public_id: &str) -> Result<bool> {
    FileRevision::find()
        .select_only()
        .column(file_revision::Column::Id)
        .filter(file_revision::Column::PublicId.eq(public_id))
        .into_tuple::<i64>()
        .one(db)
        .await
        .map(|row| row.is_some())
        .map_err(AsterError::from)
}

async fn insert_history<C: ConnectionTrait>(
    db: &C,
    file: &file::Model,
) -> Result<file_revision_history::Model> {
    let public_id = aster_forge_utils::id::new_best_effort_uuid(
        "file revision history public id",
        |candidate| {
            let public_id = candidate.hyphenated().to_string();
            async move { history_public_id_exists(db, &public_id).await }
        },
    )
    .await?
    .hyphenated()
    .to_string();
    file_revision_history::ActiveModel {
        public_id: Set(public_id),
        file_id: Set(Some(file.id)),
        current_revision_id: Set(None),
        next_sequence: Set(2),
        created_at: Set(file.created_at),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(AsterError::from)
}

pub async fn create_initial<C: ConnectionTrait>(
    db: &C,
    file: &file::Model,
    reason: RevisionReason,
) -> Result<file_revision::Model> {
    let history = insert_history(db, file).await?;

    let revision = insert_revision(
        db,
        history.id,
        1,
        None,
        NewRevision {
            blob_id: file.blob_id,
            logical_size: file.size,
            mime_type: &file.mime_type,
            content_sha256: None,
            creator_user_id: file.created_by_user_id,
            creator_display_name: &file.created_by_username,
            comment: None,
            reason,
            created_at: file.created_at,
        },
    )
    .await?;
    snapshot_user_properties(db, file.id, revision.id).await?;
    set_current_revision(db, history.id, revision.id).await?;
    Ok(revision)
}

pub async fn append<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    expected_current_revision_id: Option<i64>,
    input: NewRevision<'_>,
) -> Result<file_revision::Model> {
    let history = lock_history_by_file_id(db, file_id).await?;
    if let Some(expected) = expected_current_revision_id
        && history.current_revision_id != Some(expected)
    {
        return Err(crate::errors::precondition_failed_with_code(
            crate::api::api_error_code::ApiErrorCode::FileModifiedDuringWrite,
            "file revision head changed while content was being committed",
        ));
    }
    let predecessor = history.current_revision_id;
    let revision =
        insert_revision(db, history.id, history.next_sequence, predecessor, input).await?;
    snapshot_user_properties(db, file_id, revision.id).await?;

    let mut active: file_revision_history::ActiveModel = history.into();
    active.current_revision_id = Set(Some(revision.id));
    active.next_sequence = Set(revision
        .sequence
        .checked_add(1)
        .ok_or_else(|| AsterError::internal_error("file revision sequence exhausted"))?);
    active.update(db).await.map_err(AsterError::from)?;
    Ok(revision)
}

async fn insert_revision<C: ConnectionTrait>(
    db: &C,
    history_id: i64,
    sequence: i64,
    predecessor_revision_id: Option<i64>,
    input: NewRevision<'_>,
) -> Result<file_revision::Model> {
    let public_id =
        aster_forge_utils::id::new_best_effort_uuid("file revision public id", |candidate| {
            let public_id = candidate.hyphenated().to_string();
            async move { revision_public_id_exists(db, &public_id).await }
        })
        .await?
        .hyphenated()
        .to_string();
    file_revision::ActiveModel {
        public_id: Set(public_id),
        history_id: Set(history_id),
        sequence: Set(sequence),
        predecessor_revision_id: Set(predecessor_revision_id),
        blob_id: Set(Some(input.blob_id)),
        logical_size: Set(input.logical_size),
        mime_type: Set(Some(input.mime_type.to_string())),
        etag: Set(new_etag()),
        content_sha256: Set(input.content_sha256.map(ToOwned::to_owned)),
        creator_user_id: Set(input.creator_user_id),
        creator_display_name: Set(Some(input.creator_display_name.to_string())),
        comment: Set(input.comment.map(ToOwned::to_owned)),
        reason: Set(input.reason.as_str().to_string()),
        created_at: Set(input.created_at),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(AsterError::from)
}

async fn set_current_revision<C: ConnectionTrait>(
    db: &C,
    history_id: i64,
    revision_id: i64,
) -> Result<()> {
    FileRevisionHistory::update_many()
        .col_expr(
            file_revision_history::Column::CurrentRevisionId,
            Expr::value(Some(revision_id)),
        )
        .filter(file_revision_history::Column::Id.eq(history_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

async fn snapshot_user_properties<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    revision_id: i64,
) -> Result<()> {
    let properties = entity_property::Entity::find()
        .filter(entity_property::Column::EntityType.eq(EntityType::File))
        .filter(entity_property::Column::EntityId.eq(file_id))
        .filter(entity_property::Column::Namespace.ne("DAV:"))
        .filter(entity_property::Column::Namespace.not_like("system.%"))
        .all(db)
        .await
        .map_err(AsterError::from)?;
    if properties.is_empty() {
        return Ok(());
    }
    FileRevision::find_by_id(revision_id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::internal_error("new revision disappeared before snapshot"))?;
    file_revision_property::Entity::insert_many(properties.into_iter().map(|property| {
        file_revision_property::ActiveModel {
            revision_id: Set(revision_id),
            namespace: Set(property.namespace),
            name: Set(property.name),
            xml_value: Set(property.value),
        }
    }))
    .exec(db)
    .await
    .map_err(AsterError::from)?;
    Ok(())
}

pub async fn find_history_by_file_id<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
) -> Result<file_revision_history::Model> {
    FileRevisionHistory::find()
        .filter(file_revision_history::Column::FileId.eq(file_id))
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| {
            AsterError::internal_error(format!("file #{file_id} has no revision history"))
        })
}

pub async fn find_current_by_file_id<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
) -> Result<file_revision::Model> {
    let history = find_history_by_file_id(db, file_id).await?;
    let current_revision_id = history.current_revision_id.ok_or_else(|| {
        AsterError::internal_error(format!(
            "file #{file_id} revision history has no current revision"
        ))
    })?;
    let revision = FileRevision::find_by_id(current_revision_id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| {
            AsterError::internal_error(format!(
                "file #{file_id} revision history has a dangling current pointer"
            ))
        })?;
    if revision.history_id != history.id || revision.retired_at.is_some() {
        return Err(AsterError::internal_error(format!(
            "file #{file_id} revision history points to an invalid current revision"
        )));
    }
    Ok(revision)
}

pub async fn current_etag<C: ConnectionTrait>(db: &C, file_id: i64) -> Result<String> {
    find_current_by_file_id(db, file_id)
        .await
        .map(|revision| revision.etag)
}

pub async fn current_etags_by_file_ids<C: ConnectionTrait>(
    db: &C,
    file_ids: &[i64],
) -> Result<HashMap<i64, String>> {
    if file_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = FileRevisionHistory::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            file_revision_history::Relation::Revisions.def(),
        )
        .select_only()
        .column(file_revision_history::Column::FileId)
        .column(file_revision::Column::Etag)
        .filter(file_revision_history::Column::FileId.is_in(file_ids.iter().copied()))
        .filter(
            Expr::col((file_revision::Entity, file_revision::Column::Id)).equals((
                file_revision_history::Entity,
                file_revision_history::Column::CurrentRevisionId,
            )),
        )
        .into_tuple::<(i64, String)>()
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok(rows.into_iter().collect())
}

pub async fn lock_history_by_file_id<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
) -> Result<file_revision_history::Model> {
    let query =
        FileRevisionHistory::find().filter(file_revision_history::Column::FileId.eq(file_id));
    let history = match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => query.lock_exclusive().one(db).await,
        _ => query.one(db).await,
    }
    .map_err(AsterError::from)?;
    history.ok_or_else(|| {
        AsterError::internal_error(format!("file #{file_id} has no revision history"))
    })
}

pub async fn find_by_id_for_file<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    revision_id: i64,
) -> Result<Option<file_revision::Model>> {
    let history = find_history_by_file_id(db, file_id).await?;
    FileRevision::find_by_id(revision_id)
        .filter(file_revision::Column::HistoryId.eq(history.id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_file_id<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
) -> Result<Vec<file_revision::Model>> {
    let history = find_history_by_file_id(db, file_id).await?;
    FileRevision::find()
        .filter(file_revision::Column::HistoryId.eq(history.id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .order_by_desc(file_revision::Column::Sequence)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_page_by_file_id<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    limit: u64,
    after_sequence: Option<i64>,
) -> Result<Vec<file_revision::Model>> {
    let history = find_history_by_file_id(db, file_id).await?;
    let mut query = FileRevision::find()
        .filter(file_revision::Column::HistoryId.eq(history.id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .order_by_desc(file_revision::Column::Sequence);
    if let Some(sequence) = after_sequence {
        query = query.filter(file_revision::Column::Sequence.lt(sequence));
    }
    query.limit(limit).all(db).await.map_err(AsterError::from)
}

pub async fn find_properties<C: ConnectionTrait>(
    db: &C,
    revision_id: i64,
) -> Result<Vec<file_revision_property::Model>> {
    file_revision_property::Entity::find()
        .filter(file_revision_property::Column::RevisionId.eq(revision_id))
        .order_by_asc(file_revision_property::Column::Namespace)
        .order_by_asc(file_revision_property::Column::Name)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn restore_user_properties<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    revision_id: i64,
) -> Result<()> {
    let live = entity_property::Entity::find()
        .filter(entity_property::Column::EntityType.eq(EntityType::File))
        .filter(entity_property::Column::EntityId.eq(file_id))
        .all(db)
        .await
        .map_err(AsterError::from)?;
    for property in live {
        if property.namespace != "DAV:" && !property.namespace.starts_with("system.") {
            property.delete(db).await.map_err(AsterError::from)?;
        }
    }
    for property in find_properties(db, revision_id).await? {
        entity_property::ActiveModel {
            entity_type: Set(EntityType::File),
            entity_id: Set(file_id),
            namespace: Set(property.namespace),
            name: Set(property.name),
            value: Set(property.xml_value),
            ..Default::default()
        }
        .insert(db)
        .await
        .map_err(AsterError::from)?;
    }
    Ok(())
}

pub async fn find_by_blob_id<C: ConnectionTrait>(
    db: &C,
    blob_id: i64,
) -> Result<Vec<(file_revision::Model, file_revision_history::Model)>> {
    FileRevision::find()
        .find_also_related(FileRevisionHistory)
        .filter(file_revision::Column::BlobId.eq(blob_id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .order_by_asc(file_revision::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
        .map(|rows| {
            rows.into_iter()
                .filter_map(|(revision, history)| history.map(|h| (revision, h)))
                .collect()
        })
}

pub async fn count_by_file_id<C: ConnectionTrait>(db: &C, file_id: i64) -> Result<u64> {
    let history = find_history_by_file_id(db, file_id).await?;
    FileRevision::find()
        .filter(file_revision::Column::HistoryId.eq(history.id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_oldest_non_current<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
) -> Result<Option<file_revision::Model>> {
    let history = find_history_by_file_id(db, file_id).await?;
    FileRevision::find()
        .filter(file_revision::Column::HistoryId.eq(history.id))
        .filter(file_revision::Column::Id.ne(history.current_revision_id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .order_by_asc(file_revision::Column::Sequence)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn tombstone<C: ConnectionTrait>(db: &C, revision: file_revision::Model) -> Result<()> {
    let successor = FileRevision::find()
        .filter(file_revision::Column::HistoryId.eq(revision.history_id))
        .filter(file_revision::Column::PredecessorRevisionId.eq(revision.id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .one(db)
        .await
        .map_err(AsterError::from)?;
    if let Some(successor) = successor {
        let mut active: file_revision::ActiveModel = successor.into();
        active.predecessor_revision_id = Set(revision.predecessor_revision_id);
        active.update(db).await.map_err(AsterError::from)?;
    }
    let mut active: file_revision::ActiveModel = revision.into();
    active.blob_id = Set(None);
    let now = Utc::now();
    active.retired_at = Set(Some(now));
    active.purged_at = Set(Some(now));
    active.update(db).await.map_err(AsterError::from)?;
    Ok(())
}

pub async fn retire_histories<C: ConnectionTrait>(
    db: &C,
    file_ids: &[i64],
) -> Result<Vec<(i64, i64)>> {
    if file_ids.is_empty() {
        return Ok(Vec::new());
    }
    let histories = FileRevisionHistory::find()
        .filter(file_revision_history::Column::FileId.is_in(file_ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)?;
    let history_ids: Vec<i64> = histories.iter().map(|history| history.id).collect();
    let revisions = FileRevision::find()
        .filter(file_revision::Column::HistoryId.is_in(history_ids.iter().copied()))
        .filter(file_revision::Column::BlobId.is_not_null())
        .filter(file_revision::Column::RetiredAt.is_null())
        .all(db)
        .await
        .map_err(AsterError::from)?;
    let refs = revisions
        .iter()
        .filter_map(|revision| {
            revision
                .blob_id
                .map(|blob_id| (blob_id, revision.logical_size))
        })
        .collect();
    let now = Utc::now();
    FileRevision::update_many()
        .col_expr(
            file_revision::Column::BlobId,
            Expr::value(Option::<i64>::None),
        )
        .col_expr(file_revision::Column::RetiredAt, Expr::value(Some(now)))
        .col_expr(file_revision::Column::PurgedAt, Expr::value(Some(now)))
        .filter(file_revision::Column::HistoryId.is_in(history_ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    FileRevisionHistory::update_many()
        .col_expr(
            file_revision_history::Column::FileId,
            Expr::value(Option::<i64>::None),
        )
        .col_expr(
            file_revision_history::Column::CurrentRevisionId,
            Expr::value(Option::<i64>::None),
        )
        .col_expr(
            file_revision_history::Column::RetiredAt,
            Expr::value(Some(now)),
        )
        .filter(file_revision_history::Column::Id.is_in(history_ids))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(refs)
}

fn sum_size_expr(backend: DbBackend) -> sea_orm::sea_query::SimpleExpr {
    let type_name = match backend {
        DbBackend::Postgres => "bigint",
        DbBackend::MySql => "signed",
        _ => "integer",
    };
    Expr::col(file_revision::Column::LogicalSize)
        .sum()
        .cast_as(type_name)
}

pub async fn sum_non_current_sizes_by_file_id(
    db: &sea_orm::DatabaseConnection,
    file_id: i64,
) -> Result<i64> {
    let history = find_history_by_file_id(db, file_id).await?;
    Ok(FileRevision::find()
        .select_only()
        .column_as(sum_size_expr(db.get_database_backend()), "sum")
        .filter(file_revision::Column::HistoryId.eq(history.id))
        .filter(file_revision::Column::Id.ne(history.current_revision_id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .into_tuple::<Option<i64>>()
        .one(db)
        .await
        .map_err(AsterError::from)?
        .flatten()
        .unwrap_or(0))
}

pub async fn count_non_current_blob_refs_for_blobs<C: ConnectionTrait>(
    db: &C,
    blob_ids: &[i64],
) -> Result<HashMap<i64, i64>> {
    if blob_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = FileRevision::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            file_revision::Relation::History.def(),
        )
        .select_only()
        .column(file_revision::Column::BlobId)
        .column_as(
            Expr::col((file_revision::Entity, file_revision::Column::Id)).count(),
            "ref_count",
        )
        .filter(file_revision::Column::BlobId.is_in(blob_ids.iter().copied()))
        .filter(file_revision::Column::RetiredAt.is_null())
        .filter(
            Expr::col((file_revision::Entity, file_revision::Column::Id)).ne(Expr::col((
                file_revision_history::Entity,
                file_revision_history::Column::CurrentRevisionId,
            ))),
        )
        .group_by(file_revision::Column::BlobId)
        .into_tuple::<(i64, i64)>()
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok(rows.into_iter().collect())
}

pub async fn count_non_current_blob_refs_for_blob<C: ConnectionTrait>(
    db: &C,
    blob_id: i64,
) -> Result<i64> {
    Ok(count_non_current_blob_refs_for_blobs(db, &[blob_id])
        .await?
        .get(&blob_id)
        .copied()
        .unwrap_or(0))
}

pub async fn replace_blob_refs<C: ConnectionTrait>(
    db: &C,
    old_blob_id: i64,
    new_blob_id: i64,
) -> Result<u64> {
    let result = FileRevision::update_many()
        .col_expr(
            file_revision::Column::BlobId,
            Expr::value(Some(new_blob_id)),
        )
        .filter(file_revision::Column::BlobId.eq(old_blob_id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}
