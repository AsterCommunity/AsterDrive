//! API 错误码定义
//!
//! 按千位分域，序列化为数字传输给前端：
//! - 0: 成功
//! - 1000-1099: 通用错误
//! - 2000-2099: 认证错误
//! - 3000-3099: 文件错误
//! - 4000-4099: 存储策略错误
//! - 5000-5099: 文件夹错误
//! - 6000-6099: 分享错误

use serde_repr::{Deserialize_repr, Serialize_repr};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::subcode::ApiSubcode;
use crate::errors::AsterError;
use crate::storage::StorageErrorKind;

/// API 错误码，序列化为数字
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(example = 0))]
#[repr(i32)]
pub enum ErrorCode {
    // 成功
    Success = 0,

    // 通用错误 1000-1099
    BadRequest = 1000,
    NotFound = 1001,
    InternalServerError = 1002,
    DatabaseError = 1003,
    ConfigError = 1004,
    EndpointNotFound = 1005,
    RateLimited = 1006,
    MailNotConfigured = 1007,
    MailDeliveryFailed = 1008,
    Conflict = 1009,

    // 认证错误 2000-2099
    AuthFailed = 2000,
    TokenExpired = 2001,
    TokenInvalid = 2002,
    Forbidden = 2003,
    PendingActivation = 2004,
    ContactVerificationInvalid = 2005,
    ContactVerificationExpired = 2006,
    TokenMissing = 2007,
    CredentialsFailed = 2008,
    MfaFailed = 2009,

    // 文件错误 3000-3099
    FileNotFound = 3000,
    FileTooLarge = 3001,
    FileTypeNotAllowed = 3002,
    FileUploadFailed = 3003,
    UploadSessionNotFound = 3004,
    UploadSessionExpired = 3005,
    ChunkUploadFailed = 3006,
    UploadAssemblyFailed = 3007,
    ThumbnailFailed = 3008,
    ResourceLocked = 3009,
    PreconditionFailed = 3010,
    UploadAssembling = 3011,

    // 存储策略错误 4000-4099
    StoragePolicyNotFound = 4000,
    StorageDriverError = 4001,
    StorageQuotaExceeded = 4002,
    UnsupportedDriver = 4003,
    StorageAuthFailed = 4004,
    StoragePermissionDenied = 4005,
    StorageMisconfigured = 4006,
    StorageObjectNotFound = 4007,
    StorageRateLimited = 4008,
    StorageTransientFailure = 4009,
    StoragePreconditionFailed = 4010,
    StorageOperationUnsupported = 4011,

    // 文件夹错误 5000-5099
    FolderNotFound = 5000,

    // 分享错误 6000-6099
    ShareNotFound = 6000,
    ShareExpired = 6001,
    SharePasswordRequired = 6002,
    ShareDownloadLimitReached = 6003,
}

impl From<&AsterError> for ErrorCode {
    fn from(err: &AsterError) -> Self {
        match err {
            // 基础设施
            AsterError::DatabaseConnection(_) | AsterError::DatabaseOperation(_) => {
                ErrorCode::DatabaseError
            }
            AsterError::ConfigError(_) => ErrorCode::ConfigError,
            AsterError::InternalError(_) => ErrorCode::InternalServerError,
            AsterError::ValidationError(_) => {
                if matches!(
                    err.api_error_subcode(),
                    Some(
                        ApiSubcode::AuthUsernameExists
                            | ApiSubcode::AuthEmailExists
                            | ApiSubcode::AuthIdentifierExists
                            | ApiSubcode::AuthMfaFactorAlreadyExists
                            | ApiSubcode::FileNameConflict
                            | ApiSubcode::FolderNameConflict
                            | ApiSubcode::TeamMemberExists
                            | ApiSubcode::WebdavUsernameExists
                            | ApiSubcode::RemoteNodeUniqueConflict
                    )
                ) {
                    ErrorCode::Conflict
                } else {
                    ErrorCode::BadRequest
                }
            }
            AsterError::RecordNotFound(_) => ErrorCode::NotFound,
            AsterError::MailNotConfigured(_) => ErrorCode::MailNotConfigured,
            AsterError::MailDeliveryFailed(_) => ErrorCode::MailDeliveryFailed,

            // 认证
            AsterError::AuthInvalidCredentials(_) => ErrorCode::CredentialsFailed,
            AsterError::AuthTokenExpired(_) => ErrorCode::TokenExpired,
            AsterError::AuthTokenInvalid(_) => ErrorCode::TokenInvalid,
            AsterError::AuthTokenMissing(_) => ErrorCode::TokenMissing,
            AsterError::AuthMfaFailed(_) => ErrorCode::MfaFailed,
            AsterError::AuthForbidden(_) => ErrorCode::Forbidden,
            AsterError::AuthPendingActivation(_) => ErrorCode::PendingActivation,
            AsterError::ContactVerificationInvalid(_) => ErrorCode::ContactVerificationInvalid,
            AsterError::ContactVerificationExpired(_) => ErrorCode::ContactVerificationExpired,
            AsterError::RateLimited(_) => ErrorCode::RateLimited,

            // 文件
            AsterError::FileNotFound(_) => ErrorCode::FileNotFound,
            AsterError::FileTooLarge(_) => ErrorCode::FileTooLarge,
            AsterError::FileTypeNotAllowed(_) => ErrorCode::FileTypeNotAllowed,
            AsterError::FileUploadFailed(_) => ErrorCode::FileUploadFailed,
            AsterError::PayloadTooLarge(_) => ErrorCode::FileTooLarge,

            // 存储策略
            AsterError::StoragePolicyNotFound(_) => ErrorCode::StoragePolicyNotFound,
            AsterError::StorageDriverError(_) => {
                match err
                    .storage_error_kind()
                    .unwrap_or(StorageErrorKind::Unknown)
                {
                    StorageErrorKind::Auth => ErrorCode::StorageAuthFailed,
                    StorageErrorKind::Misconfigured => ErrorCode::StorageMisconfigured,
                    StorageErrorKind::NotFound => ErrorCode::StorageObjectNotFound,
                    StorageErrorKind::Permission => ErrorCode::StoragePermissionDenied,
                    StorageErrorKind::Precondition => ErrorCode::StoragePreconditionFailed,
                    StorageErrorKind::RateLimited => ErrorCode::StorageRateLimited,
                    StorageErrorKind::Transient => ErrorCode::StorageTransientFailure,
                    StorageErrorKind::Unsupported => ErrorCode::StorageOperationUnsupported,
                    StorageErrorKind::Unknown => ErrorCode::StorageDriverError,
                }
            }
            AsterError::StorageQuotaExceeded(_) => ErrorCode::StorageQuotaExceeded,
            AsterError::UnsupportedDriver(_) => ErrorCode::UnsupportedDriver,

            // 文件夹
            AsterError::FolderNotFound(_) => ErrorCode::FolderNotFound,

            // 分片上传
            AsterError::UploadSessionNotFound(_) => ErrorCode::UploadSessionNotFound,
            AsterError::UploadSessionExpired(_) => ErrorCode::UploadSessionExpired,
            AsterError::ChunkUploadFailed(_) => ErrorCode::ChunkUploadFailed,
            AsterError::UploadAssemblyFailed(_) => ErrorCode::UploadAssemblyFailed,
            AsterError::UploadAssembling(_) => ErrorCode::UploadAssembling,

            // 缩略图
            AsterError::ThumbnailGenerationFailed(_) => ErrorCode::ThumbnailFailed,

            // 资源锁定
            AsterError::ResourceLocked(_) => ErrorCode::ResourceLocked,
            AsterError::PreconditionFailed(_) => ErrorCode::PreconditionFailed,

            // 分享
            AsterError::ShareNotFound(_) => ErrorCode::ShareNotFound,
            AsterError::ShareExpired(_) => ErrorCode::ShareExpired,
            AsterError::SharePasswordRequired(_) => ErrorCode::SharePasswordRequired,
            AsterError::ShareDownloadLimit(_) => ErrorCode::ShareDownloadLimitReached,
        }
    }
}

// 穷举性静态检查：AsterError 每新增一个变体，必须同步更新 From 实现，
// 否则 const 断言会编译失败。
const _: () = assert!(
    crate::errors::ASTER_ERROR_VARIANT_COUNT == 40,
    "AsterError variant count mismatch: update the assertion or the From impl"
);

#[cfg(test)]
mod tests {
    use super::ErrorCode;
    use crate::api::subcode::ApiSubcode;
    use crate::errors::{AsterError, validation_error_with_subcode};

    #[test]
    fn token_missing_maps_to_token_missing_code() {
        let error = AsterError::auth_token_missing("missing token");

        assert_eq!(ErrorCode::from(&error), ErrorCode::TokenMissing);
    }

    #[test]
    fn invalid_credentials_maps_to_credentials_failed_code() {
        let error = AsterError::auth_invalid_credentials("wrong password");

        assert_eq!(ErrorCode::from(&error), ErrorCode::CredentialsFailed);
    }

    #[test]
    fn mfa_error_subcodes_map_to_mfa_failed_code() {
        for subcode in [
            ApiSubcode::AuthMfaFlowInvalid,
            ApiSubcode::AuthMfaFlowExpired,
            ApiSubcode::AuthMfaCodeInvalid,
            ApiSubcode::AuthMfaAttemptsExceeded,
            ApiSubcode::AuthMfaFactorRequired,
            ApiSubcode::AuthMfaRecoveryCodeUsed,
        ] {
            let error = crate::errors::auth_mfa_failed_with_subcode(subcode, "mfa failed");

            assert_eq!(ErrorCode::from(&error), ErrorCode::MfaFailed);
        }
    }

    #[test]
    fn validation_conflict_subcodes_map_to_conflict() {
        for subcode in [
            ApiSubcode::AuthUsernameExists,
            ApiSubcode::AuthEmailExists,
            ApiSubcode::AuthIdentifierExists,
            ApiSubcode::AuthMfaFactorAlreadyExists,
            ApiSubcode::FileNameConflict,
            ApiSubcode::FolderNameConflict,
            ApiSubcode::TeamMemberExists,
            ApiSubcode::WebdavUsernameExists,
            ApiSubcode::RemoteNodeUniqueConflict,
        ] {
            let error = validation_error_with_subcode(subcode, "already exists");

            assert_eq!(ErrorCode::from(&error), ErrorCode::Conflict);
        }
    }

    #[test]
    fn validation_non_conflict_subcodes_stay_bad_request() {
        for subcode in [
            ApiSubcode::UploadEmptyFile,
            ApiSubcode::ArchivePreviewUnsupportedType,
            ApiSubcode::ManagedIngressDriverUnsupported,
            ApiSubcode::PolicyUploadSessionsExist,
        ] {
            let error = validation_error_with_subcode(subcode, "invalid request");

            assert_eq!(ErrorCode::from(&error), ErrorCode::BadRequest);
        }
    }
}
