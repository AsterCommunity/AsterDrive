//! 跨数据库 / 跨存储的一致性审计。
//!
//! 这些检查都不是在线请求路径上的业务逻辑，而是偏运维的“全局事实核对”：
//! 例如配额计数漂移、blob 引用计数漂移、对象存储孤儿文件和目录树损坏。

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait, JoinType, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait, sea_query::Expr,
};
use serde::Serialize;

use crate::db::repository::{file_repo, revision_repo, team_repo, user_repo};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::services::{files::thumbnail, media::processing};
use crate::storage::DriverRegistry;
use aster_drive_model::entities::{
    file::{self, Entity as File},
    file_blob::{self, Entity as FileBlob},
    file_revision::{self, Entity as FileRevision},
    file_revision_history::{self},
    folder::{self, Entity as Folder},
    team::{self, Entity as Team},
    upload_session::{self, Entity as UploadSession},
    user::{self, Entity as User},
};
use aster_drive_storage::StoragePathVisitor;

// 审计走全表扫描，但必须控制单批内存占用；因此统一按主键顺序分批拉取。
const INTEGRITY_BATCH_SIZE: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageOwnerKind {
    User,
    Team,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StorageUsageDrift {
    pub owner_kind: StorageOwnerKind,
    pub owner_id: i64,
    pub recorded_bytes: i64,
    pub actual_bytes: i64,
    pub delta_bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlobRefCountDrift {
    pub blob_id: i64,
    pub policy_id: i64,
    pub storage_path: Option<String>,
    pub recorded_ref_count: i32,
    pub actual_ref_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevisionLedgerIssue {
    pub file_id: Option<i64>,
    pub history_id: i64,
    pub revision_id: Option<i64>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlobObjectIssue {
    pub policy_id: i64,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThumbnailIssue {
    pub policy_id: i64,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderTreeIssueKind {
    MissingParent,
    CrossScopeParent,
    Cycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FolderTreeIssue {
    pub kind: FolderTreeIssueKind,
    pub folder_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<i64>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct StorageObjectAudit {
    pub scanned_policies: usize,
    pub completed_policies: usize,
    pub scanned_blob_records: usize,
    pub scanned_objects: usize,
    pub ignored_paths: usize,
    pub peak_path_batch: usize,
    pub missing_blob_objects_total: usize,
    pub untracked_objects_total: usize,
    pub orphan_thumbnails_total: usize,
    pub findings_truncated: bool,
    pub missing_blob_objects: Vec<BlobObjectIssue>,
    pub untracked_objects: Vec<BlobObjectIssue>,
    pub orphan_thumbnails: Vec<ThumbnailIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageObjectFinding {
    MissingBlob(BlobObjectIssue),
    UntrackedObject(BlobObjectIssue),
    OrphanThumbnail(ThumbnailIssue),
}

pub trait StorageObjectFindingSink: Send {
    fn record(&mut self, finding: StorageObjectFinding) -> Result<()>;
}

struct NoopStorageObjectFindingSink;

impl StorageObjectFindingSink for NoopStorageObjectFindingSink {
    fn record(&mut self, _finding: StorageObjectFinding) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditFindingSummary<T> {
    pub total: usize,
    pub samples: Vec<T>,
}

impl<T> AuditFindingSummary<T> {
    fn new() -> Self {
        Self {
            total: 0,
            samples: Vec::new(),
        }
    }

    fn record(&mut self, value: T, limit: usize) {
        self.total += 1;
        if self.samples.len() < limit {
            self.samples.push(value);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum StorageOwner {
    User(i64),
    Team(i64),
}

#[derive(Clone, Copy)]
struct FolderNode {
    parent_id: Option<i64>,
    owner_user_id: Option<i64>,
    team_id: Option<i64>,
}

const STORAGE_AUDIT_PATH_BATCH_SIZE: usize = 256;

struct StorageAuditVisitor<'db, 'report, 'sink, C: ConnectionTrait> {
    db: &'db C,
    policy_id: i64,
    thumbnail_max_dimension: u32,
    image_preview_max_dimension: u32,
    finding_limit: Option<usize>,
    report: &'report mut StorageObjectAudit,
    pending_paths: Vec<String>,
    sink: &'sink mut dyn StorageObjectFindingSink,
}

fn record_finding<T>(items: &mut Vec<T>, total: &mut usize, value: T, limit: Option<usize>) {
    *total += 1;
    if limit.is_none_or(|limit| items.len() < limit) {
        items.push(value);
    }
}

impl<'db, 'report, 'sink, C: ConnectionTrait + Sync> StorageAuditVisitor<'db, 'report, 'sink, C> {
    async fn flush(&mut self) -> aster_drive_storage::Result<()> {
        if self.pending_paths.is_empty() {
            return Ok(());
        }
        let paths = std::mem::take(&mut self.pending_paths);
        let tracked_blobs = FileBlob::find()
            .filter(file_blob::Column::PolicyId.eq(self.policy_id))
            .filter(file_blob::Column::StoragePath.is_in(paths.iter().cloned()))
            .all(self.db)
            .await
            .map_aster_err(AsterError::database_operation)
            .map_err(|error| {
                aster_drive_storage::storage_driver_error(
                    aster_drive_storage::StorageErrorKind::Transient,
                    error.to_string(),
                )
            })?
            .into_iter()
            .filter_map(|blob| {
                blob.storage_path.as_ref().map(|path| {
                    (
                        path.clone(),
                        (blob.id, blob.hash.clone(), blob.is_virtual_empty()),
                    )
                })
            })
            .collect::<HashMap<_, _>>();
        let tracked_temp_paths = UploadSession::find()
            .select_only()
            .column(upload_session::Column::ObjectTempKey)
            .filter(upload_session::Column::PolicyId.eq(self.policy_id))
            .filter(upload_session::Column::ObjectTempKey.is_in(paths.iter().cloned()))
            .into_tuple::<Option<String>>()
            .all(self.db)
            .await
            .map_aster_err(AsterError::database_operation)
            .map_err(|error| {
                aster_drive_storage::storage_driver_error(
                    aster_drive_storage::StorageErrorKind::Transient,
                    error.to_string(),
                )
            })?
            .into_iter()
            .flatten()
            .collect::<HashSet<_>>();
        let hashes = paths
            .iter()
            .filter_map(|path| {
                path.rsplit('/')
                    .next()
                    .and_then(|name| name.strip_suffix(".webp"))
                    .filter(|hash| hash.len() >= 4)
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();
        let derivative_blobs = if hashes.is_empty() {
            Vec::new()
        } else {
            FileBlob::find()
                .filter(file_blob::Column::PolicyId.eq(self.policy_id))
                .filter(file_blob::Column::Hash.is_in(hashes))
                .all(self.db)
                .await
                .map_aster_err(AsterError::database_operation)
                .map_err(|error| {
                    aster_drive_storage::storage_driver_error(
                        aster_drive_storage::StorageErrorKind::Transient,
                        error.to_string(),
                    )
                })?
        };

        for path in paths {
            if tracked_blobs.contains_key(&path) || tracked_temp_paths.contains(&path) {
                continue;
            }
            let hash = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".webp"));
            if thumbnail::is_thumbnail_path(&path) {
                let tracked = hash.is_some_and(|hash| {
                    derivative_blobs.iter().any(|blob| {
                        !blob.is_virtual_empty()
                            && blob.hash == hash
                            && processing::known_thumbnail_cache_paths(
                                hash,
                                self.thumbnail_max_dimension,
                            )
                            .iter()
                            .any(|expected| expected == &path)
                    })
                });
                if tracked {
                    continue;
                }
                let finding = ThumbnailIssue {
                    policy_id: self.policy_id,
                    path,
                };
                self.sink
                    .record(StorageObjectFinding::OrphanThumbnail(finding.clone()))?;
                record_finding(
                    &mut self.report.orphan_thumbnails,
                    &mut self.report.orphan_thumbnails_total,
                    finding,
                    self.finding_limit,
                );
                continue;
            }
            if thumbnail::is_image_preview_path(&path) {
                let tracked = hash.is_some_and(|hash| {
                    derivative_blobs.iter().any(|blob| {
                        !blob.is_virtual_empty()
                            && blob.hash == hash
                            && processing::known_image_preview_cache_paths(
                                hash,
                                self.image_preview_max_dimension,
                            )
                            .iter()
                            .any(|expected| expected == &path)
                    })
                });
                if tracked {
                    continue;
                }
            }
            let finding = BlobObjectIssue {
                policy_id: self.policy_id,
                path,
                blob_id: None,
            };
            self.sink
                .record(StorageObjectFinding::UntrackedObject(finding.clone()))?;
            record_finding(
                &mut self.report.untracked_objects,
                &mut self.report.untracked_objects_total,
                finding,
                self.finding_limit,
            );
        }
        self.report.findings_truncated = self.report.missing_blob_objects.len()
            < self.report.missing_blob_objects_total
            || self.report.untracked_objects.len() < self.report.untracked_objects_total
            || self.report.orphan_thumbnails.len() < self.report.orphan_thumbnails_total;
        Ok(())
    }
}

#[async_trait]
impl<C: ConnectionTrait + Sync> StoragePathVisitor for StorageAuditVisitor<'_, '_, '_, C> {
    async fn visit_path(&mut self, path: String) -> aster_drive_storage::Result<()> {
        self.report.scanned_objects += 1;
        if path.starts_with(".staging/") {
            self.report.ignored_paths += 1;
            return Ok(());
        }
        self.pending_paths.push(path);
        self.report.peak_path_batch = self.report.peak_path_batch.max(self.pending_paths.len());
        if self.pending_paths.len() >= STORAGE_AUDIT_PATH_BATCH_SIZE {
            self.flush().await?;
        }
        Ok(())
    }
}

fn add_usage(
    total_by_owner: &mut HashMap<StorageOwner, i64>,
    owner: StorageOwner,
    bytes: i64,
) -> Result<()> {
    let entry = total_by_owner.entry(owner).or_insert(0);
    *entry = entry.checked_add(bytes).ok_or_else(|| {
        AsterError::internal_error(format!(
            "storage usage overflow while accumulating owner {:?}",
            owner
        ))
    })?;
    Ok(())
}

async fn load_actual_storage_usage_for_owners<C: ConnectionTrait>(
    db: &C,
    owners: &[StorageOwner],
) -> Result<HashMap<StorageOwner, i64>> {
    let user_ids = owners
        .iter()
        .filter_map(|owner| match owner {
            StorageOwner::User(id) => Some(*id),
            StorageOwner::Team(_) => None,
        })
        .collect::<Vec<_>>();
    let team_ids = owners
        .iter()
        .filter_map(|owner| match owner {
            StorageOwner::User(_) => None,
            StorageOwner::Team(id) => Some(*id),
        })
        .collect::<Vec<_>>();
    let sum_type = match db.get_database_backend() {
        sea_orm::DbBackend::Postgres => "bigint",
        sea_orm::DbBackend::MySql => "signed",
        _ => "integer",
    };
    let mut totals = HashMap::with_capacity(owners.len());

    if !user_ids.is_empty() {
        let rows = File::find()
            .select_only()
            .column(file::Column::OwnerUserId)
            .column_as(
                Expr::col(file::Column::Size).sum().cast_as(sum_type),
                "total",
            )
            .filter(file::Column::OwnerUserId.is_in(user_ids.iter().copied()))
            .filter(file::Column::TeamId.is_null())
            .group_by(file::Column::OwnerUserId)
            .into_tuple::<(Option<i64>, Option<i64>)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        for (owner_id, total) in rows {
            if let Some(owner_id) = owner_id {
                totals.insert(StorageOwner::User(owner_id), total.unwrap_or(0));
            }
        }

        let rows = FileRevision::find()
            .join(JoinType::InnerJoin, file_revision::Relation::History.def())
            .join(
                JoinType::InnerJoin,
                file_revision_history::Relation::File.def(),
            )
            .select_only()
            .column(file::Column::OwnerUserId)
            .column_as(
                Expr::col(file_revision::Column::LogicalSize)
                    .sum()
                    .cast_as(sum_type),
                "total",
            )
            .filter(file::Column::OwnerUserId.is_in(user_ids.iter().copied()))
            .filter(file::Column::TeamId.is_null())
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
            .group_by(file::Column::OwnerUserId)
            .into_tuple::<(Option<i64>, Option<i64>)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        for (owner_id, total) in rows {
            if let Some(owner_id) = owner_id {
                add_usage(
                    &mut totals,
                    StorageOwner::User(owner_id),
                    total.unwrap_or(0),
                )?;
            }
        }
    }

    if !team_ids.is_empty() {
        let rows = File::find()
            .select_only()
            .column(file::Column::TeamId)
            .column_as(
                Expr::col(file::Column::Size).sum().cast_as(sum_type),
                "total",
            )
            .filter(file::Column::TeamId.is_in(team_ids.iter().copied()))
            .group_by(file::Column::TeamId)
            .into_tuple::<(Option<i64>, Option<i64>)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        for (owner_id, total) in rows {
            if let Some(owner_id) = owner_id {
                totals.insert(StorageOwner::Team(owner_id), total.unwrap_or(0));
            }
        }

        let rows = FileRevision::find()
            .join(JoinType::InnerJoin, file_revision::Relation::History.def())
            .join(
                JoinType::InnerJoin,
                file_revision_history::Relation::File.def(),
            )
            .select_only()
            .column(file::Column::TeamId)
            .column_as(
                Expr::col(file_revision::Column::LogicalSize)
                    .sum()
                    .cast_as(sum_type),
                "total",
            )
            .filter(file::Column::TeamId.is_in(team_ids.iter().copied()))
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
            .group_by(file::Column::TeamId)
            .into_tuple::<(Option<i64>, Option<i64>)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        for (owner_id, total) in rows {
            if let Some(owner_id) = owner_id {
                add_usage(
                    &mut totals,
                    StorageOwner::Team(owner_id),
                    total.unwrap_or(0),
                )?;
            }
        }
    }

    Ok(totals)
}

pub async fn audit_storage_usage<C: ConnectionTrait>(db: &C) -> Result<Vec<StorageUsageDrift>> {
    Ok(audit_storage_usage_with_limit(db, usize::MAX)
        .await?
        .samples)
}

pub async fn audit_storage_usage_with_limit<C: ConnectionTrait>(
    db: &C,
    sample_limit: usize,
) -> Result<AuditFindingSummary<StorageUsageDrift>> {
    let mut drifts = AuditFindingSummary::new();

    let mut last_user_id = None;
    loop {
        let mut query = User::find()
            .select_only()
            .column(user::Column::Id)
            .column(user::Column::StorageUsed)
            .order_by_asc(user::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(id) = last_user_id {
            query = query.filter(user::Column::Id.gt(id));
        }
        let rows = query
            .into_tuple::<(i64, i64)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if rows.is_empty() {
            break;
        }
        last_user_id = rows.last().map(|(id, _)| *id);
        let owners = rows
            .iter()
            .map(|(id, _)| StorageOwner::User(*id))
            .collect::<Vec<_>>();
        let actual_usage = load_actual_storage_usage_for_owners(db, &owners).await?;
        for (id, recorded_bytes) in rows {
            let actual_bytes = actual_usage
                .get(&StorageOwner::User(id))
                .copied()
                .unwrap_or(0);
            if recorded_bytes != actual_bytes {
                drifts.record(
                    StorageUsageDrift {
                        owner_kind: StorageOwnerKind::User,
                        owner_id: id,
                        recorded_bytes,
                        actual_bytes,
                        delta_bytes: actual_bytes - recorded_bytes,
                    },
                    sample_limit,
                );
            }
        }
    }

    let mut last_team_id = None;
    loop {
        let mut query = Team::find()
            .select_only()
            .column(team::Column::Id)
            .column(team::Column::StorageUsed)
            .order_by_asc(team::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(id) = last_team_id {
            query = query.filter(team::Column::Id.gt(id));
        }
        let rows = query
            .into_tuple::<(i64, i64)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if rows.is_empty() {
            break;
        }
        last_team_id = rows.last().map(|(id, _)| *id);
        let owners = rows
            .iter()
            .map(|(id, _)| StorageOwner::Team(*id))
            .collect::<Vec<_>>();
        let actual_usage = load_actual_storage_usage_for_owners(db, &owners).await?;
        for (id, recorded_bytes) in rows {
            let actual_bytes = actual_usage
                .get(&StorageOwner::Team(id))
                .copied()
                .unwrap_or(0);
            if recorded_bytes != actual_bytes {
                drifts.record(
                    StorageUsageDrift {
                        owner_kind: StorageOwnerKind::Team,
                        owner_id: id,
                        recorded_bytes,
                        actual_bytes,
                        delta_bytes: actual_bytes - recorded_bytes,
                    },
                    sample_limit,
                );
            }
        }
    }

    Ok(drifts)
}

/// Checks the cross-table invariants that database FKs cannot express portably:
/// one live history/head per file, a matching files projection, and local
/// predecessor links.
pub async fn audit_revision_ledger<C: ConnectionTrait>(db: &C) -> Result<Vec<RevisionLedgerIssue>> {
    Ok(audit_revision_ledger_with_limit(db, usize::MAX)
        .await?
        .samples)
}

pub async fn audit_revision_ledger_with_limit<C: ConnectionTrait>(
    db: &C,
    sample_limit: usize,
) -> Result<AuditFindingSummary<RevisionLedgerIssue>> {
    let mut issues = AuditFindingSummary::new();

    let mut after_file_id = None;
    loop {
        let mut query = File::find()
            .order_by_asc(file::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(id) = after_file_id {
            query = query.filter(file::Column::Id.gt(id));
        }
        let files = query
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if files.is_empty() {
            break;
        }
        after_file_id = files.last().map(|file| file.id);
        let file_ids = files.iter().map(|file| file.id).collect::<Vec<_>>();
        let histories = file_revision_history::Entity::find()
            .filter(file_revision_history::Column::FileId.is_in(file_ids))
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        let mut histories_by_file = HashMap::<i64, Vec<_>>::new();
        for history in histories {
            if let Some(file_id) = history.file_id {
                histories_by_file.entry(file_id).or_default().push(history);
            }
        }
        let current_ids = histories_by_file
            .values()
            .flatten()
            .filter_map(|history| history.current_revision_id)
            .collect::<Vec<_>>();
        let current_revisions = FileRevision::find()
            .filter(file_revision::Column::Id.is_in(current_ids))
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?
            .into_iter()
            .map(|revision| (revision.id, revision))
            .collect::<HashMap<_, _>>();

        for file in files {
            let Some(matching_histories) = histories_by_file.get(&file.id) else {
                issues.record(
                    RevisionLedgerIssue {
                        file_id: Some(file.id),
                        history_id: 0,
                        revision_id: None,
                        detail: "live file has no revision history".to_string(),
                    },
                    sample_limit,
                );
                continue;
            };
            let history = &matching_histories[0];
            if matching_histories.len() != 1 {
                issues.record(
                    RevisionLedgerIssue {
                        file_id: Some(file.id),
                        history_id: history.id,
                        revision_id: None,
                        detail: format!(
                            "live file has {} revision histories",
                            matching_histories.len()
                        ),
                    },
                    sample_limit,
                );
                continue;
            }
            let Some(current_id) = history.current_revision_id else {
                issues.record(
                    RevisionLedgerIssue {
                        file_id: Some(file.id),
                        history_id: history.id,
                        revision_id: None,
                        detail: "live history has no current revision".to_string(),
                    },
                    sample_limit,
                );
                continue;
            };
            let Some(current) = current_revisions.get(&current_id) else {
                issues.record(
                    RevisionLedgerIssue {
                        file_id: Some(file.id),
                        history_id: history.id,
                        revision_id: Some(current_id),
                        detail: "history current pointer is dangling".to_string(),
                    },
                    sample_limit,
                );
                continue;
            };
            if current.history_id != history.id || current.retired_at.is_some() {
                issues.record(
                    RevisionLedgerIssue {
                        file_id: Some(file.id),
                        history_id: history.id,
                        revision_id: Some(current.id),
                        detail:
                            "history current pointer targets another history or retired revision"
                                .to_string(),
                    },
                    sample_limit,
                );
            }
            if current.blob_id != Some(file.blob_id)
                || file.size != current.logical_size
                || current.mime_type.as_deref() != Some(file.mime_type.as_str())
            {
                issues.record(
                    RevisionLedgerIssue {
                        file_id: Some(file.id),
                        history_id: history.id,
                        revision_id: Some(current.id),
                        detail: "files projection does not match current revision".to_string(),
                    },
                    sample_limit,
                );
            }
        }
    }

    let mut after_history_id = None;
    loop {
        let mut query = file_revision_history::Entity::find()
            .filter(file_revision_history::Column::FileId.is_not_null())
            .filter(file_revision_history::Column::RetiredAt.is_null())
            .order_by_asc(file_revision_history::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(id) = after_history_id {
            query = query.filter(file_revision_history::Column::Id.gt(id));
        }
        let histories = query
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if histories.is_empty() {
            break;
        }
        after_history_id = histories.last().map(|history| history.id);
        let history_ids = histories
            .iter()
            .map(|history| history.id)
            .collect::<Vec<_>>();
        let histories_by_id = histories
            .into_iter()
            .map(|history| (history.id, history))
            .collect::<HashMap<_, _>>();
        let mut after_revision_id = None;
        loop {
            let mut revision_query = FileRevision::find()
                .filter(file_revision::Column::HistoryId.is_in(history_ids.iter().copied()))
                .filter(file_revision::Column::RetiredAt.is_null())
                .order_by_asc(file_revision::Column::Id)
                .limit(INTEGRITY_BATCH_SIZE);
            if let Some(id) = after_revision_id {
                revision_query = revision_query.filter(file_revision::Column::Id.gt(id));
            }
            let revisions = revision_query
                .all(db)
                .await
                .map_aster_err(AsterError::database_operation)?;
            if revisions.is_empty() {
                break;
            }
            after_revision_id = revisions.last().map(|revision| revision.id);
            let predecessor_ids = revisions
                .iter()
                .filter_map(|revision| revision.predecessor_revision_id)
                .collect::<Vec<_>>();
            let predecessors = FileRevision::find()
                .filter(file_revision::Column::Id.is_in(predecessor_ids))
                .all(db)
                .await
                .map_aster_err(AsterError::database_operation)?
                .into_iter()
                .map(|revision| (revision.id, revision))
                .collect::<HashMap<_, _>>();
            for revision in revisions {
                let history = &histories_by_id[&revision.history_id];
                if let Some(predecessor_id) = revision.predecessor_revision_id {
                    match predecessors.get(&predecessor_id) {
                        Some(predecessor) if predecessor.history_id == history.id => {}
                        Some(_) => issues.record(
                            RevisionLedgerIssue {
                                file_id: history.file_id,
                                history_id: history.id,
                                revision_id: Some(revision.id),
                                detail: "predecessor points to another history".to_string(),
                            },
                            sample_limit,
                        ),
                        None => issues.record(
                            RevisionLedgerIssue {
                                file_id: history.file_id,
                                history_id: history.id,
                                revision_id: Some(revision.id),
                                detail: "predecessor pointer is dangling".to_string(),
                            },
                            sample_limit,
                        ),
                    }
                }
            }
        }
    }
    Ok(issues)
}

pub async fn fix_storage_usage_drifts<C: ConnectionTrait>(
    db: &C,
    drifts: &[StorageUsageDrift],
) -> Result<()> {
    for drift in drifts {
        match drift.owner_kind {
            StorageOwnerKind::User => {
                user_repo::set_storage_used(db, drift.owner_id, drift.actual_bytes).await?;
            }
            StorageOwnerKind::Team => {
                team_repo::set_storage_used(db, drift.owner_id, drift.actual_bytes).await?;
            }
        }
    }

    Ok(())
}

pub async fn fix_storage_usage_all<C: ConnectionTrait>(db: &C) -> Result<usize> {
    let mut fixed = 0;
    let mut last_user_id = None;
    loop {
        let mut query = User::find()
            .select_only()
            .column(user::Column::Id)
            .column(user::Column::StorageUsed)
            .order_by_asc(user::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(id) = last_user_id {
            query = query.filter(user::Column::Id.gt(id));
        }
        let rows = query
            .into_tuple::<(i64, i64)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if rows.is_empty() {
            break;
        }
        last_user_id = rows.last().map(|(id, _)| *id);
        let owners = rows
            .iter()
            .map(|(id, _)| StorageOwner::User(*id))
            .collect::<Vec<_>>();
        let actual = load_actual_storage_usage_for_owners(db, &owners).await?;
        for (id, recorded) in rows {
            let actual_bytes = actual.get(&StorageOwner::User(id)).copied().unwrap_or(0);
            if recorded != actual_bytes {
                user_repo::set_storage_used(db, id, actual_bytes).await?;
                fixed += 1;
            }
        }
    }

    let mut last_team_id = None;
    loop {
        let mut query = Team::find()
            .select_only()
            .column(team::Column::Id)
            .column(team::Column::StorageUsed)
            .order_by_asc(team::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(id) = last_team_id {
            query = query.filter(team::Column::Id.gt(id));
        }
        let rows = query
            .into_tuple::<(i64, i64)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if rows.is_empty() {
            break;
        }
        last_team_id = rows.last().map(|(id, _)| *id);
        let owners = rows
            .iter()
            .map(|(id, _)| StorageOwner::Team(*id))
            .collect::<Vec<_>>();
        let actual = load_actual_storage_usage_for_owners(db, &owners).await?;
        for (id, recorded) in rows {
            let actual_bytes = actual.get(&StorageOwner::Team(id)).copied().unwrap_or(0);
            if recorded != actual_bytes {
                team_repo::set_storage_used(db, id, actual_bytes).await?;
                fixed += 1;
            }
        }
    }
    Ok(fixed)
}

/// Computes reference counts for one bounded blob page.
///
/// Both `doctor --deep` and the admin blob-maintenance task use this helper so
/// neither path needs to materialize the reference truth for the whole instance.
pub(crate) async fn actual_blob_ref_counts_for_blobs<C: ConnectionTrait>(
    db: &C,
    blob_ids: &[i64],
) -> Result<HashMap<i64, i64>> {
    if blob_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let file_refs = file_repo::count_blob_refs_from_files_for_blobs(db, blob_ids).await?;
    let revision_refs = revision_repo::count_non_current_blob_refs_for_blobs(db, blob_ids).await?;
    let mut actual = HashMap::with_capacity(blob_ids.len());
    for blob_id in blob_ids {
        let file_count = file_refs.get(blob_id).copied().unwrap_or(0);
        let revision_count = revision_refs.get(blob_id).copied().unwrap_or(0);
        let total = file_count.checked_add(revision_count).ok_or_else(|| {
            AsterError::internal_error("blob ref count overflow during integrity audit")
        })?;
        actual.insert(*blob_id, total);
    }
    Ok(actual)
}

pub async fn audit_blob_ref_counts<C: ConnectionTrait>(
    db: &C,
    policy_id: Option<i64>,
) -> Result<Vec<BlobRefCountDrift>> {
    Ok(audit_blob_ref_counts_with_limit(db, policy_id, usize::MAX)
        .await?
        .samples)
}

pub async fn audit_blob_ref_counts_with_limit<C: ConnectionTrait>(
    db: &C,
    policy_id: Option<i64>,
    sample_limit: usize,
) -> Result<AuditFindingSummary<BlobRefCountDrift>> {
    let mut drifts = AuditFindingSummary::new();
    let mut last_blob_id: Option<i64> = None;
    loop {
        let mut query = FileBlob::find()
            .order_by_asc(file_blob::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(last_blob_id_value) = last_blob_id {
            query = query.filter(file_blob::Column::Id.gt(last_blob_id_value));
        }
        if let Some(policy_id) = policy_id {
            query = query.filter(file_blob::Column::PolicyId.eq(policy_id));
        }

        let blobs = query
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if blobs.is_empty() {
            break;
        }
        last_blob_id = blobs.last().map(|blob| blob.id);

        let blob_ids = blobs.iter().map(|blob| blob.id).collect::<Vec<_>>();
        let actual_ref_counts = actual_blob_ref_counts_for_blobs(db, &blob_ids).await?;

        for blob in blobs {
            let actual_ref_count = actual_ref_counts.get(&blob.id).copied().unwrap_or(0);
            if i64::from(blob.ref_count) != actual_ref_count {
                drifts.record(
                    BlobRefCountDrift {
                        blob_id: blob.id,
                        policy_id: blob.policy_id,
                        storage_path: blob.storage_path,
                        recorded_ref_count: blob.ref_count,
                        actual_ref_count,
                    },
                    sample_limit,
                );
            }
        }
    }

    Ok(drifts)
}

pub async fn fix_blob_ref_count_drifts<C: ConnectionTrait>(
    db: &C,
    drifts: &[BlobRefCountDrift],
) -> Result<()> {
    for drift in drifts {
        let actual_ref_count = i32::try_from(drift.actual_ref_count).map_err(|_| {
            AsterError::internal_error(format!(
                "actual ref count overflow for blob {}",
                drift.blob_id
            ))
        })?;

        let result = FileBlob::update_many()
            .col_expr(file_blob::Column::RefCount, Expr::value(actual_ref_count))
            .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
            .filter(file_blob::Column::Id.eq(drift.blob_id))
            .exec(db)
            .await
            .map_aster_err(AsterError::database_operation)?;

        if result.rows_affected == 0 {
            return Err(AsterError::record_not_found(format!(
                "file_blob #{}",
                drift.blob_id
            )));
        }
    }

    Ok(())
}

pub async fn fix_blob_ref_counts_all<C: ConnectionTrait>(
    db: &C,
    policy_id: Option<i64>,
) -> Result<usize> {
    let mut fixed = 0;
    let mut last_blob_id = None;
    loop {
        let mut query = FileBlob::find()
            .order_by_asc(file_blob::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(id) = last_blob_id {
            query = query.filter(file_blob::Column::Id.gt(id));
        }
        if let Some(policy_id) = policy_id {
            query = query.filter(file_blob::Column::PolicyId.eq(policy_id));
        }
        let blobs = query
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if blobs.is_empty() {
            break;
        }
        last_blob_id = blobs.last().map(|blob| blob.id);
        let ids = blobs.iter().map(|blob| blob.id).collect::<Vec<_>>();
        let actual = actual_blob_ref_counts_for_blobs(db, &ids).await?;
        for blob in blobs {
            let actual_refs = actual.get(&blob.id).copied().unwrap_or(0);
            if i64::from(blob.ref_count) == actual_refs {
                continue;
            }
            let value = i32::try_from(actual_refs).map_err(|_| {
                AsterError::internal_error(format!(
                    "actual ref count overflow for blob {}",
                    blob.id
                ))
            })?;
            let result = FileBlob::update_many()
                .col_expr(file_blob::Column::RefCount, Expr::value(value))
                .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
                .filter(file_blob::Column::Id.eq(blob.id))
                .exec(db)
                .await
                .map_aster_err(AsterError::database_operation)?;
            if result.rows_affected == 0 {
                return Err(AsterError::record_not_found(format!(
                    "file_blob #{}",
                    blob.id
                )));
            }
            fixed += 1;
        }
    }
    Ok(fixed)
}

#[derive(Debug, Clone, Copy)]
enum FolderAuditScope {
    User(i64),
    Team(i64),
    Unowned,
}

#[cfg(test)]
#[expect(
    dead_code,
    reason = "legacy implementation retained for differential tests"
)]
async fn audit_folder_tree_partition<C: ConnectionTrait>(
    db: &C,
    scope: FolderAuditScope,
) -> Result<Vec<FolderTreeIssue>> {
    // 目录树审计只关心结构完整性，不关心 deleted_at；
    // 已删除节点如果 parent 指错、跨 scope 或形成环，一样会污染后续恢复/清理逻辑。
    let mut folder_by_id = HashMap::<i64, FolderNode>::new();
    let mut ordered_folder_ids = Vec::new();
    let mut last_folder_id: Option<i64> = None;
    loop {
        let mut query = Folder::find()
            .select_only()
            .column(folder::Column::Id)
            .column(folder::Column::ParentId)
            .column(folder::Column::OwnerUserId)
            .column(folder::Column::TeamId)
            .order_by_asc(folder::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(last_folder_id_value) = last_folder_id {
            query = query.filter(folder::Column::Id.gt(last_folder_id_value));
        }
        query = match scope {
            FolderAuditScope::User(user_id) => query
                .filter(folder::Column::OwnerUserId.eq(user_id))
                .filter(folder::Column::TeamId.is_null()),
            FolderAuditScope::Team(team_id) => query.filter(folder::Column::TeamId.eq(team_id)),
            FolderAuditScope::Unowned => query
                .filter(folder::Column::OwnerUserId.is_null())
                .filter(folder::Column::TeamId.is_null()),
        };

        let rows = query
            .into_tuple::<(i64, Option<i64>, Option<i64>, Option<i64>)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if rows.is_empty() {
            break;
        }
        last_folder_id = rows.last().map(|(id, _, _, _)| *id);

        for (id, parent_id, owner_user_id, team_id) in rows {
            ordered_folder_ids.push(id);
            folder_by_id.insert(
                id,
                FolderNode {
                    parent_id,
                    owner_user_id,
                    team_id,
                },
            );
        }
    }

    let mut issues = Vec::new();

    for &folder_id in &ordered_folder_ids {
        let Some(folder) = folder_by_id.get(&folder_id) else {
            tracing::warn!(folder_id, "folder missing from integrity audit map");
            continue;
        };
        if let Some(parent_id) = folder.parent_id {
            match folder_by_id.get(&parent_id) {
                Some(parent)
                    if parent.owner_user_id == folder.owner_user_id
                        && parent.team_id == folder.team_id => {}
                Some(parent) => issues.push(FolderTreeIssue {
                    kind: FolderTreeIssueKind::CrossScopeParent,
                    folder_id,
                    parent_id: Some(parent_id),
                    detail: format!(
                        "folder#{} points to parent#{} outside its workspace (folder owner/team={:?}/{:?}, parent owner/team={:?}/{:?})",
                        folder_id,
                        parent_id,
                        folder.owner_user_id,
                        folder.team_id,
                        parent.owner_user_id,
                        parent.team_id
                    ),
                }),
                None => {
                    let parent_exists = Folder::find_by_id(parent_id)
                        .one(db)
                        .await
                        .map_aster_err(AsterError::database_operation)?
                        .is_some();
                    issues.push(FolderTreeIssue {
                        kind: if parent_exists {
                            FolderTreeIssueKind::CrossScopeParent
                        } else {
                            FolderTreeIssueKind::MissingParent
                        },
                        folder_id,
                        parent_id: Some(parent_id),
                        detail: if parent_exists {
                            format!(
                                "folder#{} points to parent#{} outside its workspace",
                                folder_id, parent_id
                            )
                        } else {
                            format!(
                                "folder#{} points to missing parent#{}",
                                folder_id, parent_id
                            )
                        },
                    });
                }
            }
        }
    }

    let mut visited = HashSet::new();
    let mut reported_cycles = HashSet::<Vec<i64>>::new();
    for &folder_id in &ordered_folder_ids {
        if visited.contains(&folder_id) {
            continue;
        }

        let mut path = Vec::new();
        let mut path_index = HashMap::<i64, usize>::new();
        let mut current_id = Some(folder_id);

        while let Some(id) = current_id {
            if let Some(&cycle_start) = path_index.get(&id) {
                let cycle = path[cycle_start..].to_vec();
                let mut normalized = cycle.clone();
                normalized.sort_unstable();
                if reported_cycles.insert(normalized) {
                    let cycle_path = cycle
                        .iter()
                        .map(|id| id.to_string())
                        .chain(std::iter::once(cycle[0].to_string()))
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    issues.push(FolderTreeIssue {
                        kind: FolderTreeIssueKind::Cycle,
                        folder_id: cycle[0],
                        parent_id: folder_by_id
                            .get(&cycle[0])
                            .and_then(|folder| folder.parent_id),
                        detail: format!("folder cycle detected: {cycle_path}"),
                    });
                }
                break;
            }

            let Some(current) = folder_by_id.get(&id) else {
                break;
            };
            path_index.insert(id, path.len());
            path.push(id);

            current_id = match current.parent_id {
                Some(parent_id) => match folder_by_id.get(&parent_id) {
                    Some(parent)
                        if parent.owner_user_id == current.owner_user_id
                            && parent.team_id == current.team_id =>
                    {
                        Some(parent_id)
                    }
                    _ => None,
                },
                None => None,
            };
        }

        for id in path {
            visited.insert(id);
        }
    }

    Ok(issues)
}

fn folder_in_scope(scope: FolderAuditScope, node: &FolderNode) -> bool {
    match scope {
        FolderAuditScope::User(user_id) => {
            node.owner_user_id == Some(user_id) && node.team_id.is_none()
        }
        FolderAuditScope::Team(team_id) => node.team_id == Some(team_id),
        FolderAuditScope::Unowned => node.owner_user_id.is_none() && node.team_id.is_none(),
    }
}

async fn load_folder_node<C: ConnectionTrait>(db: &C, id: i64) -> Result<Option<FolderNode>> {
    Folder::find_by_id(id)
        .select_only()
        .column(folder::Column::ParentId)
        .column(folder::Column::OwnerUserId)
        .column(folder::Column::TeamId)
        .into_tuple::<(Option<i64>, Option<i64>, Option<i64>)>()
        .one(db)
        .await
        .map_aster_err(AsterError::database_operation)
        .map(|node| {
            node.map(|(parent_id, owner_user_id, team_id)| FolderNode {
                parent_id,
                owner_user_id,
                team_id,
            })
        })
}

async fn next_folder_parent<C: ConnectionTrait>(
    db: &C,
    id: i64,
    scope: FolderAuditScope,
) -> Result<Option<i64>> {
    Ok(load_folder_node(db, id)
        .await?
        .filter(|node| folder_in_scope(scope, node))
        .and_then(|node| node.parent_id))
}

async fn find_folder_cycle_representative<C: ConnectionTrait>(
    db: &C,
    start: i64,
    scope: FolderAuditScope,
) -> Result<Option<i64>> {
    let mut slow = Some(start);
    let mut fast = Some(start);
    let meeting = loop {
        slow = match slow {
            Some(id) => next_folder_parent(db, id, scope).await?,
            None => return Ok(None),
        };
        fast = match fast {
            Some(id) => next_folder_parent(db, id, scope).await?,
            None => return Ok(None),
        };
        fast = match fast {
            Some(id) => next_folder_parent(db, id, scope).await?,
            None => return Ok(None),
        };
        if let (Some(slow_id), Some(fast_id)) = (slow, fast)
            && slow_id == fast_id
        {
            break slow_id;
        }
    };

    let mut representative = meeting;
    let mut current = next_folder_parent(db, meeting, scope).await?;
    while let Some(id) = current {
        representative = std::cmp::min(representative, id);
        if id == meeting {
            break;
        }
        current = next_folder_parent(db, id, scope).await?;
    }
    Ok(Some(representative))
}

async fn audit_folder_tree_partition_bounded<C: ConnectionTrait>(
    db: &C,
    scope: FolderAuditScope,
    sample_limit: usize,
) -> Result<AuditFindingSummary<FolderTreeIssue>> {
    let mut issues = AuditFindingSummary::new();
    let mut reported_cycles = HashSet::<i64>::new();
    let mut last_folder_id = None;
    loop {
        let mut query = Folder::find()
            .select_only()
            .column(folder::Column::Id)
            .column(folder::Column::ParentId)
            .column(folder::Column::OwnerUserId)
            .column(folder::Column::TeamId)
            .order_by_asc(folder::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(id) = last_folder_id {
            query = query.filter(folder::Column::Id.gt(id));
        }
        query = match scope {
            FolderAuditScope::User(user_id) => query
                .filter(folder::Column::OwnerUserId.eq(user_id))
                .filter(folder::Column::TeamId.is_null()),
            FolderAuditScope::Team(team_id) => query.filter(folder::Column::TeamId.eq(team_id)),
            FolderAuditScope::Unowned => query
                .filter(folder::Column::OwnerUserId.is_null())
                .filter(folder::Column::TeamId.is_null()),
        };
        let rows = query
            .into_tuple::<(i64, Option<i64>, Option<i64>, Option<i64>)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if rows.is_empty() {
            break;
        }
        last_folder_id = rows.last().map(|(id, _, _, _)| *id);
        for (folder_id, parent_id, owner_user_id, team_id) in rows {
            let Some(parent_id) = parent_id else {
                continue;
            };
            let parent = load_folder_node(db, parent_id).await?;
            match parent {
                None => issues.record(FolderTreeIssue {
                    kind: FolderTreeIssueKind::MissingParent,
                    folder_id,
                    parent_id: Some(parent_id),
                    detail: format!("folder#{} points to missing parent#{}", folder_id, parent_id),
                }, sample_limit),
                Some(parent) if !folder_in_scope(scope, &parent) => issues.record(FolderTreeIssue {
                    kind: FolderTreeIssueKind::CrossScopeParent,
                    folder_id,
                    parent_id: Some(parent_id),
                    detail: format!(
                        "folder#{} points to parent#{} outside its workspace (folder owner/team={:?}/{:?}, parent owner/team={:?}/{:?})",
                        folder_id,
                        parent_id,
                        owner_user_id,
                        team_id,
                        parent.owner_user_id,
                        parent.team_id
                    ),
                }, sample_limit),
                Some(_) => {}
            }
            if let Some(representative) =
                find_folder_cycle_representative(db, folder_id, scope).await?
                && reported_cycles.insert(representative)
            {
                issues.record(
                    FolderTreeIssue {
                        kind: FolderTreeIssueKind::Cycle,
                        folder_id: representative,
                        parent_id: load_folder_node(db, representative)
                            .await?
                            .and_then(|node| node.parent_id),
                        detail: format!(
                            "folder cycle detected involving folder#{}",
                            representative
                        ),
                    },
                    sample_limit,
                );
            }
        }
    }
    Ok(issues)
}

pub async fn audit_folder_tree<C: ConnectionTrait>(db: &C) -> Result<Vec<FolderTreeIssue>> {
    Ok(audit_folder_tree_with_limit(db, usize::MAX).await?.samples)
}

pub async fn audit_folder_tree_with_limit<C: ConnectionTrait>(
    db: &C,
    sample_limit: usize,
) -> Result<AuditFindingSummary<FolderTreeIssue>> {
    let mut issues = AuditFindingSummary::new();
    let mut last_user_id = None;
    loop {
        let mut query = Folder::find()
            .select_only()
            .column(folder::Column::OwnerUserId)
            .distinct()
            .order_by_asc(folder::Column::OwnerUserId)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(id) = last_user_id {
            query = query.filter(folder::Column::OwnerUserId.gt(id));
        }
        let rows = query
            .filter(folder::Column::OwnerUserId.is_not_null())
            .filter(folder::Column::TeamId.is_null())
            .into_tuple::<Option<i64>>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if rows.is_empty() {
            break;
        }
        last_user_id = rows.last().copied().flatten();
        for user_id in rows.into_iter().flatten() {
            let partition = audit_folder_tree_partition_bounded(
                db,
                FolderAuditScope::User(user_id),
                sample_limit,
            )
            .await?;
            issues.total += partition.total;
            issues.samples.extend(
                partition
                    .samples
                    .into_iter()
                    .take(sample_limit.saturating_sub(issues.samples.len())),
            );
        }
    }

    let mut last_team_id = None;
    loop {
        let mut query = Folder::find()
            .select_only()
            .column(folder::Column::TeamId)
            .distinct()
            .order_by_asc(folder::Column::TeamId)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(id) = last_team_id {
            query = query.filter(folder::Column::TeamId.gt(id));
        }
        let rows = query
            .filter(folder::Column::TeamId.is_not_null())
            .into_tuple::<Option<i64>>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if rows.is_empty() {
            break;
        }
        last_team_id = rows.last().copied().flatten();
        for team_id in rows.into_iter().flatten() {
            let partition = audit_folder_tree_partition_bounded(
                db,
                FolderAuditScope::Team(team_id),
                sample_limit,
            )
            .await?;
            issues.total += partition.total;
            issues.samples.extend(
                partition
                    .samples
                    .into_iter()
                    .take(sample_limit.saturating_sub(issues.samples.len())),
            );
        }
    }

    let partition =
        audit_folder_tree_partition_bounded(db, FolderAuditScope::Unowned, sample_limit).await?;
    issues.total += partition.total;
    issues.samples.extend(
        partition
            .samples
            .into_iter()
            .take(sample_limit.saturating_sub(issues.samples.len())),
    );
    Ok(issues)
}

async fn audit_missing_blob_objects<C: ConnectionTrait>(
    db: &C,
    driver: &dyn aster_drive_storage::StorageDriver,
    policy_id: i64,
    report: &mut StorageObjectAudit,
    finding_limit: Option<usize>,
    sink: &mut dyn StorageObjectFindingSink,
) -> Result<()> {
    let mut last_blob_id = None;
    loop {
        let mut query = FileBlob::find()
            .filter(file_blob::Column::PolicyId.eq(policy_id))
            .order_by_asc(file_blob::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(id) = last_blob_id {
            query = query.filter(file_blob::Column::Id.gt(id));
        }
        let blobs = query
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if blobs.is_empty() {
            break;
        }
        last_blob_id = blobs.last().map(|blob| blob.id);
        for blob in blobs {
            report.scanned_blob_records += 1;
            let Some(path) = blob.storage_path_for_connector() else {
                continue;
            };
            if !driver.exists(path).await.map_err(AsterError::from)? {
                let finding = BlobObjectIssue {
                    policy_id,
                    path: path.to_string(),
                    blob_id: Some(blob.id),
                };
                sink.record(StorageObjectFinding::MissingBlob(finding.clone()))?;
                record_finding(
                    &mut report.missing_blob_objects,
                    &mut report.missing_blob_objects_total,
                    finding,
                    finding_limit,
                );
            }
        }
    }
    Ok(())
}

pub async fn audit_storage_objects<C: ConnectionTrait>(
    db: &C,
    driver_registry: &DriverRegistry,
    policy_id: Option<i64>,
    thumbnail_max_dimension: u32,
    image_preview_max_dimension: u32,
) -> Result<StorageObjectAudit> {
    audit_storage_objects_with_finding_limit(
        db,
        driver_registry,
        policy_id,
        thumbnail_max_dimension,
        image_preview_max_dimension,
        None,
    )
    .await
}

/// Audits storage objects while retaining at most `finding_limit` examples of
/// each finding kind in memory. Totals remain exact for paginated/reporting use.
pub async fn audit_storage_objects_with_finding_limit<C: ConnectionTrait>(
    db: &C,
    driver_registry: &DriverRegistry,
    policy_id: Option<i64>,
    thumbnail_max_dimension: u32,
    image_preview_max_dimension: u32,
    finding_limit: Option<usize>,
) -> Result<StorageObjectAudit> {
    let mut sink = NoopStorageObjectFindingSink;
    audit_storage_objects_impl(
        db,
        driver_registry,
        policy_id,
        thumbnail_max_dimension,
        image_preview_max_dimension,
        finding_limit,
        &mut sink,
    )
    .await
}

pub async fn audit_storage_objects_with_sink<C: ConnectionTrait>(
    db: &C,
    driver_registry: &DriverRegistry,
    policy_id: Option<i64>,
    thumbnail_max_dimension: u32,
    image_preview_max_dimension: u32,
    sink: &mut dyn StorageObjectFindingSink,
) -> Result<StorageObjectAudit> {
    audit_storage_objects_impl(
        db,
        driver_registry,
        policy_id,
        thumbnail_max_dimension,
        image_preview_max_dimension,
        Some(0),
        sink,
    )
    .await
}

async fn audit_storage_objects_impl<C: ConnectionTrait>(
    db: &C,
    driver_registry: &DriverRegistry,
    policy_id: Option<i64>,
    thumbnail_max_dimension: u32,
    image_preview_max_dimension: u32,
    finding_limit: Option<usize>,
    sink: &mut dyn StorageObjectFindingSink,
) -> Result<StorageObjectAudit> {
    let mut policies = crate::db::repository::policy_repo::find_all(db).await?;
    if let Some(policy_id) = policy_id {
        policies.retain(|policy| policy.id == policy_id);
    }

    let mut report = StorageObjectAudit {
        scanned_policies: policies.len(),
        ..Default::default()
    };

    for policy in policies {
        let driver = driver_registry.get_driver(&policy)?;
        audit_missing_blob_objects(
            db,
            driver.as_ref(),
            policy.id,
            &mut report,
            finding_limit,
            sink,
        )
        .await?;
        {
            let mut visitor = StorageAuditVisitor {
                db,
                policy_id: policy.id,
                thumbnail_max_dimension,
                image_preview_max_dimension,
                finding_limit,
                report: &mut report,
                pending_paths: Vec::with_capacity(STORAGE_AUDIT_PATH_BATCH_SIZE),
                sink,
            };
            if let Some(list_driver) = driver.extensions().list {
                list_driver.scan_paths(None, &mut visitor).await?;
            }
            visitor.flush().await.map_err(AsterError::from)?;
        }
        report.completed_policies += 1;
    }

    report.findings_truncated = report.missing_blob_objects.len()
        < report.missing_blob_objects_total
        || report.untracked_objects.len() < report.untracked_objects_total
        || report.orphan_thumbnails.len() < report.orphan_thumbnails_total;

    Ok(report)
}
