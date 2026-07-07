//! 分享服务内部共用的边界校验。
//!
//! 这层负责回答几个核心问题：
//! - 某条 share 是否仍然属于当前工作空间
//! - 公开 token 是否仍有效
//! - 被访问的 file / folder 是否仍在分享声明的范围内

use std::collections::HashMap;

use chrono::Utc;
use sea_orm::DatabaseConnection;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{file_repo, folder_repo, share_repo, team_repo};
use crate::entities::share;
use crate::errors::{AsterError, Result, auth_forbidden_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::{
    files::{file, folder},
    workspace_storage_service::{self, WorkspaceStorageScope},
};
use crate::types::EntityType;

use super::{
    cache::load_share_record_by_token,
    models::{ShareStatus, ShareTarget, share_target_for_share},
};

pub(super) fn validate_max_downloads(max_downloads: i64) -> Result<()> {
    if max_downloads < 0 {
        return Err(AsterError::validation_error(
            "max_downloads cannot be negative",
        ));
    }
    Ok(())
}

fn ensure_share_scope(share: &share::Model, scope: WorkspaceStorageScope) -> Result<()> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            if share.team_id.is_some() {
                return Err(auth_forbidden_with_code(
                    ApiErrorCode::ShareScopeDenied,
                    "share belongs to a team workspace",
                ));
            }
            crate::utils::verify_owner(share.user_id, user_id, "share")?;
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            if share.team_id != Some(team_id) {
                return Err(auth_forbidden_with_code(
                    ApiErrorCode::ShareScopeDenied,
                    "share is outside team workspace",
                ));
            }
        }
    }

    Ok(())
}

pub(super) async fn lock_share_resource_in_scope<C: sea_orm::ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    file_id: Option<i64>,
    folder_id: Option<i64>,
) -> Result<()> {
    // 创建分享前先锁目标资源，避免并发请求同时通过“当前没有活跃分享”的检查，
    // 最终写出多条针对同一资源的活跃 share。
    if let Some(file_id) = file_id {
        let file = file_repo::lock_by_id(db, file_id).await?;
        workspace_storage_service::ensure_active_file_scope(&file, scope)?;
    }

    if let Some(folder_id) = folder_id {
        let folder = folder_repo::lock_by_id(db, folder_id).await?;
        workspace_storage_service::ensure_active_folder_scope(&folder, scope)?;
    }

    Ok(())
}

pub(super) async fn load_share_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    share_id: i64,
) -> Result<share::Model> {
    workspace_storage_service::require_scope_access(state, scope).await?;
    let share = share_repo::find_by_id(state.writer_db(), share_id).await?;
    ensure_share_scope(&share, scope)?;
    Ok(share)
}

pub(super) async fn load_valid_share(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<share::Model> {
    let share = load_share_record(state, token).await?;
    validate_share(&share)?;
    Ok(share)
}

pub(crate) async fn load_usable_share_ignoring_download_limit(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<share::Model> {
    let share = load_share_record(state, token).await?;
    validate_share_without_download_limit(&share)?;
    Ok(share)
}

pub(super) async fn load_share_record(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<share::Model> {
    let share = load_share_record_by_token(state, token).await?;
    // 团队分享如果指向的团队已被归档 / 删除，对外表现应当像 share 不存在，
    // 不再向匿名访问者暴露“token 有效但团队没了”这种内部状态。
    if let Some(team_id) = share.team_id {
        match team_repo::find_active_by_id(state.reader_db(), team_id).await {
            Ok(_) => {}
            Err(AsterError::RecordNotFound(_)) => {
                return Err(AsterError::share_not_found(format!("token={token}")));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(share)
}

pub(super) fn ensure_share_matches_file(
    share: &share::Model,
    file: &crate::entities::file::Model,
) -> Result<()> {
    if let Some(team_id) = share.team_id {
        if file.team_id != Some(team_id) {
            return Err(auth_forbidden_with_code(
                ApiErrorCode::ShareScopeDenied,
                "file is outside shared scope",
            ));
        }
    } else {
        file::ensure_personal_file_scope(file)?;
        crate::utils::verify_optional_owner(file.owner_user_id, share.user_id, "file")?;
    }
    Ok(())
}

pub(super) fn ensure_share_matches_folder(
    share: &share::Model,
    folder: &crate::entities::folder::Model,
) -> Result<()> {
    if let Some(team_id) = share.team_id {
        if folder.team_id != Some(team_id) {
            return Err(auth_forbidden_with_code(
                ApiErrorCode::ShareScopeDenied,
                "folder is outside shared scope",
            ));
        }
    } else {
        folder::ensure_personal_folder_scope(folder)?;
        crate::utils::verify_optional_owner(folder.owner_user_id, share.user_id, "folder")?;
    }
    Ok(())
}

pub(super) async fn load_share_file_resource(
    state: &impl SharedRuntimeState,
    share: &share::Model,
) -> Result<crate::entities::file::Model> {
    let file_id = match share_target_for_share(share)? {
        ShareTarget {
            r#type: EntityType::File,
            id,
        } => id,
        ShareTarget {
            r#type: EntityType::Folder,
            ..
        } => {
            return Err(AsterError::validation_error(
                "this share is for a folder, not a file",
            ));
        }
    };
    let file = file_repo::find_by_id(state.reader_db(), file_id).await?;
    ensure_share_matches_file(share, &file)?;
    if file.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "file #{file_id} is in trash"
        )));
    }
    Ok(file)
}

pub(super) async fn load_share_folder_resource(
    state: &impl SharedRuntimeState,
    share: &share::Model,
) -> Result<crate::entities::folder::Model> {
    let folder_id = match share_target_for_share(share)? {
        ShareTarget {
            r#type: EntityType::Folder,
            id,
        } => id,
        ShareTarget {
            r#type: EntityType::File,
            ..
        } => {
            return Err(AsterError::validation_error(
                "this share is for a file, not a folder",
            ));
        }
    };
    let folder = folder_repo::find_by_id(state.reader_db(), folder_id).await?;
    ensure_share_matches_folder(share, &folder)?;
    if folder.deleted_at.is_some() {
        return Err(AsterError::folder_not_found(format!(
            "folder #{folder_id} is in trash"
        )));
    }
    Ok(folder)
}

pub(crate) async fn load_valid_folder_share_root(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<(share::Model, i64)> {
    let share = load_valid_share(state, token).await?;
    let root = load_share_folder_resource(state, &share).await?;
    Ok((share, root.id))
}

pub(crate) async fn load_usable_folder_share_root_ignoring_download_limit(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<(share::Model, i64)> {
    let share = load_usable_share_ignoring_download_limit(state, token).await?;
    let root = load_share_folder_resource(state, &share).await?;
    Ok((share, root.id))
}

pub(super) async fn load_shared_folder_file_target(
    state: &impl SharedRuntimeState,
    token: &str,
    file_id: i64,
) -> Result<(share::Model, crate::entities::file::Model)> {
    let (share, root_folder_id) = load_valid_folder_share_root(state, token).await?;
    let file = file_repo::find_by_id(state.reader_db(), file_id).await?;
    ensure_share_matches_file(&share, &file)?;
    if file.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "file #{file_id} is in trash"
        )));
    }
    // 文件夹分享的授权边界不是“同一个 user/team 就行”，而是必须位于
    // share 根目录的子树内；否则同空间的任意文件都会被越权读到。
    let file_folder_id = file.folder_id.ok_or_else(|| {
        auth_forbidden_with_code(
            ApiErrorCode::ShareScopeDenied,
            "file is outside shared folder scope",
        )
    })?;
    folder::verify_folder_in_scope(state.reader_db(), file_folder_id, root_folder_id).await?;
    Ok((share, file))
}

pub(crate) async fn load_shared_folder_file_target_ignoring_download_limit(
    state: &impl SharedRuntimeState,
    token: &str,
    file_id: i64,
) -> Result<(share::Model, crate::entities::file::Model)> {
    let (share, root_folder_id) =
        load_usable_folder_share_root_ignoring_download_limit(state, token).await?;
    let file = file_repo::find_by_id(state.reader_db(), file_id).await?;
    ensure_share_matches_file(&share, &file)?;
    if file.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "file #{file_id} is in trash"
        )));
    }
    let file_folder_id = file.folder_id.ok_or_else(|| {
        auth_forbidden_with_code(
            ApiErrorCode::ShareScopeDenied,
            "file is outside shared folder scope",
        )
    })?;
    folder::verify_folder_in_scope(state.reader_db(), file_folder_id, root_folder_id).await?;
    Ok((share, file))
}

pub(super) async fn load_shared_subfolder_target(
    state: &impl SharedRuntimeState,
    token: &str,
    folder_id: i64,
) -> Result<(share::Model, crate::entities::folder::Model)> {
    let (share, root_folder_id) = load_valid_folder_share_root(state, token).await?;
    let target = folder_repo::find_by_id(state.reader_db(), folder_id).await?;
    ensure_share_matches_folder(&share, &target)?;
    if target.deleted_at.is_some() {
        return Err(AsterError::folder_not_found(format!(
            "folder #{folder_id} is in trash"
        )));
    }
    folder::verify_folder_in_scope(state.reader_db(), folder_id, root_folder_id).await?;
    Ok((share, target))
}

pub(super) fn validate_share(share: &share::Model) -> Result<()> {
    // 这里仅验证 share 自身状态是否还能继续使用。
    // 目标资源是否存在、是否仍在分享范围内，由具体 file / folder 加载函数负责。
    validate_share_without_download_limit(share)?;
    validate_share_download_limit(share)?;
    Ok(())
}

fn validate_share_without_download_limit(share: &share::Model) -> Result<()> {
    share_target_for_share(share)?;

    if let Some(exp) = share.expires_at
        && exp < Utc::now()
    {
        return Err(AsterError::share_expired("share has expired"));
    }
    Ok(())
}

fn validate_share_download_limit(share: &share::Model) -> Result<()> {
    if share.max_downloads > 0 && share.download_count >= share.max_downloads {
        return Err(AsterError::share_download_limit("download limit reached"));
    }

    Ok(())
}

pub(super) fn resolve_share_resource(
    share: &share::Model,
    file_map: &HashMap<i64, crate::entities::file::Model>,
    folder_map: &HashMap<i64, crate::entities::folder::Model>,
) -> Result<(i64, String, EntityType, bool)> {
    match share_target_for_share(share)? {
        ShareTarget {
            r#type: EntityType::File,
            id: file_id,
        } => {
            if let Some(file) = file_map.get(&file_id) {
                return Ok((
                    file_id,
                    file.name.clone(),
                    EntityType::File,
                    file.deleted_at.is_some(),
                ));
            }
            Ok((file_id, "Unknown file".to_string(), EntityType::File, true))
        }
        ShareTarget {
            r#type: EntityType::Folder,
            id: folder_id,
        } => {
            if let Some(folder) = folder_map.get(&folder_id) {
                return Ok((
                    folder_id,
                    folder.name.clone(),
                    EntityType::Folder,
                    folder.deleted_at.is_some(),
                ));
            }
            Ok((
                folder_id,
                "Unknown folder".to_string(),
                EntityType::Folder,
                true,
            ))
        }
    }
}

pub(super) fn resolve_share_status(share: &share::Model, resource_deleted: bool) -> ShareStatus {
    if resource_deleted {
        return ShareStatus::Deleted;
    }
    if share
        .expires_at
        .is_some_and(|expires_at| expires_at < Utc::now())
    {
        return ShareStatus::Expired;
    }
    if share.max_downloads > 0 && share.download_count >= share.max_downloads {
        return ShareStatus::Exhausted;
    }
    ShareStatus::Active
}

pub(super) fn remaining_downloads(max_downloads: i64, download_count: i64) -> Option<i64> {
    (max_downloads > 0).then_some((max_downloads - download_count).max(0))
}

pub(super) async fn resolve_share_name(
    db: &DatabaseConnection,
    share: &share::Model,
) -> Result<(String, String, Option<String>, Option<i64>)> {
    match share_target_for_share(share)? {
        ShareTarget {
            r#type: EntityType::File,
            id: file_id,
        } => {
            let file = file_repo::find_by_id(db, file_id).await?;
            ensure_share_matches_file(share, &file)?;
            if file.deleted_at.is_some() {
                return Err(AsterError::file_not_found(format!(
                    "file #{file_id} is in trash"
                )));
            }
            Ok((
                file.name,
                "file".to_string(),
                Some(file.mime_type),
                Some(file.size),
            ))
        }
        ShareTarget {
            r#type: EntityType::Folder,
            id: folder_id,
        } => {
            let folder = folder_repo::find_by_id(db, folder_id).await?;
            ensure_share_matches_folder(share, &folder)?;
            if folder.deleted_at.is_some() {
                return Err(AsterError::folder_not_found(format!(
                    "folder #{folder_id} is in trash"
                )));
            }
            Ok((folder.name, "folder".to_string(), None, None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::share;
    use crate::types::EntityType;
    use std::collections::HashMap;

    fn mock_share_file(file_id: i64) -> share::Model {
        share::Model {
            id: 1,
            token: "abc".to_string(),
            user_id: 1,
            team_id: None,
            file_id: Some(file_id),
            folder_id: None,
            password: None,
            expires_at: None,
            max_downloads: 0,
            download_count: 0,
            view_count: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn mock_share_folder(folder_id: i64) -> share::Model {
        share::Model {
            id: 2,
            token: "def".to_string(),
            user_id: 1,
            team_id: None,
            file_id: None,
            folder_id: Some(folder_id),
            password: None,
            expires_at: None,
            max_downloads: 0,
            download_count: 0,
            view_count: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn mock_file(id: i64, name: &str) -> crate::entities::file::Model {
        crate::entities::file::Model {
            id,
            name: name.to_string(),
            folder_id: None,
            team_id: None,
            blob_id: 1,
            size: 100,
            owner_user_id: Some(1),
            created_by_user_id: Some(1),
            created_by_username: "tester".to_string(),
            mime_type: "text/plain".to_string(),
            extension: "txt".to_string(),
            compound_extension: None,
            file_category: crate::types::FileCategory::Document,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            is_locked: false,
        }
    }

    fn mock_folder(id: i64, name: &str) -> crate::entities::folder::Model {
        crate::entities::folder::Model {
            id,
            name: name.to_string(),
            parent_id: None,
            team_id: None,
            owner_user_id: Some(1),
            created_by_user_id: Some(1),
            created_by_username: "tester".to_string(),
            policy_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            is_locked: false,
        }
    }

    #[test]
    fn validate_max_downloads_negative_fails() {
        let err = validate_max_downloads(-1).unwrap_err();
        assert_eq!(err.code(), "E005");
    }

    #[test]
    fn validate_max_downloads_zero_ok() {
        assert!(validate_max_downloads(0).is_ok());
    }

    #[test]
    fn resolve_share_status_active() {
        let share = mock_share_file(1);
        assert_eq!(resolve_share_status(&share, false), ShareStatus::Active);
    }

    #[test]
    fn resolve_share_status_deleted() {
        let share = mock_share_file(1);
        assert_eq!(resolve_share_status(&share, true), ShareStatus::Deleted);
    }

    #[test]
    fn resolve_share_status_expired() {
        let mut share = mock_share_file(1);
        share.expires_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        assert_eq!(resolve_share_status(&share, false), ShareStatus::Expired);
    }

    #[test]
    fn resolve_share_status_exhausted() {
        let mut share = mock_share_file(1);
        share.max_downloads = 5;
        share.download_count = 5;
        assert_eq!(resolve_share_status(&share, false), ShareStatus::Exhausted);
    }

    #[test]
    fn remaining_downloads_unlimited() {
        assert_eq!(remaining_downloads(0, 100), None);
    }

    #[test]
    fn remaining_downloads_calculates() {
        assert_eq!(remaining_downloads(10, 3), Some(7));
    }

    #[test]
    fn remaining_downloads_never_negative() {
        assert_eq!(remaining_downloads(5, 10), Some(0));
    }

    #[test]
    fn resolve_share_resource_returns_file_name_and_deleted_state() {
        let share = mock_share_file(7);
        let mut file = mock_file(7, "report.txt");
        file.deleted_at = Some(chrono::Utc::now());

        let file_map = HashMap::from([(file.id, file)]);
        let folder_map = HashMap::new();

        let (id, name, kind, deleted) =
            resolve_share_resource(&share, &file_map, &folder_map).unwrap();

        assert_eq!(id, 7);
        assert_eq!(name, "report.txt");
        assert_eq!(kind, EntityType::File);
        assert!(deleted);
    }

    #[test]
    fn resolve_share_resource_returns_folder_name_and_deleted_state() {
        let share = mock_share_folder(9);
        let mut folder = mock_folder(9, "designs");
        folder.deleted_at = Some(chrono::Utc::now());

        let file_map = HashMap::new();
        let folder_map = HashMap::from([(folder.id, folder)]);

        let (id, name, kind, deleted) =
            resolve_share_resource(&share, &file_map, &folder_map).unwrap();

        assert_eq!(id, 9);
        assert_eq!(name, "designs");
        assert_eq!(kind, EntityType::Folder);
        assert!(deleted);
    }
}
