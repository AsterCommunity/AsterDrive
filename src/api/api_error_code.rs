//! Stable string API error codes for the transition away from subcodes.

use serde::{Deserialize, Serialize};
use std::str::FromStr;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::error_code::ErrorCode;
use crate::api::subcode::ApiSubcode;

macro_rules! define_api_error_codes {
    ($($variant:ident => $value:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
        pub enum ApiErrorCode {
            $(
                #[serde(rename = $value)]
                #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(rename = $value))]
                $variant,
            )+
        }

        impl ApiErrorCode {
            pub const ALL: &'static [Self] = &[
                $(Self::$variant,)+
            ];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value,)+
                }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value {
                    $($value => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

define_api_error_codes! {
    // response: success envelope.
    Success => "success",

    // common/runtime: generic HTTP and infrastructure failures.
    BadRequest => "bad_request",
    NotFound => "not_found",
    InternalServerError => "internal_server_error",
    DatabaseError => "database.error",
    ConfigError => "config.error",
    EndpointNotFound => "endpoint.not_found",
    RateLimited => "rate_limited",
    MailNotConfigured => "mail.not_configured",
    MailDeliveryFailed => "mail.delivery_failed",
    Conflict => "conflict",

    // auth/session: coarse authentication and session lifecycle errors.
    AuthFailed => "auth.failed",
    TokenExpired => "auth.token_expired",
    TokenInvalid => "auth.token_invalid",
    Forbidden => "forbidden",
    PendingActivation => "auth.pending_activation",
    ContactVerificationInvalid => "auth.contact_verification_invalid",
    ContactVerificationExpired => "auth.contact_verification_expired",
    TokenMissing => "auth.token_missing",
    CredentialsFailed => "auth.credentials_failed",
    MfaFailed => "auth.mfa_failed",
    RefreshTokenStale => "auth.refresh_token_stale",
    RefreshTokenReuseDetected => "auth.refresh_token_reuse_detected",

    // files/upload/thumbnails/locks: legacy top-level file and upload categories.
    FileNotFound => "file.not_found",
    FileTooLarge => "file.too_large",
    FileTypeNotAllowed => "file.type_not_allowed",
    FileUploadFailed => "file.upload_failed",
    UploadSessionNotFound => "upload.session_not_found",
    UploadSessionExpired => "upload.session_expired",
    ChunkUploadFailed => "upload.chunk_failed",
    UploadAssemblyFailed => "upload.assembly_failed",
    ThumbnailFailed => "thumbnail.failed",
    ResourceLocked => "resource.locked",
    PreconditionFailed => "precondition_failed",
    UploadAssembling => "upload.assembling",

    // storage policy/driver: legacy top-level storage categories.
    StoragePolicyNotFound => "storage.policy_not_found",
    StorageDriverError => "storage.driver_error",
    StorageQuotaExceeded => "storage.quota_exceeded",
    UnsupportedDriver => "storage.unsupported_driver",
    StorageAuthFailed => "storage.auth_failed",
    StoragePermissionDenied => "storage.permission_denied",
    StorageMisconfigured => "storage.misconfigured",
    StorageObjectNotFound => "storage.object_not_found",
    StorageRateLimited => "storage.rate_limited",
    StorageTransientFailure => "storage.transient_failure",
    StoragePreconditionFailed => "storage.precondition_failed",
    StorageOperationUnsupported => "storage.operation_unsupported",

    // folders: legacy top-level folder categories.
    FolderNotFound => "folder.not_found",

    // shares: legacy top-level share categories.
    ShareNotFound => "share.not_found",
    ShareExpired => "share.expired",
    SharePasswordRequired => "share.password_required",
    ShareDownloadLimitReached => "share.download_limit_reached",

    // archive_preview service: archive inspection and preview task failures.
    ArchivePreviewDisabled => "archive_preview.disabled",
    ArchivePreviewUserDisabled => "archive_preview.user_disabled",
    ArchivePreviewShareDisabled => "archive_preview.share_disabled",
    ArchivePreviewSourceTooLarge => "archive_preview.source_too_large",
    ArchivePreviewInvalidArchive => "archive_preview.invalid_archive",
    ArchivePreviewManifestTooLarge => "archive_preview.manifest_too_large",
    ArchivePreviewUnsupportedType => "archive_preview.unsupported_type",
    ArchivePreviewRejected => "archive_preview.rejected",
    ArchivePreviewSourceSizeMismatch => "archive_preview.source_size_mismatch",

    // auth/public/session/mfa/csrf middleware: specific auth validation and security failures.
    AuthUsernameExists => "auth.username_exists",
    AuthEmailExists => "auth.email_exists",
    AuthIdentifierExists => "auth.identifier_exists",
    AuthAdminRequired => "auth.admin_required",
    AuthAccountDisabled => "auth.account_disabled",
    AuthRequestSourceUntrusted => "auth.request_source_untrusted",
    AuthRequestOriginUntrusted => "auth.request_origin_untrusted",
    AuthRequestRefererUntrusted => "auth.request_referer_untrusted",
    AuthRequestSourceMissing => "auth.request_source_missing",
    AuthSessionUserMismatch => "auth.session_user_mismatch",
    AuthCsrfCookieMissing => "auth.csrf_cookie_missing",
    AuthCsrfHeaderMissing => "auth.csrf_header_missing",
    AuthCsrfTokenInvalid => "auth.csrf_token_invalid",
    AuthRegistrationDisabled => "auth.registration_disabled",
    AuthMfaFlowInvalid => "auth.mfa_flow_invalid",
    AuthMfaFlowExpired => "auth.mfa_flow_expired",
    AuthMfaCodeInvalid => "auth.mfa_code_invalid",
    AuthMfaAttemptsExceeded => "auth.mfa_attempts_exceeded",
    AuthMfaFactorRequired => "auth.mfa_factor_required",
    AuthMfaFactorAlreadyExists => "auth.mfa_factor_already_exists",
    AuthMfaRecoveryCodeUsed => "auth.mfa_recovery_code_used",
    AuthMfaEmailCodeRequired => "auth.mfa_email_code_required",
    AuthMfaEmailCodeExpired => "auth.mfa_email_code_expired",

    // profile/avatar/media processing: avatar upload and rendering failures.
    AvatarFileRequired => "avatar.file_required",
    AvatarUploadReadFailed => "avatar.upload_read_failed",
    AvatarProcessorUnavailable => "avatar.processor_unavailable",
    AvatarEmptyImage => "avatar.empty_image",
    AvatarRenderFailed => "avatar.render_failed",
    AvatarOutputInvalid => "avatar.output_invalid",

    // file/folder repositories and mutation services: entity conflicts and preconditions.
    FileNameConflict => "file.name_conflict",
    FileEtagMismatch => "file.etag_mismatch",
    FileModifiedDuringWrite => "file.modified_during_write",
    FolderNameConflict => "folder.name_conflict",
    LockNotOwner => "lock.not_owner",
    ShareScopeDenied => "share.scope_denied",

    // managed_ingress services: local/remote ingress profile validation and binding failures.
    ManagedIngressBindingMismatch => "managed_ingress.binding_mismatch",
    ManagedIngressDefaultDeleteRequiresReplacement => "managed_ingress.default_delete_requires_replacement",
    ManagedIngressDefaultError => "managed_ingress.default_error",
    ManagedIngressDefaultMissing => "managed_ingress.default_missing",
    ManagedIngressDefaultNotApplied => "managed_ingress.default_not_applied",
    ManagedIngressDefaultUpdateRequiresReplacement => "managed_ingress.default_update_requires_replacement",
    ManagedIngressDriverUnsupported => "managed_ingress.driver_unsupported",
    ManagedIngressLocalPathInvalid => "managed_ingress.local_path_invalid",
    ManagedIngressRequired => "managed_ingress.required",
    ManagedIngressSinglePrimaryRequired => "managed_ingress.single_primary_required",

    // master binding service: master/follower binding state.
    MasterBindingDisabled => "master_binding.disabled",

    // passkey service: WebAuthn credential validation.
    PasskeyNameInvalid => "passkey.name_invalid",
    PasskeyNameTooLong => "passkey.name_too_long",
    PasskeyNotDiscoverable => "passkey.not_discoverable",

    // team services: team membership and role authorization.
    TeamNotMember => "team.not_member",
    TeamOwnerRequired => "team.owner_required",
    TeamAdminOrOwnerRequired => "team.admin_or_owner_required",

    // policy service: policy mutation preconditions.
    PolicyUploadSessionsExist => "policy.upload_sessions_exist",

    // workspace services: workspace scope authorization.
    WorkspaceScopeDenied => "workspace.scope_denied",

    // external auth service: external provider/link policy decisions.
    ExternalAuthProviderDisabled => "external_auth.provider_disabled",
    ExternalAuthPolicyDenied => "external_auth.policy_denied",

    // remote node services: managed follower and remote storage enrollment.
    RemoteNodeDisabled => "remote_node.disabled",
    RemoteNodeEnrollmentRequired => "remote_node.enrollment_required",
    RemoteNodeUniqueConflict => "remote_node.unique_conflict",

    // storage drivers: normalized driver failure kinds.
    StorageAuth => "storage.auth",
    StorageNotFound => "storage.not_found",
    StoragePermission => "storage.permission",
    StoragePrecondition => "storage.precondition",
    StorageTransient => "storage.transient",
    StorageUnsupported => "storage.unsupported",
    StorageUnknown => "storage.unknown",

    // task service/runtime: background task lease failures.
    TaskLeaseLost => "task.lease_lost",
    TaskLeaseRenewalTimedOut => "task.lease_renewal_timed_out",

    // team member repository: member uniqueness conflicts.
    TeamMemberExists => "team.member_exists",

    // thumbnail/media processing: thumbnail generation pipeline failures.
    ThumbnailFormatGuessFailed => "thumbnail.format_guess_failed",
    ThumbnailDecodeFailed => "thumbnail.decode_failed",
    ThumbnailEncodeFailed => "thumbnail.encode_failed",
    ThumbnailSourceOpenFailed => "thumbnail.source_open_failed",
    ThumbnailSourceStreamFailed => "thumbnail.source_stream_failed",
    ThumbnailTaskPanicked => "thumbnail.task_panicked",
    ThumbnailSourceTooLarge => "thumbnail.source_too_large",
    ThumbnailProcessorUnavailable => "thumbnail.processor_unavailable",
    ThumbnailRenderFailed => "thumbnail.render_failed",
    ThumbnailOutputInvalid => "thumbnail.output_invalid",
    ThumbnailSourceTempCreateFailed => "thumbnail.source_temp_create_failed",
    ThumbnailSourceTempFlushFailed => "thumbnail.source_temp_flush_failed",
    ThumbnailSourceTempCopyFailed => "thumbnail.source_temp_copy_failed",

    // wopi service: public site URL and request source validation.
    WopiPublicSiteUrlRequired => "wopi.public_site_url_required",
    WopiAppDisabled => "wopi.app_disabled",
    WopiRequestOriginUntrusted => "wopi.request_origin_untrusted",
    WopiRequestRefererUntrusted => "wopi.request_referer_untrusted",

    // upload/workspace storage services: upload staging, chunking, hashing, and assembly failures.
    UploadTempDirCreateFailed => "upload.temp_dir_create_failed",
    UploadTempFileCreateFailed => "upload.temp_file_create_failed",
    UploadTempFileWriteFailed => "upload.temp_file_write_failed",
    UploadTempFileFlushFailed => "upload.temp_file_flush_failed",
    UploadRequestBodyReadFailed => "upload.request_body_read_failed",
    UploadRequestBodySizeOverflow => "upload.request_body_size_overflow",
    UploadRequestSizeMismatch => "upload.request_size_mismatch",
    UploadHashTempOpenFailed => "upload.hash_temp_open_failed",
    UploadHashTempReadFailed => "upload.hash_temp_read_failed",
    UploadFieldReadFailed => "upload.field_read_failed",
    UploadLocalStagingPathResolveFailed => "upload.local_staging_path_resolve_failed",
    UploadLocalStagingDirCreateFailed => "upload.local_staging_dir_create_failed",
    UploadLocalStagingFileCreateFailed => "upload.local_staging_file_create_failed",
    UploadLocalStagingWriteFailed => "upload.local_staging_write_failed",
    UploadLocalStagingFlushFailed => "upload.local_staging_flush_failed",
    UploadDirectRelayWriteFailed => "upload.direct_relay_write_failed",
    UploadDirectRelayShutdownFailed => "upload.direct_relay_shutdown_failed",
    UploadDirectRelayTaskFailed => "upload.direct_relay_task_failed",
    UploadBodySizeOverflow => "upload.body_size_overflow",
    UploadDeclaredSizeInvalid => "upload.declared_size_invalid",
    UploadEmptyFile => "upload.empty_file",
    UploadChunkPersistFailed => "upload.chunk_persist_failed",
    UploadChunkRelayFailed => "upload.chunk_relay_failed",
    UploadChunkTransportMismatch => "upload.chunk_transport_mismatch",
    UploadChunkSessionInvalid => "upload.chunk_session_invalid",
    UploadChunkNumberOutOfRange => "upload.chunk_number_out_of_range",
    UploadChunkSizeMismatch => "upload.chunk_size_mismatch",
    UploadChunkTooLarge => "upload.chunk_too_large",
    UploadChunkSizeOverflow => "upload.chunk_size_overflow",
    UploadStatusConflict => "upload.status_conflict",
    UploadCompletedFileMissing => "upload.completed_file_missing",
    UploadPreviousFailure => "upload.previous_failure",
    UploadPartsRequired => "upload.parts_required",
    UploadIncompleteChunks => "upload.incomplete_chunks",
    UploadIncompleteParts => "upload.incomplete_parts",
    UploadMissingPart => "upload.missing_part",
    UploadTempObjectMissing => "upload.temp_object_missing",
    UploadTempObjectSizeMismatch => "upload.temp_object_size_mismatch",
    UploadFinalObjectSizeMismatch => "upload.final_object_size_mismatch",
    UploadSessionCorrupted => "upload.session_corrupted",
    UploadPartNumbersEmpty => "upload.part_numbers_empty",
    UploadPartNumbersTooMany => "upload.part_numbers_too_many",
    UploadPartNumberOutOfRange => "upload.part_number_out_of_range",
    UploadAssemblyIoFailed => "upload.assembly_io_failed",
    UploadAssemblySizeOverflow => "upload.assembly_size_overflow",

    // webdav account service: account uniqueness conflicts.
    WebdavUsernameExists => "webdav.username_exists",

    // wopi service: file size precondition.
    WopiMaxExpectedSizeExceeded => "wopi.max_expected_size_exceeded",

    // validation middleware/routes: request shape, origin, and setup validation.
    ValidationRequestOriginInvalid => "validation.request_origin_invalid",
    ValidationRequestRefererInvalid => "validation.request_referer_invalid",
    ValidationRequestHostInvalid => "validation.request_host_invalid",
    ValidationRequestSchemeInvalid => "validation.request_scheme_invalid",
    ValidationRequestHeaderValueInvalid => "validation.request_header_value_invalid",
    ValidationSystemAlreadyInitialized => "validation.system_already_initialized",
}

impl ApiErrorCode {
    // TODO(0.3.0): remove this compatibility bridge when the top-level response
    // code field is migrated from legacy numeric ErrorCode to ApiErrorCode.
    pub fn from_legacy_code(code: ErrorCode) -> Self {
        match code {
            ErrorCode::Success => Self::Success,
            ErrorCode::BadRequest => Self::BadRequest,
            ErrorCode::NotFound => Self::NotFound,
            ErrorCode::InternalServerError => Self::InternalServerError,
            ErrorCode::DatabaseError => Self::DatabaseError,
            ErrorCode::ConfigError => Self::ConfigError,
            ErrorCode::EndpointNotFound => Self::EndpointNotFound,
            ErrorCode::RateLimited => Self::RateLimited,
            ErrorCode::MailNotConfigured => Self::MailNotConfigured,
            ErrorCode::MailDeliveryFailed => Self::MailDeliveryFailed,
            ErrorCode::Conflict => Self::Conflict,
            ErrorCode::AuthFailed => Self::AuthFailed,
            ErrorCode::TokenExpired => Self::TokenExpired,
            ErrorCode::TokenInvalid => Self::TokenInvalid,
            ErrorCode::Forbidden => Self::Forbidden,
            ErrorCode::PendingActivation => Self::PendingActivation,
            ErrorCode::ContactVerificationInvalid => Self::ContactVerificationInvalid,
            ErrorCode::ContactVerificationExpired => Self::ContactVerificationExpired,
            ErrorCode::TokenMissing => Self::TokenMissing,
            ErrorCode::CredentialsFailed => Self::CredentialsFailed,
            ErrorCode::MfaFailed => Self::MfaFailed,
            ErrorCode::RefreshTokenStale => Self::RefreshTokenStale,
            ErrorCode::RefreshTokenReuseDetected => Self::RefreshTokenReuseDetected,
            ErrorCode::FileNotFound => Self::FileNotFound,
            ErrorCode::FileTooLarge => Self::FileTooLarge,
            ErrorCode::FileTypeNotAllowed => Self::FileTypeNotAllowed,
            ErrorCode::FileUploadFailed => Self::FileUploadFailed,
            ErrorCode::UploadSessionNotFound => Self::UploadSessionNotFound,
            ErrorCode::UploadSessionExpired => Self::UploadSessionExpired,
            ErrorCode::ChunkUploadFailed => Self::ChunkUploadFailed,
            ErrorCode::UploadAssemblyFailed => Self::UploadAssemblyFailed,
            ErrorCode::ThumbnailFailed => Self::ThumbnailFailed,
            ErrorCode::ResourceLocked => Self::ResourceLocked,
            ErrorCode::PreconditionFailed => Self::PreconditionFailed,
            ErrorCode::UploadAssembling => Self::UploadAssembling,
            ErrorCode::StoragePolicyNotFound => Self::StoragePolicyNotFound,
            ErrorCode::StorageDriverError => Self::StorageDriverError,
            ErrorCode::StorageQuotaExceeded => Self::StorageQuotaExceeded,
            ErrorCode::UnsupportedDriver => Self::UnsupportedDriver,
            ErrorCode::StorageAuthFailed => Self::StorageAuthFailed,
            ErrorCode::StoragePermissionDenied => Self::StoragePermissionDenied,
            ErrorCode::StorageMisconfigured => Self::StorageMisconfigured,
            ErrorCode::StorageObjectNotFound => Self::StorageObjectNotFound,
            ErrorCode::StorageRateLimited => Self::StorageRateLimited,
            ErrorCode::StorageTransientFailure => Self::StorageTransientFailure,
            ErrorCode::StoragePreconditionFailed => Self::StoragePreconditionFailed,
            ErrorCode::StorageOperationUnsupported => Self::StorageOperationUnsupported,
            ErrorCode::FolderNotFound => Self::FolderNotFound,
            ErrorCode::ShareNotFound => Self::ShareNotFound,
            ErrorCode::ShareExpired => Self::ShareExpired,
            ErrorCode::SharePasswordRequired => Self::SharePasswordRequired,
            ErrorCode::ShareDownloadLimitReached => Self::ShareDownloadLimitReached,
        }
    }

    // TODO(0.3.0): remove after ApiSubcode stops being exposed in API responses.
    // New code should construct or carry ApiErrorCode directly instead of
    // deriving it from the legacy subcode compatibility layer.
    pub fn from_subcode(subcode: ApiSubcode) -> Option<Self> {
        Self::parse(subcode.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseApiErrorCodeError;

impl std::fmt::Display for ParseApiErrorCodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown API error code")
    }
}

impl std::error::Error for ParseApiErrorCodeError {}

impl FromStr for ApiErrorCode {
    type Err = ParseApiErrorCodeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or(ParseApiErrorCodeError)
    }
}

impl AsRef<str> for ApiErrorCode {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for ApiErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::ApiErrorCode;
    use crate::api::error_code::ErrorCode;
    use crate::api::subcode::ApiSubcode;
    use std::collections::HashSet;

    #[test]
    fn api_error_codes_parse_all_stable_values() {
        for &code in ApiErrorCode::ALL {
            assert_eq!(code.as_str().parse::<ApiErrorCode>(), Ok(code));
            assert_eq!(ApiErrorCode::parse(code.as_str()), Some(code));
        }
    }

    #[test]
    fn api_error_codes_do_not_accept_variant_names_or_unknown_values() {
        for value in [
            "",
            "AuthFailed",
            "StorageTransient",
            "auth.failed ",
            " auth.failed",
            "AUTH.FAILED",
            "auth_failed",
            "2000",
            "remote.dynamic",
            "storage.remote_permission",
        ] {
            assert!(
                value.parse::<ApiErrorCode>().is_err(),
                "{value:?} should not parse as ApiErrorCode"
            );
            assert_eq!(ApiErrorCode::parse(value), None);
        }
    }

    #[test]
    fn api_error_codes_have_unique_wire_values() {
        let mut values = HashSet::new();

        for &code in ApiErrorCode::ALL {
            assert!(
                values.insert(code.as_str()),
                "duplicate ApiErrorCode wire value {}",
                code.as_str()
            );
        }
    }

    #[test]
    fn api_error_codes_serialize_to_wire_values() {
        assert_eq!(
            serde_json::to_value(ApiErrorCode::AuthFailed).unwrap(),
            serde_json::json!("auth.failed")
        );
        assert_eq!(
            serde_json::to_value(ApiErrorCode::UploadChunkSizeMismatch).unwrap(),
            serde_json::json!("upload.chunk_size_mismatch")
        );
        assert_eq!(
            serde_json::to_value(ApiErrorCode::StorageTransient).unwrap(),
            serde_json::json!("storage.transient")
        );
    }

    #[test]
    fn api_error_codes_deserialize_from_wire_values() {
        assert_eq!(
            serde_json::from_value::<ApiErrorCode>(serde_json::json!("auth.failed")).unwrap(),
            ApiErrorCode::AuthFailed
        );
        assert_eq!(
            serde_json::from_value::<ApiErrorCode>(serde_json::json!("upload.chunk_size_mismatch"))
                .unwrap(),
            ApiErrorCode::UploadChunkSizeMismatch
        );
        assert!(serde_json::from_value::<ApiErrorCode>(serde_json::json!("AuthFailed")).is_err());
        assert!(
            serde_json::from_value::<ApiErrorCode>(serde_json::json!("remote.dynamic")).is_err()
        );
    }

    #[test]
    fn api_error_codes_display_and_as_ref_use_wire_values() {
        let code = ApiErrorCode::RemoteNodeEnrollmentRequired;

        assert_eq!(code.to_string(), "remote_node.enrollment_required");
        assert_eq!(code.as_ref(), "remote_node.enrollment_required");
    }

    #[test]
    fn legacy_error_codes_map_to_stable_api_error_codes() {
        let cases = [
            (ErrorCode::Success, ApiErrorCode::Success),
            (ErrorCode::BadRequest, ApiErrorCode::BadRequest),
            (ErrorCode::NotFound, ApiErrorCode::NotFound),
            (
                ErrorCode::InternalServerError,
                ApiErrorCode::InternalServerError,
            ),
            (ErrorCode::DatabaseError, ApiErrorCode::DatabaseError),
            (ErrorCode::ConfigError, ApiErrorCode::ConfigError),
            (ErrorCode::EndpointNotFound, ApiErrorCode::EndpointNotFound),
            (ErrorCode::RateLimited, ApiErrorCode::RateLimited),
            (
                ErrorCode::MailNotConfigured,
                ApiErrorCode::MailNotConfigured,
            ),
            (
                ErrorCode::MailDeliveryFailed,
                ApiErrorCode::MailDeliveryFailed,
            ),
            (ErrorCode::Conflict, ApiErrorCode::Conflict),
            (ErrorCode::AuthFailed, ApiErrorCode::AuthFailed),
            (ErrorCode::TokenExpired, ApiErrorCode::TokenExpired),
            (ErrorCode::TokenInvalid, ApiErrorCode::TokenInvalid),
            (ErrorCode::Forbidden, ApiErrorCode::Forbidden),
            (
                ErrorCode::PendingActivation,
                ApiErrorCode::PendingActivation,
            ),
            (
                ErrorCode::ContactVerificationInvalid,
                ApiErrorCode::ContactVerificationInvalid,
            ),
            (
                ErrorCode::ContactVerificationExpired,
                ApiErrorCode::ContactVerificationExpired,
            ),
            (ErrorCode::TokenMissing, ApiErrorCode::TokenMissing),
            (
                ErrorCode::CredentialsFailed,
                ApiErrorCode::CredentialsFailed,
            ),
            (ErrorCode::MfaFailed, ApiErrorCode::MfaFailed),
            (
                ErrorCode::RefreshTokenStale,
                ApiErrorCode::RefreshTokenStale,
            ),
            (
                ErrorCode::RefreshTokenReuseDetected,
                ApiErrorCode::RefreshTokenReuseDetected,
            ),
            (ErrorCode::FileNotFound, ApiErrorCode::FileNotFound),
            (ErrorCode::FileTooLarge, ApiErrorCode::FileTooLarge),
            (
                ErrorCode::FileTypeNotAllowed,
                ApiErrorCode::FileTypeNotAllowed,
            ),
            (ErrorCode::FileUploadFailed, ApiErrorCode::FileUploadFailed),
            (
                ErrorCode::UploadSessionNotFound,
                ApiErrorCode::UploadSessionNotFound,
            ),
            (
                ErrorCode::UploadSessionExpired,
                ApiErrorCode::UploadSessionExpired,
            ),
            (
                ErrorCode::ChunkUploadFailed,
                ApiErrorCode::ChunkUploadFailed,
            ),
            (
                ErrorCode::UploadAssemblyFailed,
                ApiErrorCode::UploadAssemblyFailed,
            ),
            (ErrorCode::ThumbnailFailed, ApiErrorCode::ThumbnailFailed),
            (ErrorCode::ResourceLocked, ApiErrorCode::ResourceLocked),
            (
                ErrorCode::PreconditionFailed,
                ApiErrorCode::PreconditionFailed,
            ),
            (ErrorCode::UploadAssembling, ApiErrorCode::UploadAssembling),
            (
                ErrorCode::StoragePolicyNotFound,
                ApiErrorCode::StoragePolicyNotFound,
            ),
            (
                ErrorCode::StorageDriverError,
                ApiErrorCode::StorageDriverError,
            ),
            (
                ErrorCode::StorageQuotaExceeded,
                ApiErrorCode::StorageQuotaExceeded,
            ),
            (
                ErrorCode::UnsupportedDriver,
                ApiErrorCode::UnsupportedDriver,
            ),
            (
                ErrorCode::StorageAuthFailed,
                ApiErrorCode::StorageAuthFailed,
            ),
            (
                ErrorCode::StoragePermissionDenied,
                ApiErrorCode::StoragePermissionDenied,
            ),
            (
                ErrorCode::StorageMisconfigured,
                ApiErrorCode::StorageMisconfigured,
            ),
            (
                ErrorCode::StorageObjectNotFound,
                ApiErrorCode::StorageObjectNotFound,
            ),
            (
                ErrorCode::StorageRateLimited,
                ApiErrorCode::StorageRateLimited,
            ),
            (
                ErrorCode::StorageTransientFailure,
                ApiErrorCode::StorageTransientFailure,
            ),
            (
                ErrorCode::StoragePreconditionFailed,
                ApiErrorCode::StoragePreconditionFailed,
            ),
            (
                ErrorCode::StorageOperationUnsupported,
                ApiErrorCode::StorageOperationUnsupported,
            ),
            (ErrorCode::FolderNotFound, ApiErrorCode::FolderNotFound),
            (ErrorCode::ShareNotFound, ApiErrorCode::ShareNotFound),
            (ErrorCode::ShareExpired, ApiErrorCode::ShareExpired),
            (
                ErrorCode::SharePasswordRequired,
                ApiErrorCode::SharePasswordRequired,
            ),
            (
                ErrorCode::ShareDownloadLimitReached,
                ApiErrorCode::ShareDownloadLimitReached,
            ),
        ];

        for (legacy_code, expected_api_code) in cases {
            assert_eq!(
                ApiErrorCode::from_legacy_code(legacy_code),
                expected_api_code,
                "{legacy_code:?} mapped to unexpected ApiErrorCode"
            );
        }
    }

    #[test]
    fn every_legacy_subcode_maps_to_api_error_code_with_same_wire_value() {
        for &subcode in ApiSubcode::ALL {
            let code = ApiErrorCode::from_subcode(subcode)
                .unwrap_or_else(|| panic!("ApiErrorCode missing {}", subcode.as_str()));

            assert_eq!(code.as_str(), subcode.as_str());
        }
    }
}
