//! Legacy stable API error subcodes.
//!
//! TODO(0.3.0): remove this as a public API contract after all callers and
//! clients use `ApiErrorCode`. Until then, keep wire values stable so old
//! clients can read `ApiErrorInfo.subcode`.

use serde::{Deserialize, Serialize};
use std::str::FromStr;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

macro_rules! define_api_subcodes {
    ($($variant:ident => $value:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
        pub enum ApiSubcode {
            $(
                #[serde(rename = $value)]
                #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(rename = $value))]
                $variant,
            )+
        }

        impl ApiSubcode {
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

define_api_subcodes! {
    ArchivePreviewDisabled => "archive_preview.disabled",
    ArchivePreviewUserDisabled => "archive_preview.user_disabled",
    ArchivePreviewShareDisabled => "archive_preview.share_disabled",
    ArchivePreviewSourceTooLarge => "archive_preview.source_too_large",
    ArchivePreviewInvalidArchive => "archive_preview.invalid_archive",
    ArchivePreviewManifestTooLarge => "archive_preview.manifest_too_large",
    ArchivePreviewUnsupportedType => "archive_preview.unsupported_type",
    ArchivePreviewRejected => "archive_preview.rejected",
    ArchivePreviewSourceSizeMismatch => "archive_preview.source_size_mismatch",

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

    AvatarFileRequired => "avatar.file_required",
    AvatarUploadReadFailed => "avatar.upload_read_failed",
    AvatarProcessorUnavailable => "avatar.processor_unavailable",
    AvatarEmptyImage => "avatar.empty_image",
    AvatarRenderFailed => "avatar.render_failed",
    AvatarOutputInvalid => "avatar.output_invalid",

    FileNameConflict => "file.name_conflict",
    FileEtagMismatch => "file.etag_mismatch",
    FileModifiedDuringWrite => "file.modified_during_write",

    FolderNameConflict => "folder.name_conflict",

    LockNotOwner => "lock.not_owner",

    ShareScopeDenied => "share.scope_denied",

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

    MasterBindingDisabled => "master_binding.disabled",

    PasskeyNameInvalid => "passkey.name_invalid",
    PasskeyNameTooLong => "passkey.name_too_long",
    PasskeyNotDiscoverable => "passkey.not_discoverable",

    TeamNotMember => "team.not_member",
    TeamOwnerRequired => "team.owner_required",
    TeamAdminOrOwnerRequired => "team.admin_or_owner_required",

    PolicyUploadSessionsExist => "policy.upload_sessions_exist",

    WorkspaceScopeDenied => "workspace.scope_denied",

    ExternalAuthProviderDisabled => "external_auth.provider_disabled",
    ExternalAuthPolicyDenied => "external_auth.policy_denied",

    RemoteNodeDisabled => "remote_node.disabled",
    RemoteNodeEnrollmentRequired => "remote_node.enrollment_required",
    RemoteNodeUniqueConflict => "remote_node.unique_conflict",

    StorageAuth => "storage.auth",
    StorageMisconfigured => "storage.misconfigured",
    StorageNotFound => "storage.not_found",
    StoragePermission => "storage.permission",
    StoragePrecondition => "storage.precondition",
    StorageRateLimited => "storage.rate_limited",
    StorageTransient => "storage.transient",
    StorageUnsupported => "storage.unsupported",
    StorageUnknown => "storage.unknown",

    TaskLeaseLost => "task.lease_lost",
    TaskLeaseRenewalTimedOut => "task.lease_renewal_timed_out",

    TeamMemberExists => "team.member_exists",

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

    WopiPublicSiteUrlRequired => "wopi.public_site_url_required",
    WopiAppDisabled => "wopi.app_disabled",
    WopiRequestOriginUntrusted => "wopi.request_origin_untrusted",
    WopiRequestRefererUntrusted => "wopi.request_referer_untrusted",

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

    WebdavUsernameExists => "webdav.username_exists",

    WopiMaxExpectedSizeExceeded => "wopi.max_expected_size_exceeded",

    ValidationRequestOriginInvalid => "validation.request_origin_invalid",
    ValidationRequestRefererInvalid => "validation.request_referer_invalid",
    ValidationRequestHostInvalid => "validation.request_host_invalid",
    ValidationRequestSchemeInvalid => "validation.request_scheme_invalid",
    ValidationRequestHeaderValueInvalid => "validation.request_header_value_invalid",
    ValidationSystemAlreadyInitialized => "validation.system_already_initialized",
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseApiSubcodeError;

impl std::fmt::Display for ParseApiSubcodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown API subcode")
    }
}

impl std::error::Error for ParseApiSubcodeError {}

impl FromStr for ApiSubcode {
    type Err = ParseApiSubcodeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or(ParseApiSubcodeError)
    }
}

impl AsRef<str> for ApiSubcode {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for ApiSubcode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::ApiSubcode;

    #[test]
    fn api_subcodes_parse_all_stable_values() {
        for &subcode in ApiSubcode::ALL {
            assert_eq!(subcode.as_str().parse::<ApiSubcode>(), Ok(subcode));
        }
    }

    #[test]
    fn api_subcodes_do_not_claim_unknown_remote_values() {
        assert!("remote.dynamic".parse::<ApiSubcode>().is_err());
    }

    #[test]
    fn api_subcodes_serialize_to_wire_values() {
        assert_eq!(
            serde_json::to_value(ApiSubcode::ArchivePreviewDisabled).unwrap(),
            serde_json::json!("archive_preview.disabled")
        );
        assert_eq!(
            serde_json::to_value(ApiSubcode::PolicyUploadSessionsExist).unwrap(),
            serde_json::json!("policy.upload_sessions_exist")
        );
    }

    #[test]
    fn api_subcodes_deserialize_from_wire_values() {
        assert_eq!(
            serde_json::from_value::<ApiSubcode>(serde_json::json!("storage.transient")).unwrap(),
            ApiSubcode::StorageTransient
        );
        assert_eq!(
            serde_json::from_value::<ApiSubcode>(serde_json::json!("ArchivePreviewDisabled"))
                .unwrap_err()
                .classify(),
            serde_json::error::Category::Data
        );
        assert_eq!(
            serde_json::from_value::<ApiSubcode>(serde_json::json!("remote.dynamic"))
                .unwrap_err()
                .classify(),
            serde_json::error::Category::Data
        );
    }

    #[test]
    fn api_subcodes_display_and_as_ref_use_wire_values() {
        let subcode = ApiSubcode::RemoteNodeEnrollmentRequired;

        assert_eq!(subcode.to_string(), "remote_node.enrollment_required");
        assert_eq!(subcode.as_ref(), "remote_node.enrollment_required");
    }
}
