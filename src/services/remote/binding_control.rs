//! Follower-side reconciliation of binding state owned by the primary.

use reqwest::Method;
use sea_orm::Set;
use serde::Deserialize;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::master_binding_repo;
use crate::errors::Result;
use crate::runtime::FollowerRuntimeState;
use crate::storage::remote_protocol::{
    REMOTE_CONTROL_PLANE_BODY_LIMIT, REMOTE_NODE_BINDING_STATE_PATH, RemoteBindingDesiredState,
    read_reqwest_response_body_limited, send_signed_master_request,
};
use aster_drive_model::entities::master_binding;
use aster_drive_storage::StorageErrorKind;

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    code: ApiErrorCode,
    msg: String,
    data: Option<T>,
}

/// Pull every primary-owned desired state and refresh the follower's runtime
/// binding projection. Returns `true` only when the persisted bindings are
/// available through the runtime registry and can be acknowledged after the
/// worker topology has also been reconciled.
pub async fn reconcile_all<S: FollowerRuntimeState>(state: &S, client: &reqwest::Client) -> bool {
    let bindings = match master_binding_repo::find_all(state.writer_db()).await {
        Ok(bindings) => bindings,
        Err(error) => {
            tracing::warn!("failed to load master bindings for control reconciliation: {error}");
            return false;
        }
    };
    for binding in bindings {
        match pull_desired_state(client, &binding).await {
            Ok(Some(desired)) => match apply_desired_state(state, binding, desired).await {
                Ok(()) => {}
                Err(error) => {
                    tracing::warn!("failed to persist remote binding desired state: {error}")
                }
            },
            Ok(None) => {}
            Err(error) => tracing::warn!(
                binding_id = binding.id,
                master_url = %binding.master_url,
                "failed to pull remote binding desired state: {error}"
            ),
        }
    }
    if let Err(error) = state
        .driver_registry()
        .reload_master_bindings(state.writer_db())
        .await
    {
        tracing::warn!("failed to reload master bindings after control reconciliation: {error}");
        return false;
    }
    true
}

pub async fn mark_all_applied<S: FollowerRuntimeState>(state: &S) {
    let bindings = match master_binding_repo::find_all(state.writer_db()).await {
        Ok(bindings) => bindings,
        Err(error) => {
            tracing::warn!("failed to load master bindings for revision apply: {error}");
            return;
        }
    };
    for binding in bindings {
        if binding.applied_revision >= binding.desired_revision {
            continue;
        }
        if let Err(error) = master_binding_repo::mark_applied_revision(
            state.writer_db(),
            binding.id,
            binding.desired_revision,
        )
        .await
        {
            tracing::warn!(
                binding_id = binding.id,
                desired_revision = binding.desired_revision,
                "failed to mark remote binding revision applied: {error}"
            );
        }
    }
}

async fn apply_desired_state<S: FollowerRuntimeState>(
    state: &S,
    binding: master_binding::Model,
    desired: RemoteBindingDesiredState,
) -> Result<()> {
    let state_changed = binding.name != desired.name
        || binding.is_enabled != desired.is_enabled
        || binding.resolved_transport != desired.resolved_transport
        || binding.desired_revision != desired.desired_revision;
    if !state_changed {
        return Ok(());
    }
    let mut active: master_binding::ActiveModel = binding.into();
    active.name = Set(desired.name);
    active.is_enabled = Set(desired.is_enabled);
    active.resolved_transport = Set(desired.resolved_transport);
    active.desired_revision = Set(desired.desired_revision);
    active.applied_revision = Set(0);
    active.updated_at = Set(chrono::Utc::now());
    master_binding_repo::update(state.writer_db(), active).await?;
    Ok(())
}

async fn pull_desired_state(
    client: &reqwest::Client,
    binding: &master_binding::Model,
) -> Result<Option<RemoteBindingDesiredState>> {
    let path_and_query = format!(
        "{REMOTE_NODE_BINDING_STATE_PATH}?applied_revision={}",
        binding.applied_revision
    );
    let url = format!("{}{path_and_query}", binding.master_url);
    let response =
        send_signed_master_request(client, binding, Method::GET, &url, &path_and_query, None)
            .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let status = response.status();
    let body = read_reqwest_response_body_limited(
        response,
        "pull remote binding desired state",
        REMOTE_CONTROL_PLANE_BODY_LIMIT,
    )
    .await?;
    let envelope: ApiEnvelope<RemoteBindingDesiredState> =
        serde_json::from_slice(&body).map_err(|error| {
            crate::errors::storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("failed to parse remote binding desired state: {error}"),
            )
        })?;
    if !status.is_success() || envelope.code != ApiErrorCode::Success {
        return Err(crate::errors::storage_driver_error(
            StorageErrorKind::Transient,
            if envelope.msg.trim().is_empty() {
                format!("pull remote binding desired state failed with HTTP {status}")
            } else {
                envelope.msg
            },
        ));
    }
    envelope.data.map(Some).ok_or_else(|| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Misconfigured,
            "remote binding desired state response missing data",
        )
    })
}
