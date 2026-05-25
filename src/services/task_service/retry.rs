//! Background task retry policy.

use crate::errors::AsterError;
use crate::storage::StorageErrorKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TaskRetryClass {
    Auto,
    Manual,
    Never,
}

impl TaskRetryClass {
    pub(super) fn should_auto_retry(self) -> bool {
        matches!(self, Self::Auto)
    }

    pub(super) fn can_manual_retry(self) -> bool {
        matches!(self, Self::Auto | Self::Manual)
    }
}

pub(super) trait TaskRetryPolicy {
    fn retry_class(error: &AsterError) -> TaskRetryClass {
        default_retry_class(error)
    }
}

pub(super) fn default_retry_class(error: &AsterError) -> TaskRetryClass {
    match error {
        AsterError::DatabaseConnection(_) | AsterError::RateLimited(_) => TaskRetryClass::Auto,
        AsterError::DatabaseOperation(_)
        | AsterError::ConfigError(_)
        | AsterError::InternalError(_) => TaskRetryClass::Manual,
        AsterError::StorageQuotaExceeded(_)
        | AsterError::ResourceLocked(_)
        | AsterError::StoragePolicyNotFound(_) => TaskRetryClass::Manual,
        AsterError::StorageDriverError(_) => match error.storage_error_kind() {
            Some(StorageErrorKind::Transient | StorageErrorKind::RateLimited) => {
                TaskRetryClass::Auto
            }
            Some(
                StorageErrorKind::Auth
                | StorageErrorKind::Misconfigured
                | StorageErrorKind::Permission
                | StorageErrorKind::Precondition
                | StorageErrorKind::Unknown,
            ) => TaskRetryClass::Manual,
            Some(StorageErrorKind::NotFound | StorageErrorKind::Unsupported) | None => {
                TaskRetryClass::Never
            }
        },
        AsterError::ValidationError(_)
        | AsterError::RecordNotFound(_)
        | AsterError::MailNotConfigured(_)
        | AsterError::MailDeliveryFailed(_)
        | AsterError::AuthInvalidCredentials(_)
        | AsterError::AuthTokenExpired(_)
        | AsterError::AuthTokenInvalid(_)
        | AsterError::AuthTokenMissing(_)
        | AsterError::AuthMfaFailed(_)
        | AsterError::AuthForbidden(_)
        | AsterError::AuthPendingActivation(_)
        | AsterError::ContactVerificationInvalid(_)
        | AsterError::ContactVerificationExpired(_)
        | AsterError::FileNotFound(_)
        | AsterError::FileTooLarge(_)
        | AsterError::FileTypeNotAllowed(_)
        | AsterError::FileUploadFailed(_)
        | AsterError::PayloadTooLarge(_)
        | AsterError::UnsupportedDriver(_)
        | AsterError::FolderNotFound(_)
        | AsterError::ShareNotFound(_)
        | AsterError::ShareExpired(_)
        | AsterError::SharePasswordRequired(_)
        | AsterError::ShareDownloadLimit(_)
        | AsterError::UploadSessionNotFound(_)
        | AsterError::UploadSessionExpired(_)
        | AsterError::ChunkUploadFailed(_)
        | AsterError::UploadAssemblyFailed(_)
        | AsterError::ThumbnailGenerationFailed(_)
        | AsterError::PreconditionFailed(_)
        | AsterError::UploadAssembling(_) => TaskRetryClass::Never,
    }
}
