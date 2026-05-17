//! 文件服务子模块：`metadata`。

use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};

use crate::db::repository::file_repo;
use crate::entities::file;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    storage_change_service,
    workspace_models::FileInfo,
    workspace_storage_service::{self, WorkspaceStorageScope},
};
use crate::types::NullablePatch;

pub(crate) async fn get_info_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    id: i64,
) -> Result<file::Model> {
    workspace_storage_service::verify_file_access(state, scope, id).await
}

pub(crate) async fn update_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    id: i64,
    name: Option<String>,
    folder_id: NullablePatch<i64>,
) -> Result<file::Model> {
    let db = &state.db;
    tracing::debug!(
        scope = ?scope,
        file_id = id,
        target_name = name.as_deref().unwrap_or(""),
        folder_patch = ?folder_id,
        "updating file metadata"
    );
    let f = get_info_in_scope(state, scope, id).await?;
    if f.is_locked {
        return Err(AsterError::resource_locked("file is locked"));
    }

    let target_folder = match folder_id {
        NullablePatch::Absent => f.folder_id,
        NullablePatch::Null => None,
        NullablePatch::Value(fid) => Some(fid),
    };
    if let NullablePatch::Value(fid) = folder_id {
        workspace_storage_service::verify_folder_access(state, scope, fid).await?;
    }

    let name = match name {
        Some(name) => Some(crate::utils::normalize_validate_name(&name)?),
        None => None,
    };

    let final_name = name.clone().unwrap_or_else(|| f.name.clone());
    let existing = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_by_name_in_folder(db, user_id, target_folder, &final_name).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_by_name_in_team_folder(db, team_id, target_folder, &final_name).await?
        }
    };
    if let Some(existing) = existing
        && existing.id != id
    {
        return Err(file_repo::duplicate_name_error(&final_name));
    }

    let previous_folder_id = f.folder_id;
    let mime_type = f.mime_type.clone();
    let mut active: file::ActiveModel = f.into();
    if let Some(n) = name {
        let classification = crate::utils::file_classification::classify_file(&n, &mime_type);
        active.name = Set(n);
        active.extension = Set(classification.extension);
        active.compound_extension = Set(classification.compound_extension);
        active.file_category = Set(classification.category);
    }
    match folder_id {
        NullablePatch::Absent => {}
        NullablePatch::Null => active.folder_id = Set(None),
        NullablePatch::Value(fid) => active.folder_id = Set(Some(fid)),
    }
    active.updated_at = Set(Utc::now());
    let updated = active
        .update(db)
        .await
        .map_err(|err| file_repo::map_name_db_err(err, &final_name))?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FileUpdated,
            scope,
            vec![updated.id],
            vec![],
            vec![previous_folder_id, updated.folder_id],
        ),
    );
    tracing::debug!(
        scope = ?scope,
        file_id = updated.id,
        folder_id = updated.folder_id,
        name = %updated.name,
        "updated file metadata"
    );
    Ok(updated)
}

/// 获取文件信息
pub async fn get_info(state: &PrimaryAppState, id: i64, user_id: i64) -> Result<FileInfo> {
    get_info_in_scope(state, WorkspaceStorageScope::Personal { user_id }, id)
        .await
        .map(Into::into)
}

/// 更新文件（重命名/移动）
pub async fn update(
    state: &PrimaryAppState,
    id: i64,
    user_id: i64,
    name: Option<String>,
    folder_id: NullablePatch<i64>,
) -> Result<FileInfo> {
    update_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        id,
        name,
        folder_id,
    )
    .await
    .map(Into::into)
}

/// 移动文件到指定文件夹（None = 根目录）
///
/// 与 `update()` 的区别：`update()` 用 `NullablePatch<i64>` 区分
/// “未传字段”和“显式传 null”，而本函数的 `target_folder_id: None`
/// 明确表示“移到根目录”。
pub async fn move_file(
    state: &PrimaryAppState,
    id: i64,
    user_id: i64,
    target_folder_id: Option<i64>,
) -> Result<FileInfo> {
    update_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        id,
        None,
        match target_folder_id {
            Some(folder_id) => NullablePatch::Value(folder_id),
            None => NullablePatch::Null,
        },
    )
    .await
    .map(Into::into)
}
