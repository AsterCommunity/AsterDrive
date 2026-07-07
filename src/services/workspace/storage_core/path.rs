use chrono::Utc;
use sea_orm::Set;

use crate::db::repository::folder_repo;
use crate::entities::folder;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::workspace::scope::{
    WorkspaceStorageScope, load_scope_actor_username_cached, verify_folder_access,
};

use super::policy::{VerifiedFolderPolicyHint, resolve_verified_folder_policy_hint};

pub(crate) struct ParsedUploadPath {
    pub base_folder_id: Option<i64>,
    pub base_folder: Option<VerifiedFolderPolicyHint>,
    pub parent_segments: Vec<String>,
    pub filename: String,
}

pub(crate) struct ResolvedUploadParent {
    pub folder_id: Option<i64>,
    pub folder: Option<VerifiedFolderPolicyHint>,
}

pub(crate) async fn parse_relative_upload_path(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    base_folder_id: Option<i64>,
    relative_path: &str,
) -> Result<ParsedUploadPath> {
    let base_folder = match base_folder_id {
        Some(folder_id) => {
            let folder = verify_folder_access(state, scope, folder_id).await?;
            Some(resolve_verified_folder_policy_hint(state, scope, folder).await?)
        }
        None => None,
    };

    if relative_path.split('/').any(|segment| segment.is_empty()) {
        return Err(AsterError::validation_error(
            "relative_path contains empty path segments",
        ));
    }

    let segments: Vec<&str> = relative_path.split('/').collect();
    let filename = segments
        .last()
        .ok_or_else(|| AsterError::validation_error("relative_path cannot be empty"))?;
    let filename = crate::utils::normalize_validate_name(filename)?;

    let parent_segments: Vec<String> = segments[..segments.len().saturating_sub(1)]
        .iter()
        .map(|segment| crate::utils::normalize_validate_name(segment))
        .collect::<Result<Vec<_>>>()?;

    Ok(ParsedUploadPath {
        base_folder_id,
        base_folder,
        parent_segments,
        filename,
    })
}

pub(crate) async fn ensure_upload_parent_path(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    parsed: &ParsedUploadPath,
    actor_username: Option<&str>,
) -> Result<ResolvedUploadParent> {
    if parsed.parent_segments.is_empty() {
        return Ok(ResolvedUploadParent {
            folder_id: parsed.base_folder_id,
            folder: parsed.base_folder,
        });
    }

    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let mut current_parent = parsed.base_folder_id;
    let mut current_folder = parsed.base_folder;

    // 整条父路径在一个事务里补齐，避免目录上传时只创建出半截层级。
    for segment in &parsed.parent_segments {
        let folder =
            ensure_folder_in_parent(state, &txn, scope, current_parent, segment, actor_username)
                .await?;
        current_parent = Some(folder.id);
        current_folder = Some(match current_folder {
            Some(parent_hint) => parent_hint.merge_child(&folder),
            None => (&folder).into(),
        });
    }

    crate::db::transaction::commit(txn).await?;
    Ok(ResolvedUploadParent {
        folder_id: current_parent,
        folder: current_folder,
    })
}

async fn ensure_folder_in_parent<C: sea_orm::ConnectionTrait>(
    state: &PrimaryAppState,
    db: &C,
    scope: WorkspaceStorageScope,
    parent_id: Option<i64>,
    name: &str,
    actor_username: Option<&str>,
) -> Result<folder::Model> {
    // 目录上传 / 解压导入会并发命中同一路径。
    // 这里先查后建，并在插入冲突后回读，保证“得到该目录”的语义是幂等的。
    let name = crate::utils::normalize_validate_name(name)?;

    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            if let Some(existing) =
                folder_repo::find_by_name_in_parent(db, user_id, parent_id, &name).await?
            {
                return Ok(existing);
            }

            let created_by_username = match actor_username {
                Some(username) => username.to_string(),
                None => load_scope_actor_username_cached(state, scope).await?,
            };
            let now = Utc::now();
            let model = folder::ActiveModel {
                name: Set(name.clone()),
                parent_id: Set(parent_id),
                owner_user_id: Set(Some(user_id)),
                created_by_user_id: Set(Some(user_id)),
                created_by_username: Set(created_by_username),
                policy_id: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            };

            match folder_repo::create(db, model).await {
                Ok(created) => Ok(created),
                Err(err) => {
                    if let Some(existing) =
                        folder_repo::find_by_name_in_parent(db, user_id, parent_id, &name).await?
                    {
                        Ok(existing)
                    } else {
                        Err(err)
                    }
                }
            }
        }
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id,
        } => {
            if let Some(existing) =
                folder_repo::find_by_name_in_team_parent(db, team_id, parent_id, &name).await?
            {
                return Ok(existing);
            }
            let created_by_username = match actor_username {
                Some(username) => username.to_string(),
                None => load_scope_actor_username_cached(state, scope).await?,
            };
            let now = Utc::now();
            let model = folder::ActiveModel {
                name: Set(name.clone()),
                parent_id: Set(parent_id),
                team_id: Set(Some(team_id)),
                owner_user_id: Set(None),
                created_by_user_id: Set(Some(actor_user_id)),
                created_by_username: Set(created_by_username),
                policy_id: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            };

            match folder_repo::create(db, model).await {
                Ok(created) => Ok(created),
                Err(err) => {
                    if let Some(existing) =
                        folder_repo::find_by_name_in_team_parent(db, team_id, parent_id, &name)
                            .await?
                    {
                        Ok(existing)
                    } else {
                        Err(err)
                    }
                }
            }
        }
    }
}
