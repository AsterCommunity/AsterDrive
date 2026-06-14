use azure_core::error::ErrorKind;
use azure_core::http::StatusCode;
use azure_storage_blob::models::StorageErrorCode;

use crate::storage::error::StorageErrorKind;

use super::AzureBlobDriver;

impl AzureBlobDriver {
    pub(super) fn format_azure_error(error: azure_core::Error) -> String {
        crate::errors::sanitize_storage_driver_client_message(&error.to_string())
    }

    pub(super) fn classify_azure_error(error: &azure_core::Error) -> StorageErrorKind {
        if let ErrorKind::HttpResponse {
            status, error_code, ..
        } = error.kind()
        {
            if let Some(code) = error_code
                && let Ok(code) = code.parse::<StorageErrorCode>()
            {
                return match code {
                    StorageErrorCode::AuthenticationFailed
                    | StorageErrorCode::InvalidAuthenticationInfo => StorageErrorKind::Auth,
                    StorageErrorCode::AuthorizationFailure
                    | StorageErrorCode::AuthorizationPermissionMismatch
                    | StorageErrorCode::AuthorizationProtocolMismatch
                    | StorageErrorCode::AuthorizationResourceTypeMismatch
                    | StorageErrorCode::AuthorizationServiceMismatch
                    | StorageErrorCode::AuthorizationSourceIPMismatch
                    | StorageErrorCode::InsufficientAccountPermissions => {
                        StorageErrorKind::Permission
                    }
                    StorageErrorCode::BlobNotFound | StorageErrorCode::ContainerNotFound => {
                        StorageErrorKind::NotFound
                    }
                    StorageErrorCode::ConditionNotMet
                    | StorageErrorCode::AppendPositionConditionNotMet
                    | StorageErrorCode::BlobAlreadyExists => StorageErrorKind::Precondition,
                    StorageErrorCode::InvalidBlockId
                    | StorageErrorCode::InvalidBlockList
                    | StorageErrorCode::InvalidHeaderValue
                    | StorageErrorCode::InvalidInput
                    | StorageErrorCode::InvalidQueryParameterValue
                    | StorageErrorCode::InvalidRange
                    | StorageErrorCode::InvalidRequestUrl
                    | StorageErrorCode::InvalidResourceName => StorageErrorKind::Misconfigured,
                    StorageErrorCode::InternalError | StorageErrorCode::ServerBusy => {
                        StorageErrorKind::Transient
                    }
                    _ => StorageErrorKind::Unknown,
                };
            }

            return match *status {
                StatusCode::Unauthorized => StorageErrorKind::Auth,
                StatusCode::Forbidden => StorageErrorKind::Permission,
                StatusCode::NotFound => StorageErrorKind::NotFound,
                StatusCode::RequestTimeout
                | StatusCode::TooManyRequests
                | StatusCode::InternalServerError
                | StatusCode::BadGateway
                | StatusCode::ServiceUnavailable
                | StatusCode::GatewayTimeout => StorageErrorKind::Transient,
                _ => StorageErrorKind::Unknown,
            };
        }

        match error.kind() {
            ErrorKind::Io => StorageErrorKind::Transient,
            ErrorKind::DataConversion | ErrorKind::Other => StorageErrorKind::Misconfigured,
            _ => StorageErrorKind::Unknown,
        }
    }
}
