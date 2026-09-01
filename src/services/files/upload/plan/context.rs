use chrono::{DateTime, Duration, Utc};
use sea_orm::Set;

use crate::db::repository::upload_session_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::files::upload::session::responses::InitUploadResponse;
use crate::services::files::upload::session::shared::{
    UniqueUuidAttempt, abort_created_multipart_upload_after_init_error, with_unique_upload_id,
};
use crate::services::storage_policy::policy::placement::StorageRoutingDecision;
use crate::services::workspace::storage::{self, PolicyUploadTransport, WorkspaceStorageScope};
use aster_drive_model::entities::{storage_policy, upload_session};
use aster_drive_model::types::{
    ObjectStorageUploadStrategy, ProviderResumableUploadStrategy, RemoteUploadStrategy,
    UploadSessionKind, UploadSessionStatus, UploadTransport,
};
use aster_drive_storage::MultipartStorageDriver;

#[derive(Debug)]
pub(super) struct ResolvedUploadTarget {
    pub(super) folder_id: Option<i64>,
    pub(super) folder: Option<storage::VerifiedFolderPolicyHint>,
    pub(super) filename: String,
    pending_relative_path: Option<storage::ParsedUploadPath>,
}

pub(super) struct InitUploadContext {
    pub(super) scope: WorkspaceStorageScope,
    pub(super) target: ResolvedUploadTarget,
    pub(super) total_size: i64,
    pub(super) mime_type: String,
    pub(super) policy: storage_policy::Model,
    pub(super) routing_decision: StorageRoutingDecision,
    pub(super) frontend_client_id: Option<String>,
}

pub(super) struct UploadSessionRecordParams<'a> {
    pub(super) upload_id: &'a str,
    pub(super) scope: WorkspaceStorageScope,
    pub(super) filename: &'a str,
    pub(super) mime_type: &'a str,
    pub(super) total_size: i64,
    pub(super) chunk_size: i64,
    pub(super) total_chunks: i32,
    pub(super) folder_id: Option<i64>,
    pub(super) policy_id: i64,
    pub(super) placement_profile_id: Option<i64>,
    pub(super) placement_rule_id: Option<i64>,
    pub(super) placement_revision: Option<i64>,
    pub(super) placement_execution_preference: &'a str,
    pub(super) frontend_client_id: Option<&'a str>,
    pub(super) status: UploadSessionStatus,
    pub(super) session_kind: UploadSessionKind,
    pub(super) object_temp_key: Option<&'a str>,
    pub(super) object_multipart_id: Option<&'a str>,
    pub(super) provider_session_ciphertext: Option<&'a str>,
    pub(super) expires_at: DateTime<Utc>,
}

pub(super) struct MultipartSessionInitParams {
    pub(super) mode: UploadTransport,
    pub(super) status: UploadSessionStatus,
    pub(super) session_kind: UploadSessionKind,
    pub(super) chunk_size: i64,
    pub(super) total_chunks: i32,
    pub(super) expires_in: chrono::Duration,
    pub(super) log_label: &'static str,
    pub(super) abort_db_error_context: &'static str,
    pub(super) abort_db_error_message: &'static str,
    pub(super) abort_collision_context: &'static str,
}

/// Resolves the persisted data plane from connector-owned transport semantics.
///
/// This is intentionally expressed in terms of `PolicyUploadTransport`, not a provider enum: a
/// connector may expose the same driver through different upload strategies, and the strategy is
/// what determines the session lifecycle and cleanup contract.
pub(super) fn session_kind_for_transport(
    transport: PolicyUploadTransport,
    mode: UploadTransport,
) -> Result<UploadSessionKind> {
    let kind = match (transport, mode) {
        (PolicyUploadTransport::Local, UploadTransport::Chunked) => {
            UploadSessionKind::OffsetStaging
        }
        (
            PolicyUploadTransport::ProviderResumable(ProviderResumableUploadStrategy::ServerRelay),
            UploadTransport::Chunked,
        ) => UploadSessionKind::ProviderRelayResumable,
        (PolicyUploadTransport::Sftp, UploadTransport::Chunked) => UploadSessionKind::StreamStaging,
        (
            PolicyUploadTransport::ProviderResumable(
                ProviderResumableUploadStrategy::FrontendDirect,
            ),
            UploadTransport::ProviderResumable,
        ) => UploadSessionKind::ProviderDirectResumable,
        (
            PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::RelayStream),
            UploadTransport::Chunked,
        ) => UploadSessionKind::ProviderRelayMultipart,
        (
            PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
            UploadTransport::Presigned,
        ) => UploadSessionKind::ProviderPresignedSingle,
        (
            PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
            UploadTransport::PresignedMultipart,
        ) => UploadSessionKind::ProviderPresignedMultipart,
        (
            PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream),
            UploadTransport::Chunked,
        ) => UploadSessionKind::RemoteRelayMultipart,
        (
            PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned),
            UploadTransport::Presigned,
        ) => UploadSessionKind::RemotePresignedSingle,
        (
            PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned),
            UploadTransport::PresignedMultipart,
        ) => UploadSessionKind::RemotePresignedMultipart,
        _ => {
            return Err(AsterError::validation_error(format!(
                "upload transport {transport:?} cannot initialize mode {mode:?}"
            )));
        }
    };
    Ok(kind)
}

pub(super) async fn resolve_init_upload_context(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    params: super::InitUploadParams<'_>,
) -> Result<InitUploadContext> {
    let super::InitUploadParams {
        filename,
        total_size,
        folder_id,
        relative_path,
        mime_type: declared_mime_type,
        frontend_client_id,
    } = params;
    if total_size < 0 {
        return Err(AsterError::validation_error(
            "total_size cannot be negative",
        ));
    }
    let target = resolve_upload_target(state, scope, filename, folder_id, relative_path).await?;
    let mime_type = resolve_upload_mime_type(&target.filename, declared_mime_type)?;

    tracing::debug!(
        scope = ?scope,
        folder_id = target.folder_id,
        filename = %target.filename,
        "resolved upload session target"
    );

    let resolution = storage::resolve_blob_policy_for_write(
        state,
        storage::BlobPolicyRequest {
            scope,
            folder_id: target.folder_id,
            folder_hint: target.folder,
            filename: &target.filename,
            file_size: total_size,
            mime_type: &mime_type,
            existing_file_id: None,
        },
    )
    .await?;
    let policy = resolution.policy;
    let routing_decision = resolution.routing_decision.ok_or_else(|| {
        AsterError::storage_policy_not_found("new blob placement decision is missing")
    })?;
    validate_policy_upload_size(&policy, total_size)?;
    storage::check_quota(state.writer_db(), scope, total_size).await?;
    tracing::debug!(
        scope = ?scope,
        policy_id = policy.id,
        connector_id = %policy.connector_id,
        chunk_size = policy.chunk_size,
        total_size,
        mime_type,
        "resolved upload storage policy"
    );

    Ok(InitUploadContext {
        scope,
        target,
        total_size,
        mime_type,
        policy,
        routing_decision,
        frontend_client_id: frontend_client_id.map(str::to_string),
    })
}

fn resolve_upload_mime_type(filename: &str, declared: Option<&str>) -> Result<String> {
    if let Some(value) = declared {
        return super::mime::normalize_upload_mime_type(value);
    }
    Ok(mime_guess::from_path(filename)
        .first_or_octet_stream()
        .essence_str()
        .to_string())
}

pub(super) async fn validate_storage_capacity(
    state: &PrimaryAppState,
    policy: &storage_policy::Model,
    total_size: i64,
) -> Result<()> {
    let capacity = match state
        .driver_registry()
        .get_driver(policy)?
        .capacity_info()
        .await
    {
        Ok(capacity) => capacity,
        Err(error) => {
            tracing::warn!(
                policy_id = policy.id,
                error = %error,
                "storage capacity preflight unavailable; continuing without reservation"
            );
            return Ok(());
        }
    };
    if capacity.status == aster_drive_storage::traits::extensions::StorageCapacityStatus::Supported
        && capacity
            .available_bytes
            .is_some_and(|available| available < total_size)
    {
        return Err(AsterError::storage_driver_error(format!(
            "storage policy #{} has insufficient capacity for a {total_size} byte upload",
            policy.id
        )));
    }
    Ok(())
}

async fn resolve_upload_target(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    filename: &str,
    folder_id: Option<i64>,
    relative_path: Option<&str>,
) -> Result<ResolvedUploadTarget> {
    match relative_path {
        Some(path) => {
            let parsed = storage::parse_relative_upload_path(state, scope, folder_id, path).await?;
            let resolved_parent =
                storage::resolve_existing_upload_parent(state, scope, &parsed).await?;
            Ok(ResolvedUploadTarget {
                folder_id: resolved_parent.folder_id,
                folder: resolved_parent.folder,
                filename: parsed.filename.clone(),
                pending_relative_path: Some(parsed),
            })
        }
        None => {
            let filename = aster_forge_validation::filename::normalize_validate_name(filename)?;
            let folder = match folder_id {
                Some(folder_id) => {
                    let folder = storage::verify_folder_access(state, scope, folder_id).await?;
                    Some(storage::resolve_verified_folder_policy_hint(state, scope, folder).await?)
                }
                None => None,
            };
            Ok(ResolvedUploadTarget {
                folder_id,
                folder,
                filename,
                pending_relative_path: None,
            })
        }
    }
}

pub(super) async fn materialize_upload_target(
    state: &PrimaryAppState,
    ctx: &mut InitUploadContext,
) -> Result<()> {
    let Some(parsed) = ctx.target.pending_relative_path.take() else {
        return Ok(());
    };
    let actor_username = if parsed.parent_segments.is_empty() {
        None
    } else {
        Some(storage::load_scope_actor_username_cached(state, ctx.scope).await?)
    };
    let resolved =
        storage::ensure_upload_parent_path(state, ctx.scope, &parsed, actor_username.as_deref())
            .await?;
    ctx.target.folder_id = resolved.folder_id;
    ctx.target.folder = resolved.folder;
    Ok(())
}

fn validate_policy_upload_size(policy: &storage_policy::Model, total_size: i64) -> Result<()> {
    if policy.max_file_size > 0 && total_size > policy.max_file_size {
        return Err(AsterError::file_too_large(format!(
            "file size {} exceeds limit {}",
            total_size, policy.max_file_size
        )));
    }
    Ok(())
}

pub(super) async fn try_persist_upload_session(
    db: &sea_orm::DatabaseConnection,
    params: UploadSessionRecordParams<'_>,
) -> Result<bool> {
    let session = upload_session_active_model(params);
    upload_session_repo::try_create(db, session).await
}

pub(super) async fn init_multipart_session_with_retry(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
    multipart: &dyn MultipartStorageDriver,
    params: MultipartSessionInitParams,
) -> Result<InitUploadResponse> {
    let MultipartSessionInitParams {
        mode,
        status,
        session_kind,
        chunk_size,
        total_chunks,
        expires_in,
        log_label,
        abort_db_error_context,
        abort_db_error_message,
        abort_collision_context,
    } = params;

    with_unique_upload_id(|upload_id| async {
        let temp_key = format!("files/{upload_id}");
        let multipart_id = multipart.create_multipart_upload(&temp_key).await?;
        let inserted_result = try_persist_upload_session(
            state.writer_db(),
            UploadSessionRecordParams {
                upload_id: &upload_id,
                scope: ctx.scope,
                filename: &ctx.target.filename,
                mime_type: &ctx.mime_type,
                total_size: ctx.total_size,
                chunk_size,
                total_chunks,
                folder_id: ctx.target.folder_id,
                policy_id: ctx.policy.id,
                placement_profile_id: Some(ctx.routing_decision.profile_id),
                placement_rule_id: ctx.routing_decision.rule_id,
                placement_revision: Some(ctx.routing_decision.revision),
                placement_execution_preference: ctx.routing_decision.execution_preference.as_str(),
                frontend_client_id: ctx.frontend_client_id.as_deref(),
                status,
                session_kind,
                object_temp_key: Some(&temp_key),
                object_multipart_id: Some(&multipart_id),
                provider_session_ciphertext: None,
                expires_at: Utc::now() + expires_in,
            },
        )
        .await;

        let inserted = match inserted_result {
            Ok(inserted) => inserted,
            Err(error) => {
                let abort_result = abort_created_multipart_upload_after_init_error(
                    multipart,
                    &temp_key,
                    &multipart_id,
                    &upload_id,
                    abort_db_error_context,
                )
                .await;
                if let Err(abort_error) = abort_result {
                    return Err(AsterError::storage_driver_error(format!(
                        "{abort_db_error_message}; init error={error}, abort error={abort_error}"
                    )));
                }
                return Err(error);
            }
        };

        if !inserted {
            abort_created_multipart_upload_after_init_error(
                multipart,
                &temp_key,
                &multipart_id,
                &upload_id,
                abort_collision_context,
            )
            .await?;
            return Ok(UniqueUuidAttempt::Collision);
        }

        tracing::debug!(
            scope = ?ctx.scope,
            upload_id = %upload_id,
            policy_id = ctx.policy.id,
            mode = ?mode,
            chunk_size,
            total_chunks,
            folder_id = ctx.target.folder_id,
            log_label = %log_label,
            "initialized upload session"
        );

        Ok(UniqueUuidAttempt::Accepted(chunked_upload_response(
            mode,
            upload_id,
            chunk_size,
            total_chunks,
            session_kind,
        )))
    })
    .await
}

fn upload_session_active_model(
    params: UploadSessionRecordParams<'_>,
) -> upload_session::ActiveModel {
    let UploadSessionRecordParams {
        upload_id,
        scope,
        filename,
        mime_type,
        total_size,
        chunk_size,
        total_chunks,
        folder_id,
        policy_id,
        placement_profile_id,
        placement_rule_id,
        placement_revision,
        placement_execution_preference,
        frontend_client_id,
        status,
        session_kind,
        object_temp_key,
        object_multipart_id,
        provider_session_ciphertext,
        expires_at,
    } = params;
    let now = Utc::now();

    upload_session::ActiveModel {
        id: Set(upload_id.to_string()),
        user_id: Set(scope.actor_user_id()),
        team_id: Set(scope.team_id()),
        frontend_client_id: Set(frontend_client_id.map(str::to_string)),
        filename: Set(filename.to_string()),
        mime_type: Set(mime_type.to_string()),
        total_size: Set(total_size),
        chunk_size: Set(chunk_size),
        total_chunks: Set(total_chunks),
        received_count: Set(0),
        folder_id: Set(folder_id),
        policy_id: Set(policy_id),
        placement_profile_id: Set(placement_profile_id),
        placement_rule_id: Set(placement_rule_id),
        placement_revision: Set(placement_revision),
        placement_execution_preference: Set(placement_execution_preference.to_string()),
        status: Set(status),
        session_kind: Set(session_kind),
        object_temp_key: Set(object_temp_key.map(str::to_string)),
        object_multipart_id: Set(object_multipart_id.map(str::to_string)),
        provider_session_ciphertext: Set(provider_session_ciphertext.map(str::to_string)),
        file_id: Set(None),
        created_at: Set(now),
        expires_at: Set(expires_at),
        updated_at: Set(now),
    }
}

pub(super) async fn init_stream_session(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
) -> Result<InitUploadResponse> {
    with_unique_upload_id(|upload_id| async move {
        let inserted = try_persist_upload_session(
            state.writer_db(),
            UploadSessionRecordParams {
                upload_id: &upload_id,
                scope: ctx.scope,
                filename: &ctx.target.filename,
                mime_type: &ctx.mime_type,
                total_size: ctx.total_size,
                chunk_size: 0,
                total_chunks: 0,
                folder_id: ctx.target.folder_id,
                policy_id: ctx.policy.id,
                placement_profile_id: Some(ctx.routing_decision.profile_id),
                placement_rule_id: ctx.routing_decision.rule_id,
                placement_revision: Some(ctx.routing_decision.revision),
                placement_execution_preference: ctx.routing_decision.execution_preference.as_str(),
                frontend_client_id: ctx.frontend_client_id.as_deref(),
                status: UploadSessionStatus::Uploading,
                session_kind: UploadSessionKind::Stream,
                object_temp_key: None,
                object_multipart_id: None,
                provider_session_ciphertext: None,
                expires_at: Utc::now() + Duration::hours(24),
            },
        )
        .await?;
        if !inserted {
            return Ok(UniqueUuidAttempt::Collision);
        }
        Ok(UniqueUuidAttempt::Accepted(InitUploadResponse {
            mode: UploadTransport::Stream,
            upload_id: Some(upload_id),
            chunk_size: None,
            total_chunks: None,
            presigned_request: None,
            presigned_require_etag: None,
            provider_resumable: None,
            upload_scheduling: None,
        }))
    })
    .await
}

pub(super) fn chunked_upload_response(
    mode: UploadTransport,
    upload_id: String,
    chunk_size: i64,
    total_chunks: i32,
    session_kind: UploadSessionKind,
) -> InitUploadResponse {
    InitUploadResponse {
        mode,
        upload_id: Some(upload_id),
        chunk_size: Some(chunk_size),
        total_chunks: Some(total_chunks),
        presigned_request: None,
        presigned_require_etag: None,
        provider_resumable: None,
        upload_scheduling: crate::services::files::upload::session::kind::scheduling_for_kind(
            session_kind,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::session_kind_for_transport;
    use crate::services::workspace::storage::PolicyUploadTransport;
    use aster_drive_model::types::{
        ObjectStorageUploadStrategy, ProviderResumableUploadStrategy, RemoteUploadStrategy,
        UploadSessionKind, UploadTransport,
    };

    #[test]
    fn session_kind_mapping_covers_each_connector_transport() {
        let cases = [
            (
                PolicyUploadTransport::Local,
                UploadTransport::Chunked,
                UploadSessionKind::OffsetStaging,
            ),
            (
                PolicyUploadTransport::ProviderResumable(
                    ProviderResumableUploadStrategy::ServerRelay,
                ),
                UploadTransport::Chunked,
                UploadSessionKind::ProviderRelayResumable,
            ),
            (
                PolicyUploadTransport::ProviderResumable(
                    ProviderResumableUploadStrategy::FrontendDirect,
                ),
                UploadTransport::ProviderResumable,
                UploadSessionKind::ProviderDirectResumable,
            ),
            (
                PolicyUploadTransport::Sftp,
                UploadTransport::Chunked,
                UploadSessionKind::StreamStaging,
            ),
            (
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::RelayStream),
                UploadTransport::Chunked,
                UploadSessionKind::ProviderRelayMultipart,
            ),
            (
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
                UploadTransport::Presigned,
                UploadSessionKind::ProviderPresignedSingle,
            ),
            (
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
                UploadTransport::PresignedMultipart,
                UploadSessionKind::ProviderPresignedMultipart,
            ),
            (
                PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream),
                UploadTransport::Chunked,
                UploadSessionKind::RemoteRelayMultipart,
            ),
            (
                PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned),
                UploadTransport::Presigned,
                UploadSessionKind::RemotePresignedSingle,
            ),
            (
                PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned),
                UploadTransport::PresignedMultipart,
                UploadSessionKind::RemotePresignedMultipart,
            ),
        ];

        for (transport, mode, expected) in cases {
            assert_eq!(
                session_kind_for_transport(transport, mode).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn session_kind_mapping_rejects_impossible_mode_combinations() {
        let invalid = [
            (PolicyUploadTransport::Local, UploadTransport::Stream),
            (
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
                UploadTransport::Chunked,
            ),
            (
                PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream),
                UploadTransport::Presigned,
            ),
            (
                PolicyUploadTransport::ProviderResumable(
                    ProviderResumableUploadStrategy::FrontendDirect,
                ),
                UploadTransport::Chunked,
            ),
        ];
        for (transport, mode) in invalid {
            assert!(session_kind_for_transport(transport, mode).is_err());
        }
    }
}
