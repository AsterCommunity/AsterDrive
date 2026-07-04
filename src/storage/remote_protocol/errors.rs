use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, precondition_failed_with_code};
use crate::storage::error::{
    StorageErrorKind, storage_driver_error, storage_driver_error_with_code,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RemoteErrorEnvelope {
    code: ApiErrorCode,
    msg: String,
}

pub(super) fn map_reqwest_error(error: reqwest::Error) -> AsterError {
    if error.is_timeout() {
        storage_driver_error(
            StorageErrorKind::Transient,
            format!("remote storage request timed out: {error}"),
        )
    } else {
        storage_driver_error(
            StorageErrorKind::Transient,
            format!("remote storage request failed: {error}"),
        )
    }
}

pub fn build_remote_status_error_from_parts(
    status: reqwest::StatusCode,
    body: &str,
    context: &str,
    not_found_as_record: bool,
) -> AsterError {
    let envelope = serde_json::from_str::<RemoteErrorEnvelope>(body).ok();
    let remote_code = envelope.as_ref().map(|value| value.code);
    let remote_api_code = remote_code;
    let remote_message = envelope
        .as_ref()
        .map(|envelope| envelope.msg.as_str())
        .filter(|msg| !msg.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| body.trim().to_string());
    let message = if remote_message.is_empty() {
        format!("{context}: remote node returned HTTP {status}")
    } else {
        format!("{context}: {remote_message}")
    };
    if let Some(error) = remote_api_code.and_then(|code| remote_api_error(code, &message)) {
        return error;
    }
    let kind = remote_api_code
        .and_then(remote_api_error_kind)
        .unwrap_or_else(|| remote_status_error_kind(status));
    let is_not_found_remote_code = remote_api_code
        .map(|code| {
            matches!(
                code,
                ApiErrorCode::NotFound
                    | ApiErrorCode::FileNotFound
                    | ApiErrorCode::UploadSessionNotFound
                    | ApiErrorCode::RemoteStorageTargetNotFound
                    | ApiErrorCode::StorageObjectNotFound
                    | ApiErrorCode::StorageNotFound
            )
        })
        .unwrap_or(false);

    match status {
        reqwest::StatusCode::NOT_FOUND if not_found_as_record || is_not_found_remote_code => {
            AsterError::record_not_found(message)
        }
        reqwest::StatusCode::PRECONDITION_FAILED => {
            let api_code = remote_api_code.unwrap_or(ApiErrorCode::StoragePrecondition);
            precondition_failed_with_code(api_code, message)
        }
        _ => remote_api_code
            .map(|api_code| storage_driver_error_with_code(kind, api_code, message.clone()))
            .unwrap_or_else(|| storage_driver_error(kind, message)),
    }
}

pub(super) fn remote_api_error(code: ApiErrorCode, message: &str) -> Option<AsterError> {
    match code {
        ApiErrorCode::StorageQuotaExceeded => {
            Some(AsterError::storage_quota_exceeded(message.to_string()))
        }
        _ => None,
    }
}

pub fn remote_api_error_kind(code: ApiErrorCode) -> Option<StorageErrorKind> {
    match code {
        ApiErrorCode::BadRequest
        | ApiErrorCode::StoragePolicyNotFound
        | ApiErrorCode::StorageMisconfigured => Some(StorageErrorKind::Misconfigured),
        ApiErrorCode::NotFound
        | ApiErrorCode::FileNotFound
        | ApiErrorCode::UploadSessionNotFound
        | ApiErrorCode::RemoteStorageTargetNotFound
        | ApiErrorCode::StorageObjectNotFound
        | ApiErrorCode::StorageNotFound => Some(StorageErrorKind::NotFound),
        ApiErrorCode::RateLimited | ApiErrorCode::StorageRateLimited => {
            Some(StorageErrorKind::RateLimited)
        }
        ApiErrorCode::AuthFailed
        | ApiErrorCode::TokenExpired
        | ApiErrorCode::TokenInvalid
        | ApiErrorCode::TokenMissing
        | ApiErrorCode::CredentialsFailed
        | ApiErrorCode::MfaFailed
        | ApiErrorCode::StorageAuthFailed
        | ApiErrorCode::StorageAuth => Some(StorageErrorKind::Auth),
        ApiErrorCode::Forbidden
        | ApiErrorCode::StoragePermissionDenied
        | ApiErrorCode::StoragePermission => Some(StorageErrorKind::Permission),
        ApiErrorCode::PreconditionFailed
        | ApiErrorCode::StoragePreconditionFailed
        | ApiErrorCode::StoragePrecondition => Some(StorageErrorKind::Precondition),
        ApiErrorCode::UnsupportedDriver
        | ApiErrorCode::StorageOperationUnsupported
        | ApiErrorCode::StorageUnsupported => Some(StorageErrorKind::Unsupported),
        ApiErrorCode::StorageTransientFailure | ApiErrorCode::StorageTransient => {
            Some(StorageErrorKind::Transient)
        }
        ApiErrorCode::StorageDriverError | ApiErrorCode::StorageUnknown => {
            Some(StorageErrorKind::Unknown)
        }
        _ => None,
    }
}

pub(super) fn remote_status_error_kind(status: reqwest::StatusCode) -> StorageErrorKind {
    match status {
        reqwest::StatusCode::BAD_REQUEST | reqwest::StatusCode::UNPROCESSABLE_ENTITY => {
            StorageErrorKind::Misconfigured
        }
        reqwest::StatusCode::UNAUTHORIZED => StorageErrorKind::Auth,
        reqwest::StatusCode::FORBIDDEN => StorageErrorKind::Permission,
        reqwest::StatusCode::NOT_FOUND => StorageErrorKind::NotFound,
        reqwest::StatusCode::CONFLICT | reqwest::StatusCode::PRECONDITION_FAILED => {
            StorageErrorKind::Precondition
        }
        reqwest::StatusCode::METHOD_NOT_ALLOWED | reqwest::StatusCode::NOT_IMPLEMENTED => {
            StorageErrorKind::Unsupported
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => StorageErrorKind::RateLimited,
        status if status.is_server_error() => StorageErrorKind::Transient,
        _ => StorageErrorKind::Unknown,
    }
}
