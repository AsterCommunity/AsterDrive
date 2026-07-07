use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{folder_repo, team_repo, user_repo};
use crate::entities::folder;
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::workspace::scope::{
    WorkspaceStorageScope, require_team_policy_group_id, verify_folder_access,
};
use crate::types::{DriverType, parse_storage_policy_options};

pub(crate) async fn load_storage_limits(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
) -> Result<(i64, i64)> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            let user = user_repo::find_by_id(state.writer_db(), user_id).await?;
            Ok((user.storage_used, user.storage_quota))
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            let team = team_repo::find_active_by_id(state.writer_db(), team_id).await?;
            Ok((team.storage_used, team.storage_quota))
        }
    }
}

pub(crate) fn local_content_dedup_enabled(policy: &crate::entities::storage_policy::Model) -> bool {
    policy.driver_type == DriverType::Local
        && parse_storage_policy_options(policy.options.as_ref())
            .content_dedup
            .unwrap_or(false)
}

/// Policy hint captured from a folder after the caller has already verified that the folder is
/// accessible within the target workspace scope.
///
/// This is not an access token and does not perform validation by itself. Only construct it from
/// folders returned by `verify_folder_access` or from child folders created/loaded while walking an
/// already verified upload path.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VerifiedFolderPolicyHint {
    policy_id: Option<i64>,
}

impl VerifiedFolderPolicyHint {
    pub(crate) fn policy_id(&self) -> Option<i64> {
        self.policy_id
    }

    pub(crate) fn merge_child(self, child: &folder::Model) -> Self {
        Self {
            policy_id: child.policy_id.or(self.policy_id),
        }
    }
}

impl From<&folder::Model> for VerifiedFolderPolicyHint {
    fn from(folder: &folder::Model) -> Self {
        Self {
            policy_id: folder.policy_id,
        }
    }
}

impl From<folder::Model> for VerifiedFolderPolicyHint {
    fn from(folder: folder::Model) -> Self {
        Self {
            policy_id: folder.policy_id,
        }
    }
}

async fn resolve_scope_policy_for_size(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_size: i64,
) -> Result<crate::entities::storage_policy::Model> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => state
            .policy_snapshot
            .resolve_user_policy_for_size(user_id, file_size),
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id,
        } => {
            let policy_group_id =
                require_team_policy_group_id(state, team_id, actor_user_id).await?;
            state
                .policy_snapshot
                .resolve_policy_in_group(policy_group_id, file_size)
        }
    }
}

pub(crate) async fn resolve_policy_for_size_with_verified_folder(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder: Option<VerifiedFolderPolicyHint>,
    file_size: i64,
) -> Result<crate::entities::storage_policy::Model> {
    if let Some(folder) = folder
        && let Some(policy_id) = folder.policy_id()
    {
        return resolve_bound_folder_policy(state, policy_id);
    }

    resolve_scope_policy_for_size(state, scope, file_size).await
}

pub(crate) async fn resolve_verified_folder_policy_hint(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder: folder::Model,
) -> Result<VerifiedFolderPolicyHint> {
    Ok(VerifiedFolderPolicyHint {
        policy_id: resolve_effective_folder_policy_id(state, scope, folder).await?,
    })
}

pub(crate) fn ensure_policy_available_for_folder_binding(
    state: &impl SharedRuntimeState,
    policy: &crate::entities::storage_policy::Model,
) -> Result<()> {
    if state
        .policy_snapshot()
        .is_policy_available_for_outbound(policy)
    {
        return Ok(());
    }

    let reason = state
        .policy_snapshot()
        .describe_policy_outbound_availability(policy)
        .unwrap_or_else(|| "policy is disabled or unavailable".to_string());
    Err(validation_error_with_code(
        ApiErrorCode::BadRequest,
        format!("storage policy #{} is not available: {reason}", policy.id),
    ))
}

fn resolve_bound_folder_policy(
    state: &PrimaryAppState,
    policy_id: i64,
) -> Result<crate::entities::storage_policy::Model> {
    let policy = state.policy_snapshot().get_policy_or_err(policy_id)?;
    ensure_policy_available_for_folder_binding(state, &policy)?;
    Ok(policy)
}

async fn resolve_effective_folder_policy_id(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder: folder::Model,
) -> Result<Option<i64>> {
    let folder_id = folder.id;
    let ancestors = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_ancestor_models(state.reader_db(), user_id, folder_id).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_team_ancestor_models(state.reader_db(), team_id, folder_id).await?
        }
    };

    let mut expected_child_id = Some(folder_id);
    let mut expected_parent_id = folder.parent_id;
    let mut closest_policy_id = folder.policy_id;

    for ancestor in ancestors.iter().rev().skip(1) {
        if expected_parent_id != Some(ancestor.id) {
            return Err(AsterError::validation_error(
                "folder hierarchy is incomplete",
            ));
        }
        if expected_child_id == Some(ancestor.id) {
            return Err(AsterError::validation_error(
                "folder hierarchy contains a cycle",
            ));
        }
        closest_policy_id = closest_policy_id.or(ancestor.policy_id);
        expected_child_id = Some(ancestor.id);
        expected_parent_id = ancestor.parent_id;
    }

    if expected_parent_id.is_some() {
        return Err(AsterError::validation_error(
            "folder hierarchy is incomplete",
        ));
    }

    Ok(closest_policy_id)
}

pub(crate) async fn resolve_policy_for_size(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    file_size: i64,
) -> Result<crate::entities::storage_policy::Model> {
    // 文件夹级策略覆盖优先级最高。
    // 只有目标文件夹没有显式绑定策略时，才回退到个人默认策略 / 团队策略组。
    if let Some(folder_id) = folder_id {
        let folder = verify_folder_access(state, scope, folder_id).await?;

        if let Some(policy_id) = resolve_effective_folder_policy_id(state, scope, folder).await? {
            return resolve_bound_folder_policy(state, policy_id);
        }
    }

    resolve_scope_policy_for_size(state, scope, file_size).await
}
