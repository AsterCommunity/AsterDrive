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
    let kind = if session.status == UploadSessionStatus::Presigned {
        compatibility_presigned_kind(transport, session.object_multipart_id.is_some())
    } else if session.object_multipart_id.is_some() {
        compatibility_relay_kind(transport)?
    } else if staging::exists(state, &session.id).await? {
        compatibility_staging_kind(transport)?
    } else {
        UploadSessionKind::LegacyChunkFiles
    };

    validate_persisted_kind(session, kind)
}

fn compatibility_presigned_kind(
    transport: PolicyUploadTransport,
    has_multipart_id: bool,
) -> UploadSessionKind {
    match (transport, has_multipart_id) {
        (PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned), true) => {
            UploadSessionKind::ProviderPresignedMultipart
        }
        (PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned), true) => {
            UploadSessionKind::RemotePresignedMultipart
        }
        (PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned), false) => {
            UploadSessionKind::ProviderPresignedSingle
        }
        (PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned), false) => {
            UploadSessionKind::RemotePresignedSingle
        }
        // Old rows may outlive a policy snapshot change. Presigned status plus a multipart
        // id remains the compatibility marker, so keep provider as the conservative default.
        (_, true) => UploadSessionKind::ProviderPresignedMultipart,
        (_, false) => UploadSessionKind::ProviderPresignedSingle,
    }
}

fn compatibility_relay_kind(transport: PolicyUploadTransport) -> Result<UploadSessionKind> {
    match transport {
        PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::RelayStream) => {
            Ok(UploadSessionKind::ProviderRelayMultipart)
        }
        PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream) => {
            Ok(UploadSessionKind::RemoteRelayMultipart)
        }
        _ => Err(corrupted(
            "relay multipart session has incompatible upload transport",
        )),
    }
}

fn compatibility_staging_kind(transport: PolicyUploadTransport) -> Result<UploadSessionKind> {
    match transport {
        PolicyUploadTransport::Local => Ok(UploadSessionKind::OffsetStaging),
        PolicyUploadTransport::StreamUpload | PolicyUploadTransport::Sftp => {
            Ok(UploadSessionKind::StreamStaging)
        }
        PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::RelayStream)
        | PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream) => {
            Ok(UploadSessionKind::StreamStaging)
        }
        _ => Err(corrupted(
            "local staging session has incompatible upload transport",
        )),
    }
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

#[cfg(test)]
mod tests {
    use super::{
        compatibility_presigned_kind, compatibility_relay_kind, compatibility_staging_kind,
        mode_for_kind, validate_persisted_kind,
    };
    use crate::entities::upload_session;
    use crate::services::workspace::storage::PolicyUploadTransport;
    use crate::types::{ObjectStorageUploadStrategy, RemoteUploadStrategy};
    use crate::types::{UploadMode, UploadSessionKind, UploadSessionStatus};

    fn session(
        object_temp_key: Option<&str>,
        object_multipart_id: Option<&str>,
    ) -> upload_session::Model {
        let now = chrono::Utc::now();
        upload_session::Model {
            id: "kind-test".to_string(),
            user_id: 1,
            team_id: None,
            frontend_client_id: None,
            filename: "kind-test.bin".to_string(),
            total_size: 10,
            chunk_size: 5,
            total_chunks: 2,
            received_count: 0,
            folder_id: None,
            policy_id: 1,
            status: UploadSessionStatus::Uploading,
            session_kind: None,
            object_temp_key: object_temp_key.map(str::to_string),
            object_multipart_id: object_multipart_id.map(str::to_string),
            file_id: None,
            created_at: now,
            expires_at: now + chrono::Duration::hours(1),
            updated_at: now,
        }
    }

    #[test]
    fn mode_for_kind_covers_presigned_and_chunked_data_planes() {
        assert_eq!(
            mode_for_kind(UploadSessionKind::ProviderPresignedSingle),
            UploadMode::Presigned
        );
        assert_eq!(
            mode_for_kind(UploadSessionKind::RemotePresignedSingle),
            UploadMode::Presigned
        );
        assert_eq!(
            mode_for_kind(UploadSessionKind::ProviderPresignedMultipart),
            UploadMode::PresignedMultipart
        );
        assert_eq!(
            mode_for_kind(UploadSessionKind::RemotePresignedMultipart),
            UploadMode::PresignedMultipart
        );
        assert_eq!(
            mode_for_kind(UploadSessionKind::OffsetStaging),
            UploadMode::Chunked
        );
    }

    #[test]
    fn persisted_kind_validation_rejects_each_missing_provider_field() {
        assert!(
            validate_persisted_kind(
                &session(Some("files/temp"), None),
                UploadSessionKind::ProviderRelayMultipart,
            )
            .is_err()
        );
        assert!(
            validate_persisted_kind(
                &session(None, Some("multipart")),
                UploadSessionKind::ProviderRelayMultipart,
            )
            .is_err()
        );
        assert!(
            validate_persisted_kind(
                &session(None, None),
                UploadSessionKind::ProviderPresignedSingle,
            )
            .is_err()
        );
    }

    #[test]
    fn compatibility_presigned_kind_distinguishes_provider_and_remote_variants() {
        assert_eq!(
            compatibility_presigned_kind(
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
                false,
            ),
            UploadSessionKind::ProviderPresignedSingle
        );
        assert_eq!(
            compatibility_presigned_kind(
                PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
                true,
            ),
            UploadSessionKind::ProviderPresignedMultipart
        );
        assert_eq!(
            compatibility_presigned_kind(
                PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned),
                false,
            ),
            UploadSessionKind::RemotePresignedSingle
        );
        assert_eq!(
            compatibility_presigned_kind(
                PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned),
                true,
            ),
            UploadSessionKind::RemotePresignedMultipart
        );
        assert_eq!(
            compatibility_presigned_kind(PolicyUploadTransport::Local, true),
            UploadSessionKind::ProviderPresignedMultipart
        );
        assert_eq!(
            compatibility_presigned_kind(PolicyUploadTransport::Local, false),
            UploadSessionKind::ProviderPresignedSingle
        );
    }

    #[test]
    fn compatibility_relay_kind_rejects_non_relay_transport() {
        assert_eq!(
            compatibility_relay_kind(PolicyUploadTransport::ObjectStorage(
                ObjectStorageUploadStrategy::RelayStream,
            ))
            .unwrap(),
            UploadSessionKind::ProviderRelayMultipart
        );
        assert_eq!(
            compatibility_relay_kind(PolicyUploadTransport::Remote(
                RemoteUploadStrategy::RelayStream,
            ))
            .unwrap(),
            UploadSessionKind::RemoteRelayMultipart
        );
        assert!(compatibility_relay_kind(PolicyUploadTransport::Local).is_err());
        assert!(compatibility_relay_kind(PolicyUploadTransport::StreamUpload).is_err());
    }

    #[test]
    fn compatibility_staging_kind_covers_local_stream_and_relay_transports() {
        assert_eq!(
            compatibility_staging_kind(PolicyUploadTransport::Local).unwrap(),
            UploadSessionKind::OffsetStaging
        );
        for transport in [
            PolicyUploadTransport::StreamUpload,
            PolicyUploadTransport::Sftp,
            PolicyUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::RelayStream),
            PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream),
        ] {
            assert_eq!(
                compatibility_staging_kind(transport).unwrap(),
                UploadSessionKind::StreamStaging
            );
        }
        assert!(
            compatibility_staging_kind(PolicyUploadTransport::ObjectStorage(
                ObjectStorageUploadStrategy::Presigned,
            ))
            .is_err()
        );
        assert!(
            compatibility_staging_kind(PolicyUploadTransport::Remote(
                RemoteUploadStrategy::Presigned,
            ))
            .is_err()
        );
    }
}
