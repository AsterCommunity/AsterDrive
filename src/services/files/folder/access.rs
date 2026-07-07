//! 文件夹服务子模块：`access`。

use crate::db::repository::folder_repo;
use crate::entities::folder;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::workspace_storage_service::{self, WorkspaceStorageScope};

pub(crate) fn ensure_folder_model_in_scope(
    folder: &folder::Model,
    scope: WorkspaceStorageScope,
) -> Result<()> {
    workspace_storage_service::ensure_active_folder_scope(folder, scope)
}

pub async fn verify_folder_in_scope(
    db: &sea_orm::DatabaseConnection,
    folder_id: i64,
    root_folder_id: i64,
) -> Result<()> {
    if folder_id == root_folder_id {
        return Ok(());
    }

    let chain_map = super::hierarchy::load_folder_chain_map(db, &[folder_id]).await?;
    let mut current_id = Some(folder_id);
    while let Some(id) = current_id {
        let folder = chain_map
            .get(&id)
            .ok_or_else(|| AsterError::record_not_found(format!("folder #{id}")))?;
        if folder.parent_id == Some(root_folder_id) {
            return Ok(());
        }
        current_id = folder.parent_id;
    }

    Err(AsterError::auth_forbidden(
        "folder is outside shared folder scope",
    ))
}

pub(crate) fn ensure_personal_folder_scope(folder: &folder::Model) -> Result<()> {
    if folder.team_id.is_some() {
        return Err(AsterError::auth_forbidden(
            "folder belongs to a team workspace",
        ));
    }
    Ok(())
}

/// 校验目标文件夹存在、归属当前用户且未被删除
pub async fn verify_folder_access(
    state: &impl SharedRuntimeState,
    user_id: i64,
    folder_id: i64,
) -> Result<()> {
    let folder = folder_repo::find_by_id(state.writer_db(), folder_id).await?;
    ensure_personal_folder_scope(&folder)?;
    crate::utils::verify_optional_owner(folder.owner_user_id, user_id, "folder")?;
    if folder.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "folder #{folder_id} is in trash"
        )));
    }
    Ok(())
}
