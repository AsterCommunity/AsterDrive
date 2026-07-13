//! Background task retry policy.

use crate::errors::AsterError;
use crate::storage::StorageErrorKind;
use aster_forge_tasks::TaskRetryClass;

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
        | AsterError::AuthRefreshTokenStale(_)
        | AsterError::AuthRefreshTokenReuseDetected(_)
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

#[cfg(test)]
mod tests {
    use super::default_retry_class;
    use crate::errors::AsterError;
    use crate::storage::error::{StorageErrorKind, storage_driver_error};
    use aster_forge_tasks::TaskRetryClass;

    #[test]
    fn default_retry_class_groups_product_errors() {
        for error in [
            AsterError::database_connection("database unavailable"),
            AsterError::rate_limited("rate limited"),
            storage_driver_error(StorageErrorKind::Transient, "remote timeout"),
            storage_driver_error(StorageErrorKind::RateLimited, "provider throttled"),
        ] {
            assert_eq!(default_retry_class(&error), TaskRetryClass::Auto);
        }

        for error in [
            AsterError::database_operation("query failed"),
            AsterError::config_error("invalid runtime config"),
            AsterError::internal_error("unexpected state"),
            AsterError::storage_quota_exceeded("quota exceeded"),
            AsterError::resource_locked("resource locked"),
            AsterError::storage_policy_not_found("policy missing"),
            storage_driver_error(StorageErrorKind::Auth, "credential expired"),
            storage_driver_error(StorageErrorKind::Unknown, "unknown provider error"),
        ] {
            assert_eq!(default_retry_class(&error), TaskRetryClass::Manual);
        }

        for error in [
            AsterError::validation_error("invalid input"),
            AsterError::record_not_found("record missing"),
            storage_driver_error(StorageErrorKind::NotFound, "object missing"),
            storage_driver_error(StorageErrorKind::Unsupported, "unsupported operation"),
        ] {
            assert_eq!(default_retry_class(&error), TaskRetryClass::Never);
        }
    }

    #[test]
    fn forge_retry_capabilities_match_product_retry_classes() {
        assert!(TaskRetryClass::Auto.should_auto_retry());
        assert!(TaskRetryClass::Auto.can_manual_retry());
        assert!(!TaskRetryClass::Manual.should_auto_retry());
        assert!(TaskRetryClass::Manual.can_manual_retry());
        assert!(!TaskRetryClass::Never.should_auto_retry());
        assert!(!TaskRetryClass::Never.can_manual_retry());
    }
}
