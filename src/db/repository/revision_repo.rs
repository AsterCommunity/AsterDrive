//! Repository for the canonical immutable file revision ledger.

use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, ExprTrait,
    IsolationLevel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait, Set,
    TransactionSession, TransactionTrait,
    sea_query::{CaseStatement, Expr},
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::{
    entity_property, file,
    file_revision::{self, Entity as FileRevision},
    file_revision_history::{self, Entity as FileRevisionHistory},
    file_revision_property,
};
use aster_drive_model::types::EntityType;

/// Error returned when appending a revision to a file history fails a precondition.
#[derive(Debug)]
pub enum RevisionAppendError {
    /// The expected head revision id no longer matches the current head.
    HeadChanged,
    /// The expected ETag does not match the ETag of the current head revision.
    EtagMismatch,
    /// A database or repository error occurred while appending.
    Repository(AsterError),
}

impl From<AsterError> for RevisionAppendError {
    fn from(error: AsterError) -> Self {
        Self::Repository(error)
    }
}

/// Result type for revision append operations that can fail on optimistic concurrency checks.
pub type RevisionAppendResult<T> = std::result::Result<T, RevisionAppendError>;

/// Why a revision was appended, persisted in the revision `reason` column.
#[derive(Clone, Copy, Debug)]
pub enum RevisionReason {
    /// The revision records the initial content of a newly created file.
    Create,
    /// The revision replaced the content of an existing file.
    Overwrite,
    /// The revision restored content from an earlier revision.
    Restore,
    /// The revision was produced while copying a file.
    Copy,
    /// The revision only reflects a property change on the file.
    PropertyChange,
}

impl RevisionReason {
    /// Returns the stable string token stored in the revision `reason` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Overwrite => "overwrite",
            Self::Restore => "restore",
            Self::Copy => "copy",
            Self::PropertyChange => "property_change",
        }
    }
}

/// A DeltaV revision resolved together with the history and file that own it.
#[derive(Debug, Clone)]
pub struct DeltavRevisionTarget {
    /// The file the revision history belongs to.
    pub file: file::Model,
    /// The revision history containing the revision.
    pub history: file_revision_history::Model,
    /// The live revision at or after the DeltaV root that still carries blob content.
    pub revision: file_revision::Model,
}

/// A file's current head revision together with its DeltaV state.
#[derive(Debug, Clone)]
pub struct CurrentRevisionSnapshot {
    /// The current head revision of the file.
    pub revision: file_revision::Model,
    /// Whether the history has been activated for DeltaV versioning.
    pub deltav_controlled: bool,
}

/// Content and metadata for a revision about to be inserted into a file history.
pub struct NewRevision<'a> {
    /// Blob holding the revision content; must reference an existing blob row.
    pub blob_id: i64,
    /// Content size in bytes as presented to clients, which may differ from the stored blob size.
    pub logical_size: i64,
    /// MIME type of the revision content.
    pub mime_type: &'a str,
    /// SHA-256 digest of the content, when the caller has computed it.
    pub content_sha256: Option<&'a str>,
    /// Id of the authenticated user that produced the content, if known.
    pub creator_user_id: Option<i64>,
    /// Display name recorded for the creator.
    pub creator_display_name: &'a str,
    /// Optional client-supplied comment describing the change.
    pub comment: Option<&'a str>,
    /// Why this revision is being created.
    pub reason: RevisionReason,
    /// Revision timestamp; typically the time the file content changed.
    pub created_at: chrono::DateTime<Utc>,
    /// Caller-supplied ETag; a fresh one is generated when `None`.
    pub etag: Option<&'a str>,
}

fn new_etag() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

async fn find_user_properties<C: ConnectionTrait>(
    db: &C,
    file_ids: &[i64],
) -> Result<Vec<entity_property::Model>> {
    let targets: Vec<_> = file_ids
        .iter()
        .map(|file_id| (EntityType::File, *file_id))
        .collect();
    crate::db::repository::property_repo::find_by_entities(db, &targets)
        .await
        .map(|properties| {
            properties
                .into_iter()
                .filter(|property| {
                    !crate::db::repository::property_repo::is_protected_namespace(
                        &property.namespace,
                    )
                })
                .collect()
        })
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

/// Creates the first history row, the sequence-1 revision, and its property snapshot for
/// a newly created file, then publishes the history head pointer.
///
/// Callers must run this on the same connection, typically a transaction, that created
/// the file.
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
            etag: None,
        },
    )
    .await?;
    snapshot_user_properties(db, file.id, revision.id).await?;
    set_current_revision(db, history.id, revision.id).await?;
    Ok(revision)
}

/// Creates initial histories, revisions, property snapshots, and head pointers in bounded batches.
///
/// Callers must include this operation in the transaction that created the files and their user
/// properties. Heads are published last so no visible history points at a partial snapshot.
pub async fn create_initial_many<C: ConnectionTrait>(
    db: &C,
    files: &[file::Model],
    reason: RevisionReason,
) -> Result<Vec<file_revision::Model>> {
    // Keep multi-row INSERTs and CASE updates below cross-database bind-parameter limits.
    const BATCH_SIZE: usize = 50;

    if files.is_empty() {
        return Ok(Vec::new());
    }

    let file_by_id: HashMap<i64, &file::Model> = files.iter().map(|file| (file.id, file)).collect();
    if file_by_id.len() != files.len() {
        return Err(AsterError::internal_error(
            "initial revision batch contains duplicate file ids",
        ));
    }

    // Histories provide the foreign-key parents required by every later phase.
    for chunk in files.chunks(BATCH_SIZE) {
        FileRevisionHistory::insert_many(chunk.iter().map(|file| {
            file_revision_history::ActiveModel {
                public_id: Set(uuid::Uuid::new_v4().hyphenated().to_string()),
                file_id: Set(Some(file.id)),
                current_revision_id: Set(None),
                next_sequence: Set(2),
                created_at: Set(file.created_at),
                ..Default::default()
            }
        }))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    }

    let file_ids: Vec<i64> = files.iter().map(|file| file.id).collect();
    let mut histories = Vec::with_capacity(files.len());
    for chunk in file_ids.chunks(BATCH_SIZE) {
        histories.extend(
            FileRevisionHistory::find()
                .filter(file_revision_history::Column::FileId.is_in(chunk.iter().copied()))
                .all(db)
                .await
                .map_err(AsterError::from)?,
        );
    }
    if histories.len() != files.len() {
        return Err(AsterError::internal_error(
            "initial revision batch could not reload every history",
        ));
    }

    for chunk in histories.chunks(BATCH_SIZE) {
        FileRevision::insert_many(
            chunk
                .iter()
                .map(|history| {
                    let file_id = history.file_id.ok_or_else(|| {
                        AsterError::internal_error("new initial revision history lost its file id")
                    })?;
                    let file = file_by_id.get(&file_id).ok_or_else(|| {
                        AsterError::internal_error(format!(
                            "new initial revision history references unexpected file #{file_id}"
                        ))
                    })?;
                    Ok(file_revision::ActiveModel {
                        public_id: Set(uuid::Uuid::new_v4().hyphenated().to_string()),
                        history_id: Set(history.id),
                        sequence: Set(1),
                        predecessor_revision_id: Set(None),
                        blob_id: Set(Some(file.blob_id)),
                        logical_size: Set(file.size),
                        mime_type: Set(Some(file.mime_type.clone())),
                        etag: Set(new_etag()),
                        content_sha256: Set(None),
                        creator_user_id: Set(file.created_by_user_id),
                        creator_display_name: Set(Some(file.created_by_username.clone())),
                        comment: Set(None),
                        reason: Set(reason.as_str().to_string()),
                        created_at: Set(file.created_at),
                        ..Default::default()
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        )
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    }

    let history_ids: Vec<i64> = histories.iter().map(|history| history.id).collect();
    let mut revisions = Vec::with_capacity(files.len());
    for chunk in history_ids.chunks(BATCH_SIZE) {
        revisions.extend(
            FileRevision::find()
                .filter(file_revision::Column::HistoryId.is_in(chunk.iter().copied()))
                .filter(file_revision::Column::Sequence.eq(1))
                .all(db)
                .await
                .map_err(AsterError::from)?,
        );
    }
    if revisions.len() != files.len() {
        return Err(AsterError::internal_error(
            "initial revision batch could not reload every revision",
        ));
    }

    let revision_by_history_id: HashMap<i64, i64> = revisions
        .iter()
        .map(|revision| (revision.history_id, revision.id))
        .collect();
    let revision_by_file_id: HashMap<i64, i64> = histories
        .iter()
        .map(|history| {
            let file_id = history.file_id.ok_or_else(|| {
                AsterError::internal_error("new initial revision history lost its file id")
            })?;
            let revision_id = revision_by_history_id
                .get(&history.id)
                .copied()
                .ok_or_else(|| {
                    AsterError::internal_error(format!(
                        "new history #{} has no initial revision",
                        history.id
                    ))
                })?;
            Ok((file_id, revision_id))
        })
        .collect::<Result<_>>()?;

    // Snapshot properties only after callers have copied them onto the new file projections.
    let properties = find_user_properties(db, &file_ids).await?;
    let snapshot_models: Vec<_> = properties
        .into_iter()
        .map(|property| {
            let revision_id = revision_by_file_id
                .get(&property.entity_id)
                .copied()
                .ok_or_else(|| {
                    AsterError::internal_error(format!(
                        "initial revision property references unexpected file #{}",
                        property.entity_id
                    ))
                })?;
            Ok(file_revision_property::ActiveModel {
                revision_id: Set(revision_id),
                namespace: Set(property.namespace),
                name: Set(property.name),
                xml_value: Set(property.value),
            })
        })
        .collect::<Result<_>>()?;
    for chunk in snapshot_models.chunks(BATCH_SIZE) {
        file_revision_property::Entity::insert_many(chunk.iter().cloned())
            .exec(db)
            .await
            .map_err(AsterError::from)?;
    }

    // Publishing heads is the final phase; all referenced revisions and snapshots now exist.
    for chunk in histories.chunks(BATCH_SIZE) {
        let mut current_revision_case = CaseStatement::new();
        for history in chunk {
            let revision_id = revision_by_history_id
                .get(&history.id)
                .copied()
                .ok_or_else(|| {
                    AsterError::internal_error(format!(
                        "new history #{} has no initial revision",
                        history.id
                    ))
                })?;
            current_revision_case = current_revision_case.case(
                Expr::col(file_revision_history::Column::Id).eq(history.id),
                revision_id,
            );
        }
        let ids: Vec<i64> = chunk.iter().map(|history| history.id).collect();
        let result = FileRevisionHistory::update_many()
            .col_expr(
                file_revision_history::Column::CurrentRevisionId,
                current_revision_case
                    .finally(Expr::col(file_revision_history::Column::CurrentRevisionId))
                    .into(),
            )
            .filter(file_revision_history::Column::Id.is_in(ids))
            .exec(db)
            .await
            .map_err(AsterError::from)?;
        if result.rows_affected != chunk.len() as u64 {
            return Err(AsterError::internal_error(
                "initial revision batch did not update every history head",
            ));
        }
    }

    Ok(revisions)
}

/// Appends a new revision to a file's history under an exclusive history lock.
///
/// When `expected_current_revision_id` is `Some`, it must equal the current head
/// revision id or [`RevisionAppendError::HeadChanged`] is returned.
pub async fn append<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    expected_current_revision_id: Option<i64>,
    input: NewRevision<'_>,
) -> RevisionAppendResult<file_revision::Model> {
    let history = lock_history_by_file_id(db, file_id).await?;
    if let Some(expected) = expected_current_revision_id
        && history.current_revision_id != Some(expected)
    {
        return Err(RevisionAppendError::HeadChanged);
    }
    append_locked(db, file_id, history, input)
        .await
        .map_err(Into::into)
}

/// Appends a new revision guarded by the current head revision's ETag.
///
/// When `expected_etag` is `Some`, it is compared case-insensitively with the head
/// revision's ETag and [`RevisionAppendError::EtagMismatch`] is returned on mismatch.
pub async fn append_for_expected_etag<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    expected_etag: Option<&str>,
    input: NewRevision<'_>,
) -> RevisionAppendResult<file_revision::Model> {
    let history = lock_history_by_file_id(db, file_id).await?;
    if let Some(expected_etag) = expected_etag {
        let current_id = history.current_revision_id.ok_or_else(|| {
            AsterError::internal_error(format!(
                "file #{file_id} revision history has no current revision"
            ))
        })?;
        let current_etag = FileRevision::find_by_id(current_id)
            .select_only()
            .column(file_revision::Column::Etag)
            .into_tuple::<String>()
            .one(db)
            .await
            .map_err(AsterError::from)?
            .ok_or_else(|| {
                AsterError::internal_error(format!(
                    "file #{file_id} revision history has a dangling current pointer"
                ))
            })?;
        if !expected_etag.eq_ignore_ascii_case(&current_etag) {
            return Err(RevisionAppendError::EtagMismatch);
        }
    }
    append_locked(db, file_id, history, input)
        .await
        .map_err(Into::into)
}

async fn append_locked<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    history: file_revision_history::Model,
    input: NewRevision<'_>,
) -> Result<file_revision::Model> {
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
        etag: Set(input.etag.map_or_else(new_etag, ToOwned::to_owned)),
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
    let properties = find_user_properties(db, &[file_id]).await?;
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

/// Returns the revision history row of a file.
///
/// Fails with an internal error when the file has no history row.
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

/// Atomically marks the current canonical head as the RFC 3253 activation root.
/// Repeated activation preserves the original root and timestamp.
pub async fn activate_deltav<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
) -> Result<file_revision_history::Model> {
    FileRevisionHistory::update_many()
        .col_expr(
            file_revision_history::Column::DeltavControlledAt,
            Expr::value(Some(Utc::now())),
        )
        .col_expr(
            file_revision_history::Column::DeltavRootRevisionId,
            Expr::col(file_revision_history::Column::CurrentRevisionId),
        )
        .filter(file_revision_history::Column::FileId.eq(file_id))
        .filter(file_revision_history::Column::DeltavControlledAt.is_null())
        .filter(file_revision_history::Column::CurrentRevisionId.is_not_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    let history = find_history_by_file_id(db, file_id).await?;
    if history.deltav_controlled_at.is_none() || history.deltav_root_revision_id.is_none() {
        return Err(AsterError::internal_error(format!(
            "file #{file_id} revision history has no activatable current revision"
        )));
    }
    Ok(history)
}

async fn deltav_root_sequence<C: ConnectionTrait>(
    db: &C,
    history: &file_revision_history::Model,
) -> Result<Option<i64>> {
    let Some(root_revision_id) = history.deltav_root_revision_id else {
        return Ok(None);
    };
    FileRevision::find_by_id(root_revision_id)
        .select_only()
        .column(file_revision::Column::Sequence)
        .filter(file_revision::Column::HistoryId.eq(history.id))
        .into_tuple::<i64>()
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Resolves a live revision by public id into a DeltaV target.
///
/// Returns `None` unless the owning history is DeltaV activated, the revision's sequence
/// is at or after the activation root, and the revision still carries blob content.
pub async fn find_deltav_revision_by_public_id<C: ConnectionTrait>(
    db: &C,
    public_id: &str,
) -> Result<Option<DeltavRevisionTarget>> {
    let Some((revision, history)) = FileRevision::find()
        .find_also_related(FileRevisionHistory)
        .filter(file_revision::Column::PublicId.eq(public_id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .filter(file_revision::Column::PurgedAt.is_null())
        .one(db)
        .await
        .map_err(AsterError::from)?
    else {
        return Ok(None);
    };
    let Some(history) = history else {
        return Err(AsterError::internal_error(format!(
            "revision #{} has no history",
            revision.id
        )));
    };
    let Some(root_sequence) = deltav_root_sequence(db, &history).await? else {
        return Ok(None);
    };
    if revision.sequence < root_sequence || revision.blob_id.is_none() {
        return Ok(None);
    }
    let Some(file_id) = history.file_id else {
        return Ok(None);
    };
    let file = file::Entity::find_by_id(file_id)
        .one(db)
        .await
        .map_err(AsterError::from)?;
    Ok(file.map(|file| DeltavRevisionTarget {
        file,
        history,
        revision,
    }))
}

/// Lists live, blob-backed revisions at or after the DeltaV root, oldest sequence first.
///
/// Returns an empty vector when the history has no DeltaV root; `limit` caps the page size.
pub async fn find_deltav_revisions<C: ConnectionTrait>(
    db: &C,
    history: &file_revision_history::Model,
    limit: u64,
) -> Result<Vec<file_revision::Model>> {
    let Some(root_sequence) = deltav_root_sequence(db, history).await? else {
        return Ok(Vec::new());
    };
    FileRevision::find()
        .filter(file_revision::Column::HistoryId.eq(history.id))
        .filter(file_revision::Column::Sequence.gte(root_sequence))
        .filter(file_revision::Column::RetiredAt.is_null())
        .filter(file_revision::Column::PurgedAt.is_null())
        .filter(file_revision::Column::BlobId.is_not_null())
        .order_by_asc(file_revision::Column::Sequence)
        .limit(limit)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Loads property snapshots for the given revisions, grouped by revision id.
///
/// Queries in batches of 500 ids; each group is ordered by namespace then name, and
/// revisions without snapshots are absent from the map.
pub async fn find_properties_by_revision_ids<C: ConnectionTrait>(
    db: &C,
    revision_ids: &[i64],
) -> Result<HashMap<i64, Vec<file_revision_property::Model>>> {
    const BATCH_SIZE: usize = 500;
    let mut grouped = HashMap::new();
    for chunk in revision_ids.chunks(BATCH_SIZE) {
        let rows = file_revision_property::Entity::find()
            .filter(file_revision_property::Column::RevisionId.is_in(chunk.iter().copied()))
            .order_by_asc(file_revision_property::Column::RevisionId)
            .order_by_asc(file_revision_property::Column::Namespace)
            .order_by_asc(file_revision_property::Column::Name)
            .all(db)
            .await
            .map_err(AsterError::from)?;
        for property in rows {
            grouped
                .entry(property.revision_id)
                .or_insert_with(Vec::new)
                .push(property);
        }
    }
    Ok(grouped)
}

/// Returns the current head revision of a file.
///
/// Fails with an internal error when the history has no head, points at a missing
/// revision, or the head is retired or belongs to a different history.
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

/// Loads a file, its blob, and its current revision inside a fresh transaction.
///
/// PostgreSQL and MySQL use repeatable-read isolation so the three reads observe one
/// consistent snapshot; other backends use the default isolation level.
pub async fn find_file_blob_and_current_revision<C: ConnectionTrait + TransactionTrait>(
    db: &C,
    file_id: i64,
) -> Result<(
    file::Model,
    aster_drive_model::entities::file_blob::Model,
    file_revision::Model,
)> {
    let txn = match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => {
            db.begin_with_config(Some(IsolationLevel::RepeatableRead), None)
                .await
        }
        _ => db.begin().await,
    }
    .map_err(AsterError::from)?;
    let file = crate::db::repository::file_repo::find_by_id(&txn, file_id).await?;
    let blob = crate::db::repository::file_repo::find_blob_by_id(&txn, file.blob_id).await?;
    let revision = find_current_by_file_id(&txn, file_id).await?;
    txn.commit().await.map_err(AsterError::from)?;
    Ok((file, blob, revision))
}

/// Returns the ETag of a file's current head revision.
pub async fn current_etag<C: ConnectionTrait>(db: &C, file_id: i64) -> Result<String> {
    find_current_by_file_id(db, file_id)
        .await
        .map(|revision| revision.etag)
}

/// Returns the current head revision snapshot for each of the given file ids.
///
/// Fails with an internal error when any history has no head or a dangling head pointer.
pub async fn current_revision_snapshots_by_file_ids<C: ConnectionTrait>(
    db: &C,
    file_ids: &[i64],
) -> Result<HashMap<i64, CurrentRevisionSnapshot>> {
    if file_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let histories = FileRevisionHistory::find()
        .filter(file_revision_history::Column::FileId.is_in(file_ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)?;
    let current_ids = histories
        .iter()
        .filter_map(|history| history.current_revision_id)
        .collect::<Vec<_>>();
    let revisions = FileRevision::find()
        .filter(file_revision::Column::Id.is_in(current_ids))
        .all(db)
        .await
        .map_err(AsterError::from)?;
    let revisions_by_id = revisions
        .into_iter()
        .map(|revision| (revision.id, revision))
        .collect::<HashMap<_, _>>();
    histories
        .into_iter()
        .map(|history| {
            let file_id = history.file_id.ok_or_else(|| {
                AsterError::internal_error("active revision history has no file id")
            })?;
            let current_id = history.current_revision_id.ok_or_else(|| {
                AsterError::internal_error(format!(
                    "file #{file_id} revision history has no current revision"
                ))
            })?;
            let revision = revisions_by_id.get(&current_id).cloned().ok_or_else(|| {
                AsterError::internal_error(format!(
                    "file #{file_id} revision history has a dangling current pointer"
                ))
            })?;
            Ok((
                file_id,
                CurrentRevisionSnapshot {
                    revision,
                    deltav_controlled: history.deltav_controlled_at.is_some(),
                },
            ))
        })
        .collect()
}

/// Loads a file's revision history row, taking an exclusive row lock on PostgreSQL and MySQL.
///
/// Other backends read without a lock. Fails with an internal error when the file has no
/// history row.
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

/// Finds a live revision by id, scoped to the given file's history.
///
/// Returns `None` when the revision does not exist, belongs to another file, or is retired.
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

/// Lists all live revisions of a file, newest sequence first.
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

/// Lists live revisions of a file with keyset pagination, newest sequence first.
///
/// When `after_sequence` is `Some`, only revisions with a strictly smaller sequence are
/// returned; `limit` caps the page size.
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

/// Lists the property snapshots of a revision, ordered by namespace then name.
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

/// Restores a file's user properties from the snapshots of a revision.
///
/// Deletes the file's current user-namespaced properties first, then reinserts the
/// snapshot values; system namespaces are left untouched.
pub async fn restore_user_properties<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    revision_id: i64,
) -> Result<()> {
    entity_property::Entity::delete_many()
        .filter(entity_property::Column::EntityType.eq(EntityType::File))
        .filter(entity_property::Column::EntityId.eq(file_id))
        .filter(
            crate::db::repository::property_repo::user_namespace_condition(
                db.get_database_backend(),
            ),
        )
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    let properties = find_properties(db, revision_id).await?;
    if !properties.is_empty() {
        entity_property::Entity::insert_many(properties.into_iter().map(|property| {
            entity_property::ActiveModel {
                entity_type: Set(EntityType::File),
                entity_id: Set(file_id),
                namespace: Set(property.namespace),
                name: Set(property.name),
                value: Set(property.xml_value),
                ..Default::default()
            }
        }))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    }
    Ok(())
}

/// Lists live revisions backed by the given blob together with their histories,
/// ordered by revision id.
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

/// Counts the live revisions in a file's history.
pub async fn count_by_file_id<C: ConnectionTrait>(db: &C, file_id: i64) -> Result<u64> {
    let history = find_history_by_file_id(db, file_id).await?;
    FileRevision::find()
        .filter(file_revision::Column::HistoryId.eq(history.id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .count(db)
        .await
        .map_err(AsterError::from)
}

/// Finds the oldest live revision of a file that is not the current head.
///
/// Returns `None` when the history has no revisions besides the head, or is empty.
pub async fn find_oldest_non_current<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
) -> Result<Option<file_revision::Model>> {
    let history = find_history_by_file_id(db, file_id).await?;
    let mut query = FileRevision::find()
        .filter(file_revision::Column::HistoryId.eq(history.id))
        .filter(file_revision::Column::RetiredAt.is_null())
        .order_by_asc(file_revision::Column::Sequence);
    if let Some(current_revision_id) = history.current_revision_id {
        query = query.filter(file_revision::Column::Id.ne(current_revision_id));
    }
    query.one(db).await.map_err(AsterError::from)
}

/// Tombstones a revision by clearing its blob reference and stamping it retired and purged.
///
/// A live successor revision is relinked to this revision's predecessor first so the
/// version chain stays connected.
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

/// Retires the revision histories of the given files and returns the released blob references.
///
/// Every live, blob-backed revision has its blob reference cleared and is stamped retired
/// and purged (already tombstoned revisions keep their timestamps); the histories are
/// detached from their files and heads, then stamped retired. The returned pairs are
/// `(blob_id, logical_size)` for each revision whose blob was released.
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
        .filter(file_revision::Column::RetiredAt.is_null())
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

/// Sums the logical sizes of a file's live revisions, excluding the current head.
///
/// Returns `0` when the history has no non-head revisions.
pub async fn sum_non_current_sizes_by_file_id(
    db: &sea_orm::DatabaseConnection,
    file_id: i64,
) -> Result<i64> {
    let history = find_history_by_file_id(db, file_id).await?;
    let mut query = FileRevision::find()
        .select_only()
        .column_as(sum_size_expr(db.get_database_backend()), "sum")
        .filter(file_revision::Column::HistoryId.eq(history.id))
        .filter(file_revision::Column::RetiredAt.is_null());
    if let Some(current_revision_id) = history.current_revision_id {
        query = query.filter(file_revision::Column::Id.ne(current_revision_id));
    }
    Ok(query
        .into_tuple::<Option<i64>>()
        .one(db)
        .await
        .map_err(AsterError::from)?
        .flatten()
        .unwrap_or(0))
}

/// Sums the non-head revision logical sizes per file across the given file ids.
///
/// Files without non-head revisions are absent from the returned map.
pub async fn sum_non_current_sizes_by_file_ids<C: ConnectionTrait>(
    db: &C,
    file_ids: &[i64],
) -> Result<HashMap<i64, i64>> {
    if file_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = FileRevision::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            file_revision::Relation::History.def(),
        )
        .select_only()
        .column(file_revision_history::Column::FileId)
        .column_as(sum_size_expr(db.get_database_backend()), "sum")
        .filter(file_revision_history::Column::FileId.is_in(file_ids.iter().copied()))
        .filter(file_revision::Column::RetiredAt.is_null())
        .filter(
            Expr::col((
                file_revision_history::Entity,
                file_revision_history::Column::CurrentRevisionId,
            ))
            .is_null()
            .or(
                Expr::col((file_revision::Entity, file_revision::Column::Id)).ne(Expr::col((
                    file_revision_history::Entity,
                    file_revision_history::Column::CurrentRevisionId,
                ))),
            ),
        )
        .group_by(file_revision_history::Column::FileId)
        .into_tuple::<(i64, Option<i64>)>()
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok(rows
        .into_iter()
        .map(|(file_id, size)| (file_id, size.unwrap_or(0)))
        .collect())
}

/// Counts the live non-head revisions referencing each of the given blobs.
///
/// Blobs without such references are absent from the returned map.
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
            Expr::col((
                file_revision_history::Entity,
                file_revision_history::Column::CurrentRevisionId,
            ))
            .is_null()
            .or(
                Expr::col((file_revision::Entity, file_revision::Column::Id)).ne(Expr::col((
                    file_revision_history::Entity,
                    file_revision_history::Column::CurrentRevisionId,
                ))),
            ),
        )
        .group_by(file_revision::Column::BlobId)
        .into_tuple::<(i64, i64)>()
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok(rows.into_iter().collect())
}

/// Counts the live non-head revisions referencing a single blob; `0` when there are none.
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

/// Repoints every live revision from one blob to another, returning the updated row count.
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

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database, EntityTrait, QueryFilter};
    use std::sync::Arc;
    use tokio::sync::Barrier;

    async fn activate_deltav_with_retry(
        db: &sea_orm::DatabaseConnection,
        file_id: i64,
    ) -> Result<file_revision_history::Model> {
        aster_forge_db::transaction::with_transaction_retry(
            db,
            &aster_forge_db::retry::RetryConfig::deadlock(),
            move |txn| Box::pin(async move { activate_deltav(txn, file_id).await }),
            |error: &AsterError| {
                error
                    .database_error_kind()
                    .is_some_and(aster_forge_db::DatabaseErrorKind::is_transient_locking)
            },
        )
        .await
    }

    #[tokio::test]
    async fn concurrent_deltav_activation_preserves_one_root_and_timestamp() {
        let path = std::env::temp_dir().join(format!(
            "asterdrive-deltav-activation-{}.sqlite3",
            uuid::Uuid::new_v4().simple()
        ));
        let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        options.max_connections(4).min_connections(1);
        let db = Database::connect(options).await.unwrap();
        db.execute_unprepared(
            "CREATE TABLE file_revision_histories (id INTEGER PRIMARY KEY, public_id TEXT NOT NULL, file_id INTEGER, current_revision_id INTEGER, next_sequence INTEGER NOT NULL, deltav_controlled_at TEXT, deltav_root_revision_id INTEGER, created_at TEXT NOT NULL, retired_at TEXT); \
             CREATE TABLE file_revisions (id INTEGER PRIMARY KEY, public_id TEXT NOT NULL, history_id INTEGER NOT NULL, sequence INTEGER NOT NULL, predecessor_revision_id INTEGER, blob_id INTEGER, logical_size INTEGER NOT NULL, mime_type TEXT, etag TEXT NOT NULL, content_sha256 TEXT, creator_user_id INTEGER, creator_display_name TEXT, comment TEXT, reason TEXT NOT NULL, created_at TEXT NOT NULL, retired_at TEXT, purged_at TEXT); \
             INSERT INTO file_revision_histories VALUES (1, 'history', 7, 2, 3, NULL, NULL, '2026-08-01T00:00:00Z', NULL); \
             INSERT INTO file_revisions VALUES (2, 'current', 1, 2, 1, 9, 5, 'text/plain', 'current-etag', NULL, NULL, NULL, NULL, 'overwrite', '2026-08-03T00:00:00Z', NULL, NULL);",
        )
        .await
        .unwrap();

        let barrier = Arc::new(Barrier::new(2));
        let left_db = db.clone();
        let left_barrier = barrier.clone();
        let left = tokio::spawn(async move {
            left_barrier.wait().await;
            activate_deltav_with_retry(&left_db, 7).await
        });
        let right_db = db.clone();
        let right = tokio::spawn(async move {
            barrier.wait().await;
            activate_deltav_with_retry(&right_db, 7).await
        });
        let (left, right) = tokio::join!(left, right);
        let left = left.unwrap().unwrap();
        let right = right.unwrap().unwrap();

        assert_eq!(left.deltav_root_revision_id, Some(2));
        assert_eq!(right.deltav_root_revision_id, Some(2));
        assert_eq!(left.deltav_controlled_at, right.deltav_controlled_at);
        let final_history = find_history_by_file_id(&db, 7).await.unwrap();
        assert_eq!(final_history.deltav_root_revision_id, Some(2));
        assert_eq!(
            final_history.deltav_controlled_at,
            left.deltav_controlled_at
        );

        db.close().await.unwrap();
        std::fs::remove_file(&path).unwrap();
    }

    #[tokio::test]
    async fn retire_histories_preserves_existing_tombstone_timestamps() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared(
            "CREATE TABLE file_revision_histories (id INTEGER PRIMARY KEY, public_id TEXT NOT NULL, file_id INTEGER, current_revision_id INTEGER, next_sequence INTEGER NOT NULL, deltav_controlled_at TEXT, deltav_root_revision_id INTEGER, created_at TEXT NOT NULL, retired_at TEXT); \
             CREATE TABLE file_revisions (id INTEGER PRIMARY KEY, public_id TEXT NOT NULL, history_id INTEGER NOT NULL, sequence INTEGER NOT NULL, predecessor_revision_id INTEGER, blob_id INTEGER, logical_size INTEGER NOT NULL, mime_type TEXT, etag TEXT NOT NULL, content_sha256 TEXT, creator_user_id INTEGER, creator_display_name TEXT, comment TEXT, reason TEXT NOT NULL, created_at TEXT NOT NULL, retired_at TEXT, purged_at TEXT); \
             INSERT INTO file_revision_histories VALUES (1, 'history', 7, 2, 3, NULL, NULL, '2026-08-01T00:00:00Z', NULL); \
             INSERT INTO file_revisions VALUES (1, 'old', 1, 1, NULL, NULL, 5, NULL, 'old-etag', NULL, NULL, NULL, NULL, 'overwrite', '2026-08-01T00:00:00Z', '2026-08-02T00:00:00Z', '2026-08-02T00:00:00Z'); \
             INSERT INTO file_revisions VALUES (2, 'current', 1, 2, 1, 9, 5, 'text/plain', 'current-etag', NULL, NULL, NULL, NULL, 'overwrite', '2026-08-03T00:00:00Z', NULL, NULL);",
        )
        .await
        .unwrap();

        let old = FileRevision::find_by_id(1).one(&db).await.unwrap().unwrap();
        retire_histories(&db, &[7]).await.unwrap();
        let retired = FileRevision::find_by_id(1).one(&db).await.unwrap().unwrap();
        assert_eq!(retired.retired_at, old.retired_at);
        assert_eq!(retired.purged_at, old.purged_at);
        assert!(
            FileRevision::find()
                .filter(file_revision::Column::Id.eq(2))
                .one(&db)
                .await
                .unwrap()
                .unwrap()
                .retired_at
                .is_some()
        );
    }
}
