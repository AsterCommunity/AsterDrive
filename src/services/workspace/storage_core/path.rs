use chrono::Utc;
use sea_orm::Set;

use crate::db::repository::folder_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::workspace::scope::{
    WorkspaceStorageScope, load_scope_actor_username_cached, verify_folder_access,
};
use aster_drive_model::entities::folder;

use super::policy::{VerifiedFolderPolicyHint, resolve_verified_folder_policy_hint};

#[derive(Clone, Debug)]
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
    let filename = aster_forge_validation::filename::normalize_validate_name(filename)?;

    let parent_segments: Vec<String> = segments[..segments.len().saturating_sub(1)]
        .iter()
        .map(|segment| {
            aster_forge_validation::filename::normalize_validate_name(segment)
                .map_err(AsterError::from)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ParsedUploadPath {
        base_folder_id,
        base_folder,
        parent_segments,
        filename,
    })
}

pub(crate) async fn ensure_upload_parent_path_with_created<C: sea_orm::ConnectionTrait>(
    state: &PrimaryAppState,
    db: &C,
    scope: WorkspaceStorageScope,
    parsed: &ParsedUploadPath,
    actor_username: Option<&str>,
) -> Result<(ResolvedUploadParent, Vec<i64>)> {
    if parsed.parent_segments.is_empty() {
        return Ok((
            ResolvedUploadParent {
                folder_id: parsed.base_folder_id,
                folder: parsed.base_folder,
            },
            Vec::new(),
        ));
    }
    let mut current_parent = parsed.base_folder_id;
    let mut current_folder = parsed.base_folder;
    let mut created = Vec::new();
    for segment in &parsed.parent_segments {
        let existing = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                folder_repo::find_by_name_in_parent(db, user_id, current_parent, segment).await?
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                folder_repo::find_by_name_in_team_parent(db, team_id, current_parent, segment)
                    .await?
            }
        };
        let (folder, was_created) = if let Some(folder) = existing {
            (folder, false)
        } else {
            let created_by_username = match actor_username {
                Some(username) => username.to_string(),
                None => load_scope_actor_username_cached(state, scope).await?,
            };
            let now = Utc::now();
            let model = match scope {
                WorkspaceStorageScope::Personal { user_id } => folder::ActiveModel {
                    name: Set(segment.clone()),
                    parent_id: Set(current_parent),
                    owner_user_id: Set(Some(user_id)),
                    created_by_user_id: Set(Some(user_id)),
                    created_by_username: Set(created_by_username),
                    policy_id: Set(None),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                },
                WorkspaceStorageScope::Team {
                    team_id,
                    actor_user_id,
                } => folder::ActiveModel {
                    name: Set(segment.clone()),
                    parent_id: Set(current_parent),
                    team_id: Set(Some(team_id)),
                    owner_user_id: Set(None),
                    created_by_user_id: Set(Some(actor_user_id)),
                    created_by_username: Set(created_by_username),
                    policy_id: Set(None),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                },
            };
            let (folder, was_created) = match scope {
                WorkspaceStorageScope::Personal { user_id } => {
                    folder_repo::create_or_find_by_name_in_parent_with_created(
                        db,
                        model,
                        user_id,
                        current_parent,
                        segment,
                    )
                    .await?
                }
                WorkspaceStorageScope::Team { team_id, .. } => {
                    folder_repo::create_or_find_by_name_in_team_parent_with_created(
                        db,
                        model,
                        team_id,
                        current_parent,
                        segment,
                    )
                    .await?
                }
            };
            (folder, was_created)
        };
        if was_created {
            created.push(folder.id);
        }
        current_parent = Some(folder.id);
        current_folder = Some(match current_folder {
            Some(parent_hint) => parent_hint.merge_child(&folder),
            None => (&folder).into(),
        });
    }
    Ok((
        ResolvedUploadParent {
            folder_id: current_parent,
            folder: current_folder,
        },
        created,
    ))
}

pub(crate) async fn resolve_existing_upload_parent(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    parsed: &ParsedUploadPath,
) -> Result<ResolvedUploadParent> {
    let mut current_parent = parsed.base_folder_id;
    let mut current_folder = parsed.base_folder;

    for segment in &parsed.parent_segments {
        let existing = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                folder_repo::find_by_name_in_parent(
                    state.writer_db(),
                    user_id,
                    current_parent,
                    segment,
                )
                .await?
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                folder_repo::find_by_name_in_team_parent(
                    state.writer_db(),
                    team_id,
                    current_parent,
                    segment,
                )
                .await?
            }
        };
        let Some(folder) = existing else {
            break;
        };
        current_parent = Some(folder.id);
        current_folder = Some(match current_folder {
            Some(parent_hint) => parent_hint.merge_child(&folder),
            None => (&folder).into(),
        });
    }

    Ok(ResolvedUploadParent {
        folder_id: current_parent,
        folder: current_folder,
    })
}

pub(crate) async fn ensure_upload_parent_path_on<C: sea_orm::ConnectionTrait>(
    state: &PrimaryAppState,
    db: &C,
    scope: WorkspaceStorageScope,
    parsed: &ParsedUploadPath,
    actor_username: Option<&str>,
) -> Result<ResolvedUploadParent> {
    ensure_upload_parent_path_with_created(state, db, scope, parsed, actor_username)
        .await
        .map(|(resolved, _)| resolved)
}
