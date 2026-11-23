use super::{
    InitUploadContext, PrimaryAppState, SharedRuntimeState, UploadTransport,
    upload_data_plane_label,
};
use crate::services::workspace::storage::PolicyUploadTransport;
use aster_drive_storage::StorageDriver;

/// Emit the admitted decision, including capacity-driven reselection, before allocation.
pub(super) fn log_upload_decision(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
    transport: PolicyUploadTransport,
    mode: UploadTransport,
    driver: &dyn StorageDriver,
) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    let decision = &ctx.routing_decision;
    let group = state
        .policy_snapshot()
        .get_policy_group(decision.profile_id);
    let profile = state
        .policy_snapshot()
        .get_placement_profile(decision.profile_id)
        .filter(|profile| profile.revision == decision.revision);
    let selected_target = profile.as_ref().and_then(|profile| {
        profile
            .rules
            .iter()
            .find(|rule| Some(rule.id) == decision.rule_id)
            .and_then(|rule| {
                rule.targets
                    .iter()
                    .find(|target| target.policy_id == decision.policy_id)
            })
    });
    let excluded_targets: Vec<_> = decision
        .excluded_targets
        .iter()
        .map(|(id, reason)| (*id, reason.code()))
        .collect();
    let extensions = driver.extensions();
    tracing::debug!(
        profile_id = decision.profile_id,
        profile_name = group.as_ref().map(|group| group.name.as_str()),
        revision = decision.revision,
        rule_id = decision.rule_id,
        policy_id = decision.policy_id,
        connector_id = %ctx.policy.connector_id,
        selection_mode = decision.selection_mode.as_str(),
        folder_override = decision.folder_override,
        execution_preference = decision.execution_preference.as_str(),
        evaluated_rules = ?decision.evaluated_rules,
        excluded_targets = ?excluded_targets,
        weight = selected_target.map(|target| target.weight),
        accepting_new_writes = selected_target.map(|target| target.accepting_new_writes),
        "resolved upload routing decision"
    );
    let binding = crate::storage::connectors::resolve_remote_policy_binding(
        state.driver_registry().connectors(),
        &ctx.policy,
    )
    .ok()
    .flatten()
    .and_then(|binding| binding.remote_node_id)
    .and_then(|id| state.driver_registry().get_managed_follower(id))
    .map(|node| {
        let transport = if node
            .transport_mode
            .resolves_to_reverse_tunnel(&node.base_url)
        {
            aster_drive_model::types::ResolvedRemoteTransport::ReverseTunnel
        } else {
            aster_drive_model::types::ResolvedRemoteTransport::Direct
        };
        (node.id, transport)
    });
    let chunk_size = transport.effective_chunk_size(&ctx.policy);
    let max_single_put_size = driver.max_single_put_size();
    let candidate_modes: &[&str] = match transport {
        PolicyUploadTransport::ObjectStorage(
            aster_drive_model::types::ObjectStorageUploadStrategy::Presigned,
        )
        | PolicyUploadTransport::Remote(
            aster_drive_model::types::RemoteUploadStrategy::Presigned,
        ) => &["presigned", "presigned_multipart"],
        PolicyUploadTransport::ProviderResumable(
            aster_drive_model::types::ProviderResumableUploadStrategy::FrontendDirect,
        ) => &["provider_resumable"],
        _ => &["stream", "chunked"],
    };
    tracing::debug!(
        profile_id = decision.profile_id,
        policy_id = decision.policy_id,
        connector_id = %ctx.policy.connector_id,
        execution_preference = decision.execution_preference.as_str(),
        transport = ?transport,
        candidate_modes = ?candidate_modes,
        remote_node_id = binding.map(|(id, _)| id),
        resolved_transport = binding.map(|(_, transport)| transport.as_str()),
        mode = mode.as_str(),
        data_plane = upload_data_plane_label(transport, mode),
        total_size = ctx.total_size,
        chunk_size,
        max_single_put_size = ?max_single_put_size,
        fits_chunk_size = chunk_size == 0 || ctx.total_size <= chunk_size,
        fits_single_put = max_single_put_size.is_none_or(|limit| u64::try_from(ctx.total_size).is_ok_and(|size| size <= limit)),
        supports_presigned_upload = extensions.presigned_upload.is_some(),
        supports_multipart = extensions.multipart.is_some(),
        supports_stream_upload = extensions.stream_upload.is_some(),
        supports_provider_resumable = extensions.provider_resumable.is_some(),
        "selected upload transport"
    );
}
