use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::remote_storage_target_repo;
use crate::entities::master_binding;
use crate::errors::{Result, precondition_failed_with_code};
use crate::runtime::FollowerRuntimeState;

use super::driver::build_driver_from_target;
use super::models::ResolvedRemoteStorageTarget;

pub async fn resolve_effective_target<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
) -> Result<ResolvedRemoteStorageTarget> {
    // TODO(remote-storage-target): legacy primary policies without
    // remote_storage_target_key still fall back to the binding default target.
    // This is intentionally follower-local because the primary cannot backfill
    // target intent from its own schema. New remote policies should call
    // resolve_target_by_key through signed requests that include target_key.
    // Remove this compatibility path after policy-level target migration.
    let targets =
        remote_storage_target_repo::find_all_by_binding(state.writer_db(), binding.id).await?;
    if targets.is_empty() {
        // Keep the legacy managed_ingress.* wire code here: this branch is the
        // follower-side compatibility fallback for old policies without an
        // explicit remote_storage_target_key, not the storage policy editor
        // validation path.
        return Err(precondition_failed_with_code(
            ApiErrorCode::ManagedIngressRequired,
            "remote storage target is required before follower can accept remote writes",
        ));
    }

    let target = remote_storage_target_repo::find_default_by_binding(state.writer_db(), binding.id)
        .await?
        .ok_or_else(|| {
            precondition_failed_with_code(
                ApiErrorCode::ManagedIngressDefaultMissing,
                "remote storage targets exist but no default target is configured",
            )
        })?;
    build_resolved_target(state, target)
}

pub async fn resolve_target_by_key<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
    target_key: &str,
) -> Result<ResolvedRemoteStorageTarget> {
    let target = remote_storage_target_repo::find_by_binding_and_target_key(
        state.writer_db(),
        binding.id,
        target_key,
    )
    .await?
    .ok_or_else(|| {
        precondition_failed_with_code(
            ApiErrorCode::RemoteStorageTargetNotFound,
            format!("remote storage target '{target_key}' is not configured"),
        )
    })?;
    build_resolved_target(state, target)
}

fn build_resolved_target<S: FollowerRuntimeState>(
    state: &S,
    target: crate::entities::remote_storage_target::Model,
) -> Result<ResolvedRemoteStorageTarget> {
    if !target.last_error.trim().is_empty() {
        return Err(precondition_failed_with_code(
            ApiErrorCode::ManagedIngressDefaultError,
            format!(
                "remote storage target '{}' is not ready: {}",
                target.target_key, target.last_error
            ),
        ));
    }
    if target.applied_revision < target.desired_revision {
        return Err(precondition_failed_with_code(
            ApiErrorCode::ManagedIngressDefaultNotApplied,
            format!(
                "remote storage target '{}' is pending apply",
                target.target_key
            ),
        ));
    }
    let driver = build_driver_from_target(state, &target)?;
    Ok(ResolvedRemoteStorageTarget { driver })
}
