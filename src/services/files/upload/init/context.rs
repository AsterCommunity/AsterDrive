use chrono::{DateTime, Utc};
use sea_orm::Set;

use crate::db::repository::upload_session_repo;
use crate::entities::{storage_policy, upload_session};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::upload::responses::InitUploadResponse;
use crate::services::files::upload::shared::{
    UniqueUuidAttempt, abort_created_multipart_upload_after_init_error, with_unique_upload_id,
};
use crate::services::workspace::storage::{self, PolicyUploadTransport, WorkspaceStorageScope};
use crate::storage::MultipartStorageDriver;
use crate::types::{
    ObjectStorageUploadStrategy, ProviderResumableUploadStrategy, RemoteUploadStrategy, UploadMode,
    UploadSessionKind, UploadSessionStatus,
};

#[derive(Debug)]
pub(super) struct ResolvedUploadTarget {
    pub(super) folder_id: Option<i64>,
    pub(super) folder: Option<storage::VerifiedFolderPolicyHint>,
    pub(super) filename: String,
}

pub(super) struct InitUploadContext {
    pub(super) scope: WorkspaceStorageScope,
    pub(super) target: ResolvedUploadTarget,
    pub(super) total_size: i64,
    pub(super) policy: storage_policy::Model,
    pub(super) frontend_client_id: Option<String>,
}

pub(super) struct UploadSessionRecordParams<'a> {
    pub(super) upload_id: &'a str,
    pub(super) scope: WorkspaceStorageScope,
    pub(super) filename: &'a str,
    pub(super) total_size: i64,
    pub(super) chunk_size: i64,
    pub(super) total_chunks: i32,
    pub(super) folder_id: Option<i64>,
    pub(super) policy_id: i64,
    pub(super) frontend_client_id: Option<&'a str>,
    pub(super) status: UploadSessionStatus,
    pub(super) session_kind: UploadSessionKind,
    pub(super) object_temp_key: Option<&'a str>,
    pub(super) object_multipart_id: Option<&'a str>,
    pub(super) provider_session_ciphertext: Option<&'a str>,
    pub(super) expires_at: DateTime<Utc>,
}

pub(super) struct MultipartSessionInitParams {
    pub(super) mode: UploadMode,
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
/// This is intentionally expressed in terms of `PolicyUploadTransport`, not `DriverType`: a
/// connector may expose the same driver through different upload strategies, and the strategy is
/// what determines the session lifecycle and cleanup contract.
pub(super) fn session_kind_for_transport(
    transport: PolicyUploadTransport,
    mode: UploadMode,
) -> Result<UploadSessionKind> {
    let kind = match (transport, mode) {
        (PolicyUploadTransport::Local, UploadMode::Chunked) => UploadSessionKind::OffsetStaging,
        (
            PolicyUploadTransport::ProviderResumable(ProviderResumableUploadStrategy::ServerRelay)
            | PolicyUploadTransport::Sftp,
            UploadMode::Chunked,
        ) => UploadSessionKind::StreamStaging,
        (
            PolicyUploadTransport::ProviderResumable(
                ProviderResumableUploadStrategy::FrontendDirect,
            ),
            UploadMode::ProviderResumable,
        ) => UploadSessionKind::ProviderDirectResumable,
        (
            PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::RelayStream),
            UploadMode::Chunked,
        ) => UploadSessionKind::ProviderRelayMultipart,
        (
            PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
            UploadMode::Presigned,
        ) => UploadSessionKind::ProviderPresignedSingle,
        (
            PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
            UploadMode::PresignedMultipart,
        ) => UploadSessionKind::ProviderPresignedMultipart,
        (PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream), UploadMode::Chunked) => {
            UploadSessionKind::RemoteRelayMultipart
        }
        (PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned), UploadMode::Presigned) => {
            UploadSessionKind::RemotePresignedSingle
        }
        (
            PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned),
            UploadMode::PresignedMultipart,
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
    filename: &str,
    total_size: i64,
    folder_id: Option<i64>,
    relative_path: Option<&str>,
    frontend_client_id: Option<&str>,
) -> Result<InitUploadContext> {
    let target = resolve_upload_target(state, scope, filename, folder_id, relative_path).await?;

    tracing::debug!(
        scope = ?scope,
        folder_id = target.folder_id,
        filename = %target.filename,
        "resolved upload session target"
    );

    let policy = resolve_init_upload_policy(state, scope, target.folder, total_size).await?;

    tracing::debug!(
        scope = ?scope,
        policy_id = policy.id,
        driver_type = ?policy.driver_type,
        chunk_size = policy.chunk_size,
        total_size,
        "resolved upload storage policy"
    );

    Ok(InitUploadContext {
        scope,
        target,
        total_size,
        policy,
        frontend_client_id: frontend_client_id.map(str::to_string),
    })
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
            // 目录上传会把 `relative_path` 拆成“父目录链 + 最终文件名”。
            // 这里就把目录路径补齐，后续模式选择和 session 记录都只看解析后的最终目标。
            let parsed = storage::parse_relative_upload_path(state, scope, folder_id, path).await?;
            let actor_username = if parsed.parent_segments.is_empty() {
                None
            } else {
                Some(storage::load_scope_actor_username_cached(state, scope).await?)
            };
            let resolved_parent = storage::ensure_upload_parent_path(
                state,
                scope,
                &parsed,
                actor_username.as_deref(),
            )
            .await?;
            Ok(ResolvedUploadTarget {
                folder_id: resolved_parent.folder_id,
                folder: resolved_parent.folder,
                filename: parsed.filename,
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
            })
        }
    }
}

async fn resolve_init_upload_policy(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder: Option<storage::VerifiedFolderPolicyHint>,
    total_size: i64,
) -> Result<storage_policy::Model> {
    if total_size < 0 {
        return Err(AsterError::validation_error(
            "total_size cannot be negative",
        ));
    }

    // upload 模式协商建立在“最终会写到哪条策略”之上，而不是客户端自己传 mode。
    let policy =
        storage::resolve_policy_for_size_with_verified_folder(state, scope, folder, total_size)
            .await?;
    validate_policy_upload_size(&policy, total_size)?;
    storage::check_quota(state.writer_db(), scope, total_size).await?;
    Ok(policy)
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
                total_size: ctx.total_size,
                chunk_size,
                total_chunks,
                folder_id: ctx.target.folder_id,
                policy_id: ctx.policy.id,
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
        total_size,
        chunk_size,
        total_chunks,
        folder_id,
        policy_id,
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
        total_size: Set(total_size),
        chunk_size: Set(chunk_size),
        total_chunks: Set(total_chunks),
        received_count: Set(0),
        folder_id: Set(folder_id),
        policy_id: Set(policy_id),
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

pub(super) fn direct_upload_response() -> InitUploadResponse {
    InitUploadResponse {
        mode: UploadMode::Direct,
        upload_id: None,
        chunk_size: None,
        total_chunks: None,
        presigned_url: None,
        presigned_headers: Default::default(),
        presigned_require_etag: None,
        provider_resumable: None,
    }
}

pub(super) fn chunked_upload_response(
    mode: UploadMode,
    upload_id: String,
    chunk_size: i64,
    total_chunks: i32,
) -> InitUploadResponse {
    InitUploadResponse {
        mode,
        upload_id: Some(upload_id),
        chunk_size: Some(chunk_size),
        total_chunks: Some(total_chunks),
        presigned_url: None,
        presigned_headers: Default::default(),
        presigned_require_etag: None,
        provider_resumable: None,
    }
}

#[cfg(test)]
mod tests {
    use super::session_kind_for_transport;
    use crate::services::workspace::storage::PolicyUploadTransport;
    use crate::types::{
        ObjectStorageUploadStrategy, ProviderResumableUploadStrategy, RemoteUploadStrategy,
        UploadMode, UploadSessionKind,
    };

    #[test]
    fn session_kind_mapping_covers_each_connector_transport() {
        let cases = [
            (
                PolicyUploadTransport::Local,
                UploadMode::Chunked,
                UploadSessionKind::OffsetStaging,
            ),
            (
                PolicyUploadTransport::ProviderResumable(
                    ProviderResumableUploadStrategy::ServerRelay,
                ),
                UploadMode::Chunked,
                UploadSessionKind::StreamStaging,
            ),
            (
                PolicyUploadTransport::ProviderResumable(
                    ProviderResumableUploadStrategy::FrontendDirect,
                ),
                UploadMode::ProviderResumable,
                UploadSessionKind::ProviderDirectResumable,
            ),
            (
                PolicyUploadTransport::Sftp,
                UploadMode::Chunked,
                UploadSessionKind::StreamStaging,
            ),
            (
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::RelayStream),
                UploadMode::Chunked,
                UploadSessionKind::ProviderRelayMultipart,
            ),
            (
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
                UploadMode::Presigned,
                UploadSessionKind::ProviderPresignedSingle,
            ),
            (
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
                UploadMode::PresignedMultipart,
                UploadSessionKind::ProviderPresignedMultipart,
            ),
            (
                PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream),
                UploadMode::Chunked,
                UploadSessionKind::RemoteRelayMultipart,
            ),
            (
                PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned),
                UploadMode::Presigned,
                UploadSessionKind::RemotePresignedSingle,
            ),
            (
                PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned),
                UploadMode::PresignedMultipart,
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
            (PolicyUploadTransport::Local, UploadMode::Direct),
            (
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
                UploadMode::Chunked,
            ),
            (
                PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream),
                UploadMode::Presigned,
            ),
            (
                PolicyUploadTransport::ProviderResumable(
                    ProviderResumableUploadStrategy::FrontendDirect,
                ),
                UploadMode::Chunked,
            ),
        ];
        for (transport, mode) in invalid {
            assert!(session_kind_for_transport(transport, mode).is_err());
        }
    }
}
