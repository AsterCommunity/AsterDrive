use crate::api::error_code::ErrorCode;
use crate::api::subcode::ApiSubcode;
use crate::errors::{AsterError, Result, precondition_failed_with_subcode};
use crate::storage::error::{
    StorageErrorKind, storage_driver_error, storage_driver_error_with_subcode,
};
use serde::Deserialize;

use super::models::{ApiEnvelope, RemoteIngressProfileInfo};

#[derive(Debug, Deserialize)]
struct RemoteErrorEnvelope {
    code: i32,
    msg: String,
    error: Option<RemoteErrorInfo>,
}

#[derive(Debug, Deserialize)]
struct RemoteErrorInfo {
    subcode: Option<String>,
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

pub(super) async fn ensure_success(response: reqwest::Response, context: &str) -> Result<Vec<u8>> {
    let response = ensure_success_response(response, context).await?;
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(map_reqwest_error)
}

pub(super) async fn ensure_success_without_body(
    response: reqwest::Response,
    context: &str,
) -> Result<()> {
    ensure_success_response(response, context).await?;
    Ok(())
}

pub(super) async fn ensure_success_response(
    response: reqwest::Response,
    context: &str,
) -> Result<reqwest::Response> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(build_remote_status_error(response, context, false).await)
    }
}

pub(super) async fn build_remote_status_error(
    response: reqwest::Response,
    context: &str,
    not_found_as_record: bool,
) -> AsterError {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    build_remote_status_error_from_parts(status, &body, context, not_found_as_record)
}

pub(super) fn build_remote_status_error_from_parts(
    status: reqwest::StatusCode,
    body: &str,
    context: &str,
    not_found_as_record: bool,
) -> AsterError {
    let envelope = serde_json::from_str::<RemoteErrorEnvelope>(body).ok();
    let remote_code = envelope.as_ref().map(|value| value.code);
    let remote_subcode = envelope
        .as_ref()
        .and_then(|value| value.error.as_ref())
        .and_then(|value| value.subcode.as_deref())
        .and_then(ApiSubcode::parse);
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
    if let Some(error) = remote_code.and_then(|code| remote_api_error(code, &message)) {
        return error;
    }
    let kind = remote_code
        .and_then(remote_api_error_kind)
        .unwrap_or_else(|| remote_status_error_kind(status));
    let is_not_found_remote_code = remote_code
        .map(|code| {
            [
                ErrorCode::NotFound as i32,
                ErrorCode::StorageObjectNotFound as i32,
            ]
            .contains(&code)
        })
        .unwrap_or(false);

    match status {
        reqwest::StatusCode::NOT_FOUND if not_found_as_record || is_not_found_remote_code => {
            AsterError::record_not_found(message)
        }
        reqwest::StatusCode::PRECONDITION_FAILED => {
            let subcode = remote_subcode.unwrap_or(ApiSubcode::StoragePrecondition);
            precondition_failed_with_subcode(subcode, message)
        }
        _ => remote_subcode
            .map(|subcode| storage_driver_error_with_subcode(kind, subcode, message.clone()))
            .unwrap_or_else(|| storage_driver_error(kind, message)),
    }
}

pub(super) async fn parse_ingress_profile_response(
    response: reqwest::Response,
    context: &str,
) -> Result<RemoteIngressProfileInfo> {
    let body = ensure_success(response, context).await?;
    let envelope: ApiEnvelope<RemoteIngressProfileInfo> =
        serde_json::from_slice(&body).map_err(|e| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("decode remote ingress profile response: {e}"),
            )
        })?;
    if envelope.code != 0 {
        return Err(storage_driver_error(
            remote_api_error_kind(envelope.code).unwrap_or(StorageErrorKind::Unknown),
            format!("{context} failed: {}", envelope.msg),
        ));
    }
    envelope.data.ok_or_else(|| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("{context} response missing data"),
        )
    })
}

pub(super) fn remote_api_error(code: i32, message: &str) -> Option<AsterError> {
    match code {
        code if code == ErrorCode::StorageQuotaExceeded as i32 => {
            Some(AsterError::storage_quota_exceeded(message.to_string()))
        }
        _ => None,
    }
}

pub(super) fn remote_api_error_kind(code: i32) -> Option<StorageErrorKind> {
    match code {
        code if code == ErrorCode::BadRequest as i32 => Some(StorageErrorKind::Misconfigured),
        code if code == ErrorCode::StoragePolicyNotFound as i32
            || code == ErrorCode::StorageMisconfigured as i32 =>
        {
            Some(StorageErrorKind::Misconfigured)
        }
        code if code == ErrorCode::NotFound as i32
            || code == ErrorCode::FileNotFound as i32
            || code == ErrorCode::UploadSessionNotFound as i32
            || code == ErrorCode::StorageObjectNotFound as i32 =>
        {
            Some(StorageErrorKind::NotFound)
        }
        code if code == ErrorCode::RateLimited as i32
            || code == ErrorCode::StorageRateLimited as i32 =>
        {
            Some(StorageErrorKind::RateLimited)
        }
        code if code == ErrorCode::AuthFailed as i32
            || code == ErrorCode::TokenExpired as i32
            || code == ErrorCode::TokenInvalid as i32
            || code == ErrorCode::TokenMissing as i32
            || code == ErrorCode::CredentialsFailed as i32
            || code == ErrorCode::MfaFailed as i32
            || code == ErrorCode::StorageAuthFailed as i32 =>
        {
            Some(StorageErrorKind::Auth)
        }
        code if code == ErrorCode::Forbidden as i32
            || code == ErrorCode::StoragePermissionDenied as i32 =>
        {
            Some(StorageErrorKind::Permission)
        }
        code if code == ErrorCode::PreconditionFailed as i32 => {
            Some(StorageErrorKind::Precondition)
        }
        code if code == ErrorCode::StoragePreconditionFailed as i32 => {
            Some(StorageErrorKind::Precondition)
        }
        code if code == ErrorCode::UnsupportedDriver as i32
            || code == ErrorCode::StorageOperationUnsupported as i32 =>
        {
            Some(StorageErrorKind::Unsupported)
        }
        code if code == ErrorCode::StorageTransientFailure as i32 => {
            Some(StorageErrorKind::Transient)
        }
        code if code == ErrorCode::StorageDriverError as i32 => Some(StorageErrorKind::Unknown),
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
