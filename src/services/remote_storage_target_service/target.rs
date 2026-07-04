use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::remote_storage_target_repo;
use crate::entities::master_binding;
use crate::errors::{Result, precondition_failed_with_code};
use crate::runtime::FollowerRuntimeState;

use super::driver::build_driver_from_target;
use super::models::ResolvedIngressTarget;

pub async fn resolve_effective_target<S: FollowerRuntimeState>(
    state: &S,
    binding: &master_binding::Model,
) -> Result<ResolvedIngressTarget> {
    let targets =
        remote_storage_target_repo::find_all_by_binding(state.writer_db(), binding.id).await?;
    if targets.is_empty() {
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
    if !target.last_error.trim().is_empty() {
        return Err(precondition_failed_with_code(
            ApiErrorCode::ManagedIngressDefaultError,
            format!(
                "managed remote storage target '{}' is not ready: {}",
                target.target_key, target.last_error
            ),
        ));
    }
    if target.applied_revision < target.desired_revision {
        return Err(precondition_failed_with_code(
            ApiErrorCode::ManagedIngressDefaultNotApplied,
            format!(
                "managed remote storage target '{}' is pending apply",
                target.target_key
            ),
        ));
    }

    let driver = build_driver_from_target(state, &target)?;
    Ok(ResolvedIngressTarget {
        driver,
        max_file_size: target.max_file_size,
    })
}
