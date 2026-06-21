//! 跨数据库 / 跨存储的一致性审计。
//!
//! 这些检查都不是在线请求路径上的业务逻辑，而是偏运维的“全局事实核对”：
//! 例如配额计数漂移、blob 引用计数漂移、对象存储孤儿文件和目录树损坏。

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait, JoinType, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait, sea_query::Expr,
};
use serde::Serialize;

use crate::db::repository::{team_repo, upload_session_repo, user_repo};
use crate::entities::{
    file::{self, Entity as File},
    file_blob::{self, Entity as FileBlob},
    file_version::{self, Entity as FileVersion},
    folder::{self, Entity as Folder},
};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::services::{media_processing_service, thumbnail_service};
use crate::storage::{DriverRegistry, StoragePathVisitor};

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
    pub storage_path: String,
    pub recorded_ref_count: i32,
    pub actual_ref_count: i64,
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
    pub scanned_objects: usize,
    pub ignored_paths: usize,
    pub missing_blob_objects: Vec<BlobObjectIssue>,
    pub untracked_objects: Vec<BlobObjectIssue>,
    pub orphan_thumbnails: Vec<ThumbnailIssue>,
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

struct StorageAuditVisitor<'a> {
    policy_id: i64,
    tracked_blobs: &'a mut HashSet<String>,
    tracked_thumbnails: &'a HashSet<String>,
    tracked_temp_paths: &'a HashSet<String>,
    report: &'a mut StorageObjectAudit,
}

impl StoragePathVisitor for StorageAuditVisitor<'_> {
    fn visit_path(&mut self, path: String) -> Result<()> {
        self.report.scanned_objects += 1;

        // 对象扫描的目标不是“列出全部文件”，而是把存储中的路径分流成三类：
        // 1. 已被数据库跟踪的 blob / thumbnail / 临时对象；
        // 2. 数据库预期存在但存储里缺失的对象（后续在遍历结束后得出）；
        // 3. 存储里存在但数据库没在追踪的孤儿对象。
        if path.starts_with(".staging/") {
            self.report.ignored_paths += 1;
            return Ok(());
        }
        if self.tracked_blobs.remove(&path)
            || self.tracked_thumbnails.contains(&path)
            || self.tracked_temp_paths.contains(&path)
        {
            return Ok(());
        }
        if thumbnail_service::is_thumbnail_path(&path) {
            self.report.orphan_thumbnails.push(ThumbnailIssue {
                policy_id: self.policy_id,
                path,
            });
            return Ok(());
        }

        self.report.untracked_objects.push(BlobObjectIssue {
            policy_id: self.policy_id,
            path,
            blob_id: None,
        });
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

async fn load_actual_storage_usage<C: ConnectionTrait>(
    db: &C,
) -> Result<HashMap<StorageOwner, i64>> {
    // `storage_used` 的真实值来自“当前文件 + 历史版本”的总占用，而不是某一张表。
    // 这里分两段批量扫描，避免把全量 file / version 记录一次性装进内存。
    let mut totals = HashMap::new();
    let mut last_file_id: Option<i64> = None;
    loop {
        let mut query = File::find()
            .select_only()
            .column(file::Column::Id)
            .column(file::Column::Size)
            .column(file::Column::OwnerUserId)
            .column(file::Column::TeamId)
            .order_by_asc(file::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(last_file_id_value) = last_file_id {
            query = query.filter(file::Column::Id.gt(last_file_id_value));
        }

        let rows = query
            .into_tuple::<(i64, i64, Option<i64>, Option<i64>)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if rows.is_empty() {
            break;
        }
        last_file_id = rows.last().map(|(id, _, _, _)| *id);

        for (file_id, size, owner_user_id, team_id) in rows {
            let owner = match team_id {
                Some(team_id) => StorageOwner::Team(team_id),
                None => {
                    let Some(owner_user_id) = owner_user_id else {
                        tracing::warn!(
                            file_id,
                            "skipping personal file without owner_user_id during storage usage audit"
                        );
                        continue;
                    };
                    StorageOwner::User(owner_user_id)
                }
            };
            add_usage(&mut totals, owner, size)?;
        }
    }

    let mut last_version_id: Option<i64> = None;
    loop {
        let mut query = FileVersion::find()
            .join(JoinType::InnerJoin, file_version::Relation::File.def())
            .select_only()
            .column(file_version::Column::Id)
            .column(file_version::Column::Size)
            .column(file::Column::OwnerUserId)
            .column(file::Column::TeamId)
            .order_by_asc(file_version::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(last_version_id_value) = last_version_id {
            query = query.filter(file_version::Column::Id.gt(last_version_id_value));
        }

        let rows = query
            .into_tuple::<(i64, i64, Option<i64>, Option<i64>)>()
            .all(db)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if rows.is_empty() {
            break;
        }
        last_version_id = rows.last().map(|(id, _, _, _)| *id);

        for (version_id, size, owner_user_id, team_id) in rows {
            let owner = match team_id {
                Some(team_id) => StorageOwner::Team(team_id),
                None => {
                    let Some(owner_user_id) = owner_user_id else {
                        tracing::warn!(
                            version_id,
                            "skipping personal file version without owner_user_id during storage usage audit"
                        );
                        continue;
                    };
                    StorageOwner::User(owner_user_id)
                }
            };
            add_usage(&mut totals, owner, size)?;
        }
    }

    Ok(totals)
}

pub async fn audit_storage_usage<C: ConnectionTrait>(db: &C) -> Result<Vec<StorageUsageDrift>> {
    let users = user_repo::find_all(db).await?;
    let teams = team_repo::find_all(db).await?;
    let actual_usage = load_actual_storage_usage(db).await?;
    let mut drifts = Vec::new();

    for model in users {
        let actual_bytes = actual_usage
            .get(&StorageOwner::User(model.id))
            .copied()
            .unwrap_or(0);
        if model.storage_used != actual_bytes {
            drifts.push(StorageUsageDrift {
                owner_kind: StorageOwnerKind::User,
                owner_id: model.id,
                recorded_bytes: model.storage_used,
                actual_bytes,
                delta_bytes: actual_bytes - model.storage_used,
            });
        }
    }

    for model in teams {
        let actual_bytes = actual_usage
            .get(&StorageOwner::Team(model.id))
            .copied()
            .unwrap_or(0);
        if model.storage_used != actual_bytes {
            drifts.push(StorageUsageDrift {
                owner_kind: StorageOwnerKind::Team,
                owner_id: model.id,
                recorded_bytes: model.storage_used,
                actual_bytes,
                delta_bytes: actual_bytes - model.storage_used,
            });
        }
    }

    Ok(drifts)
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

async fn load_actual_blob_ref_counts<C: ConnectionTrait>(
    db: &C,
    policy_id: Option<i64>,
) -> Result<HashMap<i64, i64>> {
    // blob 的实际引用数来自 files 和 file_versions 两边的总和。
    // 先聚合出“理论真值”，再和 file_blobs.ref_count 做逐行比较。
    let mut actual = HashMap::new();

    let mut file_refs_query = File::find()
        .select_only()
        .column(file::Column::BlobId)
        .column_as(
            Expr::col((file::Entity, file::Column::Id)).count(),
            "ref_count",
        )
        .group_by(file::Column::BlobId);
    if let Some(policy_id) = policy_id {
        file_refs_query = file_refs_query
            .join(JoinType::InnerJoin, file::Relation::FileBlob.def())
            .filter(file_blob::Column::PolicyId.eq(policy_id));
    }
    let file_refs = file_refs_query
        .into_tuple::<(i64, i64)>()
        .all(db)
        .await
        .map_aster_err(AsterError::database_operation)?;

    for (blob_id, ref_count) in file_refs {
        *actual.entry(blob_id).or_insert(0) += ref_count;
    }

    let mut version_refs_query = FileVersion::find()
        .select_only()
        .column(file_version::Column::BlobId)
        .column_as(
            Expr::col((file_version::Entity, file_version::Column::Id)).count(),
            "ref_count",
        )
        .group_by(file_version::Column::BlobId);
    if let Some(policy_id) = policy_id {
        version_refs_query = version_refs_query
            .join(JoinType::InnerJoin, file_version::Relation::FileBlob.def())
            .filter(file_blob::Column::PolicyId.eq(policy_id));
    }
    let version_refs = version_refs_query
        .into_tuple::<(i64, i64)>()
        .all(db)
        .await
        .map_aster_err(AsterError::database_operation)?;

    for (blob_id, ref_count) in version_refs {
        *actual.entry(blob_id).or_insert(0) += ref_count;
    }

    Ok(actual)
}

pub async fn audit_blob_ref_counts<C: ConnectionTrait>(
    db: &C,
    policy_id: Option<i64>,
) -> Result<Vec<BlobRefCountDrift>> {
    let actual_ref_counts = load_actual_blob_ref_counts(db, policy_id).await?;
    let mut drifts = Vec::new();
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

        for blob in blobs {
            let actual_ref_count = actual_ref_counts.get(&blob.id).copied().unwrap_or(0);
            if i64::from(blob.ref_count) != actual_ref_count {
                drifts.push(BlobRefCountDrift {
                    blob_id: blob.id,
                    policy_id: blob.policy_id,
                    storage_path: blob.storage_path,
                    recorded_ref_count: blob.ref_count,
                    actual_ref_count,
                });
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

pub async fn audit_folder_tree<C: ConnectionTrait>(db: &C) -> Result<Vec<FolderTreeIssue>> {
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
                None => issues.push(FolderTreeIssue {
                    kind: FolderTreeIssueKind::MissingParent,
                    folder_id,
                    parent_id: Some(parent_id),
                    detail: format!("folder#{} points to missing parent#{}", folder_id, parent_id),
                }),
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

async fn load_blob_expectations_for_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
    thumbnail_max_dimension: u32,
    image_preview_max_dimension: u32,
) -> Result<(HashMap<String, i64>, HashSet<String>, HashSet<String>)> {
    let mut blob_ids_by_path = HashMap::<String, i64>::new();
    let mut tracked_blobs = HashSet::<String>::new();
    let mut tracked_thumbnails = HashSet::<String>::new();
    let mut last_blob_id: Option<i64> = None;

    loop {
        let mut query = FileBlob::find()
            .filter(file_blob::Column::PolicyId.eq(policy_id))
            .order_by_asc(file_blob::Column::Id)
            .limit(INTEGRITY_BATCH_SIZE);
        if let Some(last_blob_id_value) = last_blob_id {
            query = query.filter(file_blob::Column::Id.gt(last_blob_id_value));
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
            blob_ids_by_path.insert(blob.storage_path.clone(), blob.id);
            tracked_blobs.insert(blob.storage_path.clone());
            tracked_thumbnails.extend(media_processing_service::known_thumbnail_cache_paths(
                &blob.hash,
                thumbnail_max_dimension,
            ));
            tracked_thumbnails.extend(media_processing_service::known_image_preview_cache_paths(
                &blob.hash,
                image_preview_max_dimension,
            ));
        }
    }

    Ok((blob_ids_by_path, tracked_blobs, tracked_thumbnails))
}

async fn load_temp_paths_for_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<HashSet<String>> {
    Ok(upload_session_repo::list_temp_keys_by_policy(db, policy_id)
        .await?
        .into_iter()
        .collect())
}

pub async fn audit_storage_objects<C: ConnectionTrait>(
    db: &C,
    driver_registry: &DriverRegistry,
    policy_id: Option<i64>,
    thumbnail_max_dimension: u32,
    image_preview_max_dimension: u32,
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
        let (blob_ids_by_path, mut tracked_blobs, tracked_thumbnails) =
            load_blob_expectations_for_policy(
                db,
                policy.id,
                thumbnail_max_dimension,
                image_preview_max_dimension,
            )
            .await?;
        let tracked_temp_paths = load_temp_paths_for_policy(db, policy.id).await?;
        let driver = driver_registry.get_driver(&policy)?;
        {
            let mut visitor = StorageAuditVisitor {
                policy_id: policy.id,
                tracked_blobs: &mut tracked_blobs,
                tracked_thumbnails: &tracked_thumbnails,
                tracked_temp_paths: &tracked_temp_paths,
                report: &mut report,
            };
            if let Some(list_driver) = driver.as_list() {
                list_driver.scan_paths(None, &mut visitor).await?;
            }
        }

        for path in tracked_blobs {
            let blob_id = blob_ids_by_path.get(&path).copied();
            report.missing_blob_objects.push(BlobObjectIssue {
                policy_id: policy.id,
                path,
                blob_id,
            });
        }
    }

    Ok(report)
}
