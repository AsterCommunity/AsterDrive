//! Upload session data-plane validation.

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::upload_session;
use crate::errors::{Result, upload_assembly_error_with_code};
use crate::types::{UploadMode, UploadSessionKind};

pub(crate) fn resolve_upload_session_kind(
    session: &upload_session::Model,
) -> Result<UploadSessionKind> {
    validate_persisted_kind(session, session.session_kind)
}

pub(crate) fn validate_persisted_kind(
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
            | UploadSessionKind::ProviderDirectResumable
    );
    if expects_temp_key != session.object_temp_key.is_some() {
        return Err(corrupted(format!(
            "session kind {} does not match temporary object fields",
            kind.as_str()
        )));
    }
    let expects_provider_session = kind == UploadSessionKind::ProviderDirectResumable;
    if expects_provider_session != session.provider_session_ciphertext.is_some() {
        return Err(corrupted(format!(
            "session kind {} does not match provider session metadata",
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
        UploadSessionKind::ProviderDirectResumable => UploadMode::ProviderResumable,
        _ => UploadMode::Chunked,
    }
}

#[cfg(test)]
mod tests {
    use super::{mode_for_kind, validate_persisted_kind};
    use crate::entities::upload_session;
    use crate::types::{UploadMode, UploadSessionKind, UploadSessionStatus};

    fn session(
        kind: UploadSessionKind,
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
            session_kind: kind,
            object_temp_key: object_temp_key.map(str::to_string),
            object_multipart_id: object_multipart_id.map(str::to_string),
            provider_session_ciphertext: None,
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
    fn persisted_kind_validation_rejects_incompatible_fields() {
        assert!(
            validate_persisted_kind(
                &session(
                    UploadSessionKind::ProviderRelayMultipart,
                    Some("files/temp"),
                    None
                ),
                UploadSessionKind::ProviderRelayMultipart
            )
            .is_err()
        );
        assert!(
            validate_persisted_kind(
                &session(
                    UploadSessionKind::ProviderRelayMultipart,
                    None,
                    Some("multipart")
                ),
                UploadSessionKind::ProviderRelayMultipart
            )
            .is_err()
        );
        assert!(
            validate_persisted_kind(
                &session(UploadSessionKind::ProviderPresignedSingle, None, None),
                UploadSessionKind::ProviderPresignedSingle
            )
            .is_err()
        );
    }

    #[test]
    fn provider_direct_kind_requires_temp_key_and_encrypted_session_metadata() {
        let mut valid = session(
            UploadSessionKind::ProviderDirectResumable,
            Some("files/temp"),
            None,
        );
        valid.provider_session_ciphertext = Some("encrypted-upload-url".to_string());
        assert!(
            validate_persisted_kind(&valid, UploadSessionKind::ProviderDirectResumable).is_ok()
        );
        assert!(
            validate_persisted_kind(
                &session(
                    UploadSessionKind::ProviderDirectResumable,
                    Some("files/temp"),
                    None
                ),
                UploadSessionKind::ProviderDirectResumable
            )
            .is_err()
        );
    }
}
