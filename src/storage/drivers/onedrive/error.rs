use reqwest::StatusCode;
use serde::Deserialize;

use crate::errors::AsterError;
use crate::storage::error::{StorageErrorKind, storage_driver_error};

#[derive(Debug, Deserialize)]
pub(super) struct MicrosoftGraphErrorResponse {
    #[serde(default)]
    pub error: Option<MicrosoftGraphErrorBody>,
}

#[derive(Debug, Deserialize)]
pub(super) struct MicrosoftGraphErrorBody {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

pub(super) fn map_reqwest_error(ctx: &str, error: reqwest::Error) -> AsterError {
    if error.is_timeout() {
        return storage_driver_error(
            StorageErrorKind::Transient,
            format!("{ctx}: Microsoft Graph request timed out"),
        );
    }
    if error.is_connect() || error.is_request() {
        return storage_driver_error(
            StorageErrorKind::Transient,
            format!("{ctx}: Microsoft Graph request failed: {error}"),
        );
    }
    storage_driver_error(
        StorageErrorKind::Unknown,
        format!("{ctx}: Microsoft Graph request failed: {error}"),
    )
}

pub(super) async fn map_graph_response_error(ctx: &str, response: reqwest::Response) -> AsterError {
    let status = response.status();
    let request_id = response
        .headers()
        .get("request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| {
            response
                .headers()
                .get("client-request-id")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        });
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = response.json::<MicrosoftGraphErrorResponse>().await.ok();
    let (code, message) = body
        .and_then(|body| body.error)
        .map(|error| (error.code, error.message))
        .unwrap_or((None, None));

    let mut details = vec![format!("http_status={}", status.as_u16())];
    if let Some(code) = code.as_deref().filter(|code| !code.trim().is_empty()) {
        details.push(format!("code={code}"));
    }
    if let Some(message) = message
        .as_deref()
        .filter(|message| !message.trim().is_empty())
    {
        details.push(format!("message={message}"));
    }
    if let Some(request_id) = request_id {
        details.push(format!("request_id={request_id}"));
    }
    if let Some(retry_after) = retry_after {
        details.push(format!("retry_after={retry_after}"));
    }

    storage_driver_error(
        classify_graph_error(status, code.as_deref()),
        format!("{ctx}: Microsoft Graph error: {}", details.join(", ")),
    )
}

pub(super) fn classify_graph_error(status: StatusCode, code: Option<&str>) -> StorageErrorKind {
    if let Some(code) = code {
        match code {
            "InvalidAuthenticationToken" | "AuthenticationFailure" | "TokenExpired" => {
                return StorageErrorKind::Auth;
            }
            "accessDenied" | "AccessDenied" | "Authorization_RequestDenied" => {
                return StorageErrorKind::Permission;
            }
            "itemNotFound" | "ItemNotFound" | "ResourceNotFound" => {
                return StorageErrorKind::NotFound;
            }
            "nameAlreadyExists" | "resourceModified" | "preconditionFailed" => {
                return StorageErrorKind::Precondition;
            }
            "TooManyRequests" | "activityLimitReached" | "quotaLimitReached" => {
                return StorageErrorKind::RateLimited;
            }
            "BadRequest" | "invalidRequest" | "invalidRange" => {
                return StorageErrorKind::Misconfigured;
            }
            _ => {}
        }
    }

    match status {
        StatusCode::UNAUTHORIZED => StorageErrorKind::Auth,
        StatusCode::FORBIDDEN => StorageErrorKind::Permission,
        StatusCode::NOT_FOUND => StorageErrorKind::NotFound,
        StatusCode::CONFLICT | StatusCode::PRECONDITION_FAILED => StorageErrorKind::Precondition,
        StatusCode::TOO_MANY_REQUESTS => StorageErrorKind::RateLimited,
        StatusCode::BAD_REQUEST => StorageErrorKind::Misconfigured,
        StatusCode::INTERNAL_SERVER_ERROR
        | StatusCode::BAD_GATEWAY
        | StatusCode::SERVICE_UNAVAILABLE
        | StatusCode::GATEWAY_TIMEOUT => StorageErrorKind::Transient,
        _ => StorageErrorKind::Unknown,
    }
}

pub(super) fn invalid_graph_url(error: url::ParseError) -> AsterError {
    storage_driver_error(
        StorageErrorKind::Misconfigured,
        format!("invalid Microsoft Graph URL: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_error_classification_handles_common_statuses_and_codes() {
        assert_eq!(
            classify_graph_error(StatusCode::UNAUTHORIZED, Some("InvalidAuthenticationToken")),
            StorageErrorKind::Auth
        );
        assert_eq!(
            classify_graph_error(StatusCode::FORBIDDEN, Some("accessDenied")),
            StorageErrorKind::Permission
        );
        assert_eq!(
            classify_graph_error(StatusCode::NOT_FOUND, Some("itemNotFound")),
            StorageErrorKind::NotFound
        );
        assert_eq!(
            classify_graph_error(StatusCode::TOO_MANY_REQUESTS, None),
            StorageErrorKind::RateLimited
        );
    }
}
