//! Stable string API error codes.

use serde::{Deserialize, Serialize};
use std::str::FromStr;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

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
    ConfigPublicSiteUrlRequired => "config.public_site_url_required",
    ConfigPublicSiteUrlInvalid => "config.public_site_url_invalid",

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
    ArchiveCompressDisabled => "archive_compress.disabled",
    ArchiveDownloadUserDisabled => "archive_download.user_disabled",
    ArchiveDownloadShareDisabled => "archive_download.share_disabled",

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
    AuthPasskeyLoginDisabled => "auth.passkey_login_disabled",
    AuthRegistrationDisabled => "auth.registration_disabled",
    AuthEmailBlocked => "auth.email_blocked",
    AuthEmailNotAllowlisted => "auth.email_not_allowlisted",
    AuthMfaFlowInvalid => "auth.mfa_flow_invalid",
    AuthMfaFlowExpired => "auth.mfa_flow_expired",
    AuthMfaCodeInvalid => "auth.mfa_code_invalid",
    AuthMfaAttemptsExceeded => "auth.mfa_attempts_exceeded",
    AuthMfaFactorRequired => "auth.mfa_factor_required",
    AuthMfaFactorAlreadyExists => "auth.mfa_factor_already_exists",
    AuthMfaRecoveryCodeUsed => "auth.mfa_recovery_code_used",
    AuthMfaEmailCodeRequired => "auth.mfa_email_code_required",
    AuthMfaEmailCodeExpired => "auth.mfa_email_code_expired",
    AuthInvitationInvalid => "auth.invitation_invalid",
    AuthInvitationExpired => "auth.invitation_expired",
    AuthInvitationRevoked => "auth.invitation_revoked",
    AuthInvitationAccepted => "auth.invitation_accepted",
    AuthPasswordChangeRequired => "auth.password_change_required",

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

    // TODO(remote-storage-target): keep managed_ingress.* wire codes stable
    // for existing clients while the service/API names use remote storage target terms.
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

    // policy service: policy mutation and connection preconditions.
    PolicyUploadSessionsExist => "policy.upload_sessions_exist",
    PolicyStorageAccessKeyRequired => "policy.storage_access_key_required",
    PolicyStorageSecretKeyRequired => "policy.storage_secret_key_required",
    PolicyStorageBucketRequired => "policy.storage_bucket_required",
    PolicyStorageEndpointInvalid => "policy.storage_endpoint_invalid",
    PolicyRemoteNodeRequired => "policy.remote_node_required",
    PolicyRemoteNodeUnexpected => "policy.remote_node_unexpected",
    PolicyRemoteNodeDisabled => "policy.remote_node_disabled",
    PolicyRemoteNodeBaseUrlRequired => "policy.remote_node_base_url_required",
    PolicyRemoteNodeTransferStrategyUnsupported => "policy.remote_node_transfer_strategy_unsupported",
    PolicyOneDriveOptionsUnsupported => "policy.onedrive_options_unsupported",
    PolicyOneDriveAccountModeRequired => "policy.onedrive_account_mode_required",
    PolicyOneDrivePersonalChinaCloudUnsupported => "policy.onedrive_personal_china_cloud_unsupported",
    PolicyOneDriveSharePointSiteRequired => "policy.onedrive_sharepoint_site_required",
    PolicyOneDriveGroupRequired => "policy.onedrive_group_required",
    PolicyNativeThumbnailUnsupported => "policy.native_thumbnail_unsupported",
    PolicyNativeMediaMetadataUnsupported => "policy.native_media_metadata_unsupported",
    PolicyPromotionSourceUnsupported => "policy.promotion_source_unsupported",
    PolicyPromotionTargetUnsupported => "policy.promotion_target_unsupported",
    PolicyPromotionBucketChangeDenied => "policy.promotion_bucket_change_denied",
    PolicyActionUnsupported => "policy.action_unsupported",
    PolicyActionParameterRequired => "policy.action_parameter_required",
    PolicyActionParameterInvalid => "policy.action_parameter_invalid",

    // workspace services: workspace scope authorization.
    WorkspaceScopeDenied => "workspace.scope_denied",

    // external auth service: external provider/link policy decisions.
    ExternalAuthProviderDisabled => "external_auth.provider_disabled",
    ExternalAuthPolicyDenied => "external_auth.policy_denied",
    ExternalAuthCallbackRedirectUriRequired => "external_auth.callback_redirect_uri_required",

    // offline download: external download engine probes and setup failures.
    OfflineDownloadAria2RpcAuthFailed => "offline_download.aria2_rpc_auth_failed",
    OfflineDownloadAria2RpcProbeFailed => "offline_download.aria2_rpc_probe_failed",

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

    // task service/runtime: background task lease failures and manual retry preconditions.
    TaskLeaseLost => "task.lease_lost",
    TaskLeaseRenewalTimedOut => "task.lease_renewal_timed_out",
    TaskWorkerShutdownRequested => "task.worker_shutdown_requested",
    TaskRetryStatusConflict => "task.retry_status_conflict",
    TaskRetryNotAllowed => "task.retry_not_allowed",

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

    // search API: stable query parameter validation failures.
    SearchQueryEmpty => "search.query_empty",
    SearchTypeInvalid => "search.type_invalid",
    SearchTagMatchInvalid => "search.tag_match_invalid",
    SearchSizeRangeInvalid => "search.size_range_invalid",
    SearchFileFilterTypeConflict => "search.file_filter_type_conflict",
    SearchMimeTypeEmpty => "search.mime_type_empty",
    SearchCategoryInvalid => "search.category_invalid",
    SearchExtensionsInvalid => "search.extensions_invalid",
    SearchTagIdsInvalid => "search.tag_ids_invalid",
    SearchDateInvalid => "search.date_invalid",
    SearchDateRangeInvalid => "search.date_range_invalid",

    // internal storage protocol: stable request shape and range validation failures.
    InternalStorageRangeLengthInvalid => "internal_storage.range_length_invalid",
    InternalStorageRangeEmptyObject => "internal_storage.range_empty_object",
    InternalStorageRangeOffsetOutOfBounds => "internal_storage.range_offset_out_of_bounds",
    InternalStorageRangeHeaderInvalid => "internal_storage.range_header_invalid",
    InternalStorageRangeMultipleUnsupported => "internal_storage.range_multiple_unsupported",
    InternalStorageRangeBoundsInvalid => "internal_storage.range_bounds_invalid",
    InternalStorageContentLengthRequired => "internal_storage.content_length_required",
    InternalStorageContentLengthInvalid => "internal_storage.content_length_invalid",
    InternalStorageComposePartsRequired => "internal_storage.compose_parts_required",
    InternalStorageComposeExpectedSizeInvalid => "internal_storage.compose_expected_size_invalid",
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
}
