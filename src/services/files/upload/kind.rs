//! Upload session data-plane classification.
//!
//! New sessions persist their kind at init. Only pre-migration rows reach the compatibility
//! branch below; keeping that inference in one place prevents completion, progress and cleanup
//! from disagreeing about the same session.

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::upload_session;
use crate::errors::{Result, upload_assembly_error_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::files::upload::staging;
use crate::services::workspace::storage::{PolicyUploadTransport, resolve_policy_upload_transport};
use crate::types::{
    ObjectStorageUploadStrategy, RemoteUploadStrategy, UploadMode, UploadSessionKind,
    UploadSessionStatus,
};

pub(crate) async fn resolve_upload_session_kind(
    state: &impl SharedRuntimeState,
    session: &upload_session::Model,
) -> Result<UploadSessionKind> {
    if let Some(kind) = session.session_kind {
        return validate_persisted_kind(session, kind);
    }

    // Rows created before session_kind existed remain readable until 0.5.0. Their provider
    // fields are only a compatibility hint; local staging is identified by its dedicated path,
    // never by the legacy `assembled` output.
    let transport = resolve_policy_upload_transport_for_session(state, session)?;
    if session.status == UploadSessionStatus::Presigned {
        return Ok(match (transport, session.object_multipart_id.is_some()) {
            (
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
                true,
            ) => UploadSessionKind::ProviderPresignedMultipart,
            (PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned), true) => {
                UploadSessionKind::RemotePresignedMultipart
            }
            (
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
                false,
            ) => UploadSessionKind::ProviderPresignedSingle,
            (PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned), false) => {
                UploadSessionKind::RemotePresignedSingle
            }
            // Old rows may outlive a policy snapshot change. Presigned status plus a multipart
            // id remains the compatibility marker, so keep provider as the conservative default.
            (_, true) => UploadSessionKind::ProviderPresignedMultipart,
            (_, false) => UploadSessionKind::ProviderPresignedSingle,
        });
    }

    if session.object_multipart_id.is_some() {
        return Ok(match transport {
            PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::RelayStream) => {
                UploadSessionKind::ProviderRelayMultipart
            }
            PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream) => {
                UploadSessionKind::RemoteRelayMultipart
            }
            _ => {
                return Err(corrupted(
                    "relay multipart session has incompatible upload transport",
                ));
            }
        });
    }

    if staging::exists(state, &session.id).await? {
        return Ok(match transport {
            PolicyUploadTransport::Local => UploadSessionKind::OffsetStaging,
            PolicyUploadTransport::StreamUpload | PolicyUploadTransport::Sftp => {
                UploadSessionKind::StreamStaging
            }
            PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::RelayStream)
            | PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream) => {
                UploadSessionKind::StreamStaging
            }
            _ => {
                return Err(corrupted(
                    "local staging session has incompatible upload transport",
                ));
            }
        });
    }

    Ok(UploadSessionKind::LegacyChunkFiles)
}

fn resolve_policy_upload_transport_for_session(
    state: &impl SharedRuntimeState,
    session: &upload_session::Model,
) -> Result<PolicyUploadTransport> {
    let policy = state
        .policy_snapshot()
        .get_policy_or_err(session.policy_id)?;
    resolve_policy_upload_transport(&policy)
}

fn validate_persisted_kind(
    session: &upload_session::Model,
    kind: UploadSessionKind,
) -> Result<UploadSessionKind> {
    let has_multipart_id = session.object_multipart_id.is_some();
    let expects_multipart_id = matches!(
        kind,
        UploadSessionKind::ProviderRelayMultipart
            | UploadSessionKind::ProviderPresignedMultipart
            | UploadSessionKind::RemoteRelayMultipart
            | UploadSessionKind::RemotePresignedMultipart
    );
    if expects_multipart_id != has_multipart_id {
        return Err(corrupted(format!(
            "session kind {} does not match multipart fields",
            kind.as_str()
        )));
    }
    let expects_temp_key = matches!(
        kind,
        UploadSessionKind::ProviderRelayMultipart
            | UploadSessionKind::ProviderPresignedSingle
            | UploadSessionKind::ProviderPresignedMultipart
            | UploadSessionKind::RemoteRelayMultipart
            | UploadSessionKind::RemotePresignedSingle
            | UploadSessionKind::RemotePresignedMultipart
    );
    if expects_temp_key != session.object_temp_key.is_some() {
        return Err(corrupted(format!(
            "session kind {} does not match temporary object fields",
            kind.as_str()
        )));
    }
    Ok(kind)
}

fn corrupted(message: impl Into<String>) -> crate::errors::AsterError {
    upload_assembly_error_with_code(ApiErrorCode::UploadSessionCorrupted, message)
}

pub(crate) fn mode_for_kind(kind: UploadSessionKind) -> UploadMode {
    match kind {
        UploadSessionKind::ProviderPresignedSingle | UploadSessionKind::RemotePresignedSingle => {
            UploadMode::Presigned
        }
        UploadSessionKind::ProviderPresignedMultipart
        | UploadSessionKind::RemotePresignedMultipart => UploadMode::PresignedMultipart,
        _ => UploadMode::Chunked,
    }
}
