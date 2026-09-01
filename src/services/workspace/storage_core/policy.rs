use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{file_repo, folder_repo, team_repo, user_repo};
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::storage_policy::policy::placement::{
    FolderPlacementOverride, StoragePlacementContext, StorageRoutingDecision,
};
use crate::services::workspace::scope::{
    WorkspaceStorageScope, require_team_policy_group_id_with_db, verify_folder_access,
};
use aster_drive_model::entities::folder;
use sea_orm::ConnectionTrait;

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

pub(crate) fn local_content_dedup_enabled(
    registry: &crate::storage::connectors::StorageConnectorRegistry,
    policy: &aster_drive_model::entities::storage_policy::Model,
) -> Result<bool> {
    Ok(
        crate::storage::connectors::resolve_local_filesystem_projection(registry, policy)?
            .is_some_and(|projection| projection.content_dedup),
    )
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

pub(crate) struct BlobPolicyRequest<'a> {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub folder_hint: Option<VerifiedFolderPolicyHint>,
    pub filename: &'a str,
    pub file_size: i64,
    pub mime_type: &'a str,
    pub existing_file_id: Option<i64>,
}

pub(crate) struct BlobPolicyResolution {
    pub policy: aster_drive_model::entities::storage_policy::Model,
    pub routing_decision: Option<StorageRoutingDecision>,
}

pub(crate) async fn resolve_blob_policy_for_write(
    state: &PrimaryAppState,
    request: BlobPolicyRequest<'_>,
) -> Result<BlobPolicyResolution> {
    resolve_blob_policy_for_write_on(state, state.writer_db(), request).await
}

pub(crate) async fn resolve_blob_policy_for_write_on<C: sea_orm::ConnectionTrait>(
    state: &PrimaryAppState,
    db: &C,
    request: BlobPolicyRequest<'_>,
) -> Result<BlobPolicyResolution> {
    if let Some(existing_file_id) = request.existing_file_id {
        let file = crate::services::workspace::scope::verify_file_access(
            state,
            request.scope,
            existing_file_id,
        )
        .await?;
        let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id).await?;
        let policy = state.policy_snapshot().get_policy_or_err(blob.policy_id)?;
        return Ok(BlobPolicyResolution {
            policy,
            routing_decision: None,
        });
    }

    let folder_hint = match (request.folder_hint, request.folder_id) {
        (Some(hint), _) => Some(hint),
        (None, Some(folder_id)) => {
            let folder = verify_folder_access(state, request.scope, folder_id).await?;
            Some(resolve_verified_folder_policy_hint(state, request.scope, folder).await?)
        }
        (None, None) => None,
    };
    let (policy, routing_decision) = resolve_new_blob_policy_from_snapshot(
        state,
        db,
        request.scope,
        folder_hint,
        request.filename,
        request.file_size,
        request.mime_type,
    )
    .await?;
    Ok(BlobPolicyResolution {
        policy,
        routing_decision: Some(routing_decision),
    })
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

/// Resolve a new blob placement entirely from the in-memory policy snapshot.
///
/// Database access remains at the surrounding upload boundary for permissions,
/// quota and session persistence. Profile/rule/target matching itself is kept
/// off the upload hot path database lookup.
async fn resolve_new_blob_policy_from_snapshot<C: sea_orm::ConnectionTrait>(
    state: &impl SharedRuntimeState,
    fallback_db: &C,
    scope: WorkspaceStorageScope,
    folder: Option<VerifiedFolderPolicyHint>,
    filename: &str,
    file_size: i64,
    mime_type: &str,
) -> Result<(
    aster_drive_model::entities::storage_policy::Model,
    StorageRoutingDecision,
)> {
    let profile_id = match scope {
        WorkspaceStorageScope::Personal { user_id } => state
            .policy_snapshot()
            .require_user_policy_group_id(user_id)?,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id,
        } => match state
            .policy_snapshot()
            .resolve_team_policy_group_id(team_id)
        {
            Some(profile_id) => profile_id,
            None => {
                // TODO(0.6.0): remove this cache-miss compatibility read once
                // all team assignment mutation paths publish snapshot updates.
                require_team_policy_group_id_with_db(state, fallback_db, team_id, actor_user_id)
                    .await?
            }
        },
    };
    let context =
        StoragePlacementContext::from_filename(profile_id, filename, file_size, mime_type);
    let folder_override = folder
        .and_then(|hint| hint.policy_id())
        .map(|policy_id| -> Result<FolderPlacementOverride> {
            let policy = state.policy_snapshot().get_policy_or_err(policy_id)?;
            Ok(FolderPlacementOverride {
                policy_id,
                policy_max_file_size: policy.max_file_size,
                is_available: state
                    .policy_snapshot()
                    .is_policy_available_for_outbound(&policy),
            })
        })
        .transpose()?;
    if let Some(folder) = folder_override.as_ref()
        && folder.policy_max_file_size > 0
        && file_size > folder.policy_max_file_size
    {
        return Err(AsterError::file_too_large(format!(
            "file size {} exceeds limit {}",
            file_size, folder.policy_max_file_size
        )));
    }
    let decision = match state.policy_snapshot().resolve_placement(
        profile_id,
        &context,
        folder_override.as_ref(),
    ) {
        Ok(decision) => decision,
        Err(error) => {
            state.metrics().record_storage_routing("none", error.code());
            state.metrics().record_storage_routing_detail(
                &profile_id.to_string(),
                "none",
                "none",
                "none",
                error.code(),
            );
            return Err(error);
        }
    };
    state.metrics().record_storage_routing(
        decision.selection_mode.as_str(),
        if decision.folder_override {
            "folder_override"
        } else {
            "selected"
        },
    );
    let rule_id = decision
        .rule_id
        .map_or_else(|| "none".to_string(), |id| id.to_string());
    let profile_id = decision.profile_id.to_string();
    let policy_id = decision.policy_id.to_string();
    state.metrics().record_storage_routing_detail(
        &profile_id,
        &rule_id,
        &policy_id,
        decision.selection_mode.as_str(),
        if decision.folder_override {
            "folder_override"
        } else {
            "selected"
        },
    );
    tracing::debug!(
        placement_profile_id = decision.profile_id,
        placement_revision = decision.revision,
        placement_rule_id = decision.rule_id,
        policy_id = decision.policy_id,
        selection_mode = decision.selection_mode.as_str(),
        folder_override = decision.folder_override,
        excluded_target_count = decision.excluded_targets.len(),
        "storage placement decision selected"
    );
    let policy = state
        .policy_snapshot()
        .get_policy_or_err(decision.policy_id)?;
    Ok((policy, decision))
}

pub(crate) async fn resolve_verified_folder_policy_hint(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder: folder::Model,
) -> Result<VerifiedFolderPolicyHint> {
    resolve_verified_folder_policy_hint_on(state.reader_db(), scope, folder).await
}

pub(crate) async fn resolve_verified_folder_policy_hint_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder: folder::Model,
) -> Result<VerifiedFolderPolicyHint> {
    Ok(VerifiedFolderPolicyHint {
        policy_id: resolve_effective_folder_policy_id_on(db, scope, folder).await?,
    })
}

pub(crate) fn ensure_policy_available_for_folder_binding(
    state: &impl SharedRuntimeState,
    policy: &aster_drive_model::entities::storage_policy::Model,
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

async fn resolve_effective_folder_policy_id_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder: folder::Model,
) -> Result<Option<i64>> {
    let folder_id = folder.id;
    let ancestors = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_ancestor_models(db, user_id, folder_id).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_team_ancestor_models(db, team_id, folder_id).await?
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
