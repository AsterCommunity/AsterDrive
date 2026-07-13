//! 服务模块：`remote::remote_node`。

use crate::api::api_error_code::ApiErrorCode;
use crate::api::pagination::{AdminRemoteNodeSortBy, load_offset_page};
use crate::db::repository::{follower_enrollment_session_repo, managed_follower_repo, policy_repo};
use crate::entities::{follower_enrollment_session, managed_follower};
use crate::errors::{
    AsterError, Result, precondition_failed_with_code, validation_error_with_code,
};
use crate::runtime::RemoteProtocolRuntimeState;
use crate::services::remote::capability::RemoteCapabilityResolver;
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::remote_protocol::{
    RemoteBindingSyncRequest, RemoteStorageCapabilities, RemoteStorageClient,
    normalize_remote_base_url,
};
use crate::types::{RemoteNodeTransportMode, parse_storage_policy_options};
use aster_forge_api::{OffsetPage, SortOrder};
use chrono::Utc;
use futures::{StreamExt, stream};
use sea_orm::{ActiveModelTrait, DbErr, Set, SqlErr};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

const REMOTE_BINDING_SYNC_TIMEOUT: Duration = Duration::from_secs(5);
const REMOTE_NODE_HEALTH_TEST_CONCURRENCY: usize = 4;
pub const REMOTE_NODE_ENROLLMENT_REQUIRED_MESSAGE: &str =
    "remote node enrollment must be completed before accessing the remote follower";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RemoteNodeEnrollmentStatus {
    NotStarted,
    Pending,
    Redeemed,
    Completed,
    Expired,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteNodeInfo {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub transport_mode: RemoteNodeTransportMode,
    pub is_enabled: bool,
    pub enrollment_status: RemoteNodeEnrollmentStatus,
    pub last_error: String,
    pub capabilities: RemoteStorageCapabilities,
    pub last_checked_at: Option<chrono::DateTime<Utc>>,
    pub tunnel: crate::storage::remote_protocol::tunnel::server::RemoteTunnelInfo,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
}

impl RemoteNodeInfo {
    fn from_model<S: RemoteProtocolRuntimeState>(
        state: &S,
        model: managed_follower::Model,
        enrollment_status: RemoteNodeEnrollmentStatus,
    ) -> Self {
        Self {
            id: model.id,
            name: model.name.clone(),
            base_url: model.base_url.clone(),
            transport_mode: model.transport_mode,
            is_enabled: model.is_enabled,
            enrollment_status,
            last_error: model.last_error.clone(),
            capabilities: parse_capabilities(&model.last_capabilities),
            last_checked_at: model.last_checked_at,
            tunnel: crate::storage::remote_protocol::tunnel::server::tunnel_info_for_node(
                state, &model,
            ),
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateRemoteNodeInput {
    pub name: String,
    pub base_url: String,
    pub transport_mode: RemoteNodeTransportMode,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateRemoteNodeInput {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub transport_mode: Option<RemoteNodeTransportMode>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct TestRemoteNodeInput {
    pub base_url: String,
    pub access_key: String,
    pub secret_key: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RemoteNodeHealthTestStats {
    pub checked: u64,
    pub healthy: u64,
    pub failed: u64,
    pub skipped: u64,
}

struct ProbedRemoteNode {
    model: managed_follower::Model,
    probe_error: Option<AsterError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteNodeHealthTestOutcome {
    Skipped,
    Healthy,
    Failed,
}

pub async fn list_paginated<S: RemoteProtocolRuntimeState>(
    state: &S,
    limit: u64,
    offset: u64,
    sort_by: AdminRemoteNodeSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<RemoteNodeInfo>> {
    let page = load_offset_page(limit, offset, 100, |limit, offset| async move {
        let (items, total) = managed_follower_repo::find_paginated(
            state.writer_db(),
            limit,
            offset,
            sort_by,
            sort_order,
        )
        .await?;
        Ok((items, total))
    })
    .await?;
    let node_ids: Vec<i64> = page.items.iter().map(|model| model.id).collect();
    let enrollment_statuses = enrollment_statuses_for_nodes(state, &node_ids).await?;
    let items = page
        .items
        .into_iter()
        .map(|model| {
            let enrollment_status = enrollment_statuses
                .get(&model.id)
                .copied()
                .unwrap_or(RemoteNodeEnrollmentStatus::NotStarted);
            RemoteNodeInfo::from_model(state, model, enrollment_status)
        })
        .collect();
    Ok(OffsetPage::new(items, page.total, page.limit, page.offset))
}

pub async fn get<S: RemoteProtocolRuntimeState>(state: &S, id: i64) -> Result<RemoteNodeInfo> {
    let model = managed_follower_repo::find_by_id(state.writer_db(), id).await?;
    remote_node_info(state, model).await
}

pub async fn create<S: RemoteProtocolRuntimeState>(
    state: &S,
    input: CreateRemoteNodeInput,
) -> Result<RemoteNodeInfo> {
    let normalized = normalize_create_input(input)?;
    let (access_key, secret_key) = generate_managed_credentials();
    let now = Utc::now();
    let created = managed_follower::ActiveModel {
        name: Set(normalized.name),
        base_url: Set(normalized.base_url),
        access_key: Set(access_key),
        secret_key: Set(secret_key),
        is_enabled: Set(normalized.is_enabled),
        transport_mode: Set(normalized.transport_mode),
        last_capabilities: Set("{}".to_string()),
        last_error: Set(String::new()),
        last_checked_at: Set(None),
        tunnel_last_error: Set(String::new()),
        tunnel_last_seen_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .map_err(map_remote_node_db_err)?;

    refresh_registry(state).await?;
    remote_node_info(state, created).await
}

pub async fn update<S: RemoteProtocolRuntimeState>(
    state: &S,
    id: i64,
    input: UpdateRemoteNodeInput,
) -> Result<RemoteNodeInfo> {
    let existing = managed_follower_repo::find_by_id(state.writer_db(), id).await?;
    let normalized = normalize_update_input(input)?;
    let next_base_url = normalized
        .base_url
        .as_deref()
        .unwrap_or(existing.base_url.as_str());
    let next_transport_mode = normalized.transport_mode.unwrap_or(existing.transport_mode);
    ensure_transport_change_keeps_referencing_policies_valid(
        state,
        id,
        next_transport_mode,
        next_base_url,
    )
    .await?;

    let mut active: managed_follower::ActiveModel = existing.into();
    if let Some(value) = normalized.name {
        active.name = Set(value);
    }
    if let Some(value) = normalized.base_url {
        active.base_url = Set(value);
    }
    if let Some(value) = normalized.transport_mode {
        active.transport_mode = Set(value);
    }
    if let Some(value) = normalized.is_enabled {
        active.is_enabled = Set(value);
    }
    active.updated_at = Set(Utc::now());

    let updated = active
        .update(state.writer_db())
        .await
        .map_err(map_remote_node_db_err)?;
    refresh_registry(state).await?;
    if enrollment_status_for_node(state, updated.id).await? == RemoteNodeEnrollmentStatus::Completed
        && let Err(error) =
            sync_remote_binding_config_with_timeout(state, &updated, REMOTE_BINDING_SYNC_TIMEOUT)
                .await
    {
        tracing::warn!(
            remote_node_id = updated.id,
            "failed to sync remote binding config to follower: {error}"
        );
    }
    remote_node_info(state, updated).await
}

pub async fn delete<S: RemoteProtocolRuntimeState>(state: &S, id: i64) -> Result<()> {
    tracing::debug!(remote_node_id = id, "deleting remote node");
    let policy_refs = policy_repo::count_by_remote_node_id(state.writer_db(), id).await?;
    if policy_refs > 0 {
        return Err(AsterError::validation_error(format!(
            "cannot delete remote node: {policy_refs} storage policy(s) still reference it"
        )));
    }
    managed_follower_repo::delete(state.writer_db(), id).await?;
    refresh_registry(state).await?;
    tracing::info!(remote_node_id = id, "deleted remote node");
    Ok(())
}

pub async fn test_connection<S: RemoteProtocolRuntimeState>(
    state: &S,
    id: i64,
) -> Result<RemoteNodeInfo> {
    let node = require_completed_enrollment(state, id).await?;
    let probed = probe_and_persist_node(state, &node).await?;
    if let Some(error) = probed.probe_error {
        return Err(error);
    }
    remote_node_info(state, probed.model).await
}

pub async fn require_completed_enrollment<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<managed_follower::Model> {
    let node = managed_follower_repo::find_by_id(state.writer_db(), remote_node_id).await?;
    if enrollment_status_for_node(state, node.id).await? != RemoteNodeEnrollmentStatus::Completed {
        return Err(precondition_failed_with_code(
            ApiErrorCode::RemoteNodeEnrollmentRequired,
            REMOTE_NODE_ENROLLMENT_REQUIRED_MESSAGE,
        ));
    }
    Ok(node)
}

pub async fn test_connection_params(
    input: TestRemoteNodeInput,
) -> Result<RemoteStorageCapabilities> {
    probe_connection(&input).await
}

async fn remote_node_info<S: RemoteProtocolRuntimeState>(
    state: &S,
    model: managed_follower::Model,
) -> Result<RemoteNodeInfo> {
    let enrollment_status = enrollment_status_for_node(state, model.id).await?;
    Ok(RemoteNodeInfo::from_model(state, model, enrollment_status))
}

async fn enrollment_statuses_for_nodes<S: RemoteProtocolRuntimeState>(
    state: &S,
    node_ids: &[i64],
) -> Result<HashMap<i64, RemoteNodeEnrollmentStatus>> {
    if node_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let sessions =
        follower_enrollment_session_repo::find_by_managed_follower_ids(state.writer_db(), node_ids)
            .await?;
    let mut completed_node_ids = HashSet::new();
    let mut latest_by_node = HashMap::new();
    for session in sessions {
        if session.acked_at.is_some() {
            completed_node_ids.insert(session.managed_follower_id);
        }
        latest_by_node
            .entry(session.managed_follower_id)
            .or_insert(session);
    }

    let now = Utc::now();
    Ok(node_ids
        .iter()
        .copied()
        .map(|node_id| {
            let status = if completed_node_ids.contains(&node_id) {
                RemoteNodeEnrollmentStatus::Completed
            } else {
                enrollment_status_from_latest(latest_by_node.get(&node_id), now)
            };
            (node_id, status)
        })
        .collect())
}

async fn enrollment_status_for_node<S: RemoteProtocolRuntimeState>(
    state: &S,
    node_id: i64,
) -> Result<RemoteNodeEnrollmentStatus> {
    if follower_enrollment_session_repo::has_completed_for_managed_follower(
        state.writer_db(),
        node_id,
    )
    .await?
    {
        return Ok(RemoteNodeEnrollmentStatus::Completed);
    }

    let latest = follower_enrollment_session_repo::find_latest_for_managed_follower(
        state.writer_db(),
        node_id,
    )
    .await?;
    Ok(enrollment_status_from_latest(latest.as_ref(), Utc::now()))
}

fn enrollment_status_from_latest(
    latest: Option<&follower_enrollment_session::Model>,
    now: chrono::DateTime<Utc>,
) -> RemoteNodeEnrollmentStatus {
    let Some(latest) = latest else {
        return RemoteNodeEnrollmentStatus::NotStarted;
    };

    if latest.acked_at.is_some() {
        return RemoteNodeEnrollmentStatus::Completed;
    }

    if latest.invalidated_at.is_some() {
        return RemoteNodeEnrollmentStatus::NotStarted;
    }

    if latest.expires_at <= now {
        return RemoteNodeEnrollmentStatus::Expired;
    }

    if latest.redeemed_at.is_some() {
        return RemoteNodeEnrollmentStatus::Redeemed;
    }

    RemoteNodeEnrollmentStatus::Pending
}

pub async fn run_health_tests<S: RemoteProtocolRuntimeState>(
    state: &S,
) -> Result<RemoteNodeHealthTestStats> {
    let nodes = managed_follower_repo::find_all(state.writer_db()).await?;
    let node_ids = nodes.iter().map(|node| node.id).collect::<Vec<_>>();
    let enrollment_statuses = enrollment_statuses_for_nodes(state, &node_ids).await?;
    let outcomes = stream::iter(nodes.into_iter().map(|node| {
        let enrollment_status = enrollment_statuses
            .get(&node.id)
            .copied()
            .unwrap_or(RemoteNodeEnrollmentStatus::NotStarted);
        async move { run_health_test_for_node(state, node, enrollment_status).await }
    }))
    .buffer_unordered(REMOTE_NODE_HEALTH_TEST_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;

    let mut stats = RemoteNodeHealthTestStats::default();
    for outcome in outcomes {
        match outcome? {
            RemoteNodeHealthTestOutcome::Skipped => stats.skipped += 1,
            RemoteNodeHealthTestOutcome::Healthy => {
                stats.checked += 1;
                stats.healthy += 1;
            }
            RemoteNodeHealthTestOutcome::Failed => {
                stats.checked += 1;
                stats.failed += 1;
            }
        }
    }

    Ok(stats)
}

pub fn parse_capabilities(raw: &str) -> RemoteStorageCapabilities {
    RemoteStorageCapabilities::from_stored_json(raw)
}

pub fn serialize_capabilities(capabilities: &RemoteStorageCapabilities) -> String {
    serde_json::to_string(capabilities).unwrap_or_else(|_| "{}".to_string())
}

async fn probe_connection(input: &TestRemoteNodeInput) -> Result<RemoteStorageCapabilities> {
    let client = RemoteStorageClient::new(&input.base_url, &input.access_key, &input.secret_key)?;
    client.probe_capabilities().await
}

pub(crate) fn remote_storage_client_for_node<S: RemoteProtocolRuntimeState>(
    state: &S,
    node: &managed_follower::Model,
) -> Result<crate::storage::remote_protocol::RemoteStorageClient> {
    state.remote_protocol().client_for_node(node)
}

async fn policy_requirements_for_node<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<Vec<(i64, crate::types::StoragePolicyOptions)>> {
    let policies = policy_repo::find_by_remote_node_id(state.writer_db(), remote_node_id).await?;
    Ok(policies
        .into_iter()
        .map(|policy| {
            (
                policy.id,
                parse_storage_policy_options(policy.options.as_ref()),
            )
        })
        .collect())
}

async fn ensure_transport_change_keeps_referencing_policies_valid<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    transport_mode: RemoteNodeTransportMode,
    base_url: &str,
) -> Result<()> {
    if !transport_mode.resolves_to_reverse_tunnel(base_url) {
        return Ok(());
    }

    for (policy_id, options) in policy_requirements_for_node(state, remote_node_id).await? {
        if RemoteCapabilityResolver::requires_direct_transport_for_presigned(&options) {
            return Err(AsterError::validation_error(format!(
                "cannot switch remote node #{remote_node_id} to reverse tunnel while storage policy #{policy_id} uses presigned browser transfer strategies",
            )));
        }
    }
    Ok(())
}

async fn probe_and_persist_node<S: RemoteProtocolRuntimeState>(
    state: &S,
    node: &managed_follower::Model,
) -> Result<ProbedRemoteNode> {
    let capabilities = remote_storage_client_for_node(state, node)?
        .probe_capabilities()
        .await;

    let (last_capabilities, last_error, probe_error) = match capabilities {
        Ok(capabilities) => {
            let policy_requirements = policy_requirements_for_node(state, node.id).await?;
            let policy_requirements = policy_requirements
                .iter()
                .map(|(policy_id, options)| (*policy_id, options))
                .collect::<Vec<_>>();
            let resolver = RemoteCapabilityResolver::from_capabilities(node.id, capabilities);
            match resolver
                .ensure_binding_policy_options_supported(&node.name, policy_requirements.as_slice())
            {
                Ok(()) => (
                    serialize_capabilities(resolver.capabilities()),
                    String::new(),
                    None,
                ),
                Err(error) => {
                    tracing::warn!(
                        remote_node_id = node.id,
                        remote_node_name = %node.name,
                        protocol_version = %resolver.capabilities().protocol_version,
                        min_supported_protocol_version = %resolver.capabilities().min_supported_protocol_version,
                        "remote storage protocol compatibility check failed during probe: {error}"
                    );
                    (
                        serialize_capabilities(resolver.capabilities()),
                        error.message().to_string(),
                        Some(error),
                    )
                }
            }
        }
        Err(error) => (
            node.last_capabilities.clone(),
            error.message().to_string(),
            Some(error),
        ),
    };
    let model = managed_follower_repo::touch_probe_result(
        state.writer_db(),
        node.id,
        last_capabilities,
        last_error,
        Some(Utc::now()),
    )
    .await?;
    state
        .driver_registry()
        .reload_managed_followers(state.writer_db())
        .await?;
    state.driver_registry().invalidate_all();

    Ok(ProbedRemoteNode { model, probe_error })
}

async fn run_health_test_for_node<S: RemoteProtocolRuntimeState>(
    state: &S,
    node: managed_follower::Model,
    enrollment_status: RemoteNodeEnrollmentStatus,
) -> Result<RemoteNodeHealthTestOutcome> {
    if !node.is_enabled {
        return Ok(RemoteNodeHealthTestOutcome::Skipped);
    }

    if enrollment_status != RemoteNodeEnrollmentStatus::Completed {
        return Ok(RemoteNodeHealthTestOutcome::Skipped);
    }

    if node.transport_mode == RemoteNodeTransportMode::Direct && node.base_url.trim().is_empty() {
        return Ok(RemoteNodeHealthTestOutcome::Skipped);
    }

    if let Err(error) =
        sync_remote_binding_config_with_timeout(state, &node, REMOTE_BINDING_SYNC_TIMEOUT).await
    {
        tracing::warn!(
            remote_node_id = node.id,
            "failed to sync remote binding config during health test: {error}"
        );
    }

    let probed = probe_and_persist_node(state, &node).await?;
    Ok(if probed.probe_error.is_none() {
        RemoteNodeHealthTestOutcome::Healthy
    } else {
        RemoteNodeHealthTestOutcome::Failed
    })
}

fn normalize_create_input(input: CreateRemoteNodeInput) -> Result<CreateRemoteNodeInput> {
    Ok(CreateRemoteNodeInput {
        name: normalize_non_blank("name", &input.name)?,
        base_url: normalize_remote_base_url(&input.base_url)?,
        transport_mode: input.transport_mode,
        is_enabled: input.is_enabled,
    })
}

fn generate_managed_credentials() -> (String, String) {
    (
        format!("rn_{}", aster_forge_utils::id::new_short_token()),
        format!(
            "rns_{}{}",
            aster_forge_utils::id::new_short_token(),
            aster_forge_utils::id::new_short_token()
        ),
    )
}

fn normalize_update_input(input: UpdateRemoteNodeInput) -> Result<UpdateRemoteNodeInput> {
    Ok(UpdateRemoteNodeInput {
        name: input
            .name
            .as_deref()
            .map(|value| normalize_non_blank("name", value))
            .transpose()?,
        base_url: input
            .base_url
            .as_deref()
            .map(normalize_remote_base_url)
            .transpose()?,
        transport_mode: input.transport_mode,
        is_enabled: input.is_enabled,
    })
}

fn normalize_non_blank(field: &str, value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(format!(
            "{field} cannot be blank"
        )));
    }
    Ok(trimmed.to_string())
}

async fn refresh_registry<S: RemoteProtocolRuntimeState>(state: &S) -> Result<()> {
    state.policy_snapshot().reload(state.writer_db()).await?;
    state
        .driver_registry()
        .reload_managed_followers(state.writer_db())
        .await?;
    state.driver_registry().invalidate_all();
    Ok(())
}

async fn sync_remote_binding_config<S: RemoteProtocolRuntimeState>(
    state: &S,
    node: &managed_follower::Model,
) -> Result<()> {
    if node.transport_mode.requires_direct_base_url() && node.base_url.trim().is_empty() {
        return Ok(());
    }

    let client = remote_storage_client_for_node(state, node)?;
    client
        .sync_binding(&RemoteBindingSyncRequest {
            name: node.name.clone(),
            is_enabled: node.is_enabled,
        })
        .await
}

async fn sync_remote_binding_config_with_timeout<S: RemoteProtocolRuntimeState>(
    state: &S,
    node: &managed_follower::Model,
    timeout: Duration,
) -> Result<()> {
    tokio::time::timeout(timeout, sync_remote_binding_config(state, node))
        .await
        .map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!(
                    "sync remote binding config timed out after {}s",
                    timeout.as_secs()
                ),
            )
        })?
}

fn map_remote_node_db_err(error: DbErr) -> AsterError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        validation_error_with_code(
            ApiErrorCode::RemoteNodeUniqueConflict,
            "remote node unique field conflict",
        )
    } else {
        AsterError::from(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CreateRemoteNodeInput, UpdateRemoteNodeInput, generate_managed_credentials,
        normalize_create_input, normalize_update_input,
    };
    use crate::types::RemoteNodeTransportMode;

    #[test]
    fn normalize_create_input_ignores_managed_credentials() {
        let normalized = normalize_create_input(CreateRemoteNodeInput {
            name: " Edge ".to_string(),
            base_url: " https://remote.example.com/ ".to_string(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        })
        .unwrap();

        assert_eq!(normalized.name, "Edge");
        assert_eq!(normalized.base_url, "https://remote.example.com");
        assert!(normalized.is_enabled);
    }

    #[test]
    fn generate_managed_credentials_returns_prefixed_values() {
        let (access_key, secret_key) = generate_managed_credentials();

        assert!(access_key.starts_with("rn_"));
        assert!(secret_key.starts_with("rns_"));
        assert!(access_key.len() > 3);
        assert!(secret_key.len() > 4);
    }

    #[test]
    fn normalize_update_input_preserves_non_credential_fields() {
        let normalized = normalize_update_input(UpdateRemoteNodeInput {
            name: Some(" Edge ".to_string()),
            base_url: Some(" https://remote.example.com/ ".to_string()),
            transport_mode: None,
            is_enabled: Some(true),
        })
        .unwrap();

        assert_eq!(normalized.name.as_deref(), Some("Edge"));
        assert_eq!(
            normalized.base_url.as_deref(),
            Some("https://remote.example.com")
        );
        assert_eq!(normalized.is_enabled, Some(true));
    }
}
