use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_s3::operation::{RequestId, RequestIdExt};
use std::error::Error as StdError;

use crate::errors::AsterError;
use crate::storage::drivers::s3_config::S3ConfigError;
use crate::storage::error::{StorageErrorKind, storage_driver_error};

use super::S3Driver;

impl S3Driver {
    const ERROR_BODY_PREVIEW_LIMIT: usize = 512;

    pub(super) fn rewrap_s3_config_error(err: S3ConfigError) -> AsterError {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            err.into_aster_error().message().to_string(),
        )
    }

    pub(super) fn rewrap_message_as_storage_error(err: AsterError) -> AsterError {
        storage_driver_error(StorageErrorKind::Misconfigured, err.message().to_string())
    }

    pub(super) fn error_chain(err: &dyn StdError) -> String {
        let mut parts = Vec::new();
        let mut current = Some(err);
        while let Some(err) = current {
            let message = err.to_string();
            if parts.last() != Some(&message) {
                parts.push(message);
            }
            current = err.source();
        }
        parts.join(": ")
    }

    pub(super) fn truncate_for_log(text: &str, limit: usize) -> String {
        let mut result = String::new();
        let mut chars = text.chars();
        for _ in 0..limit {
            let Some(ch) = chars.next() else {
                return result;
            };
            result.push(ch);
        }
        if chars.next().is_some() {
            result.push_str("...");
        }
        result
    }

    pub(super) fn extract_xml_tag(body: &str, tag: &str) -> Option<String> {
        let start_tag = format!("<{tag}>");
        let end_tag = format!("</{tag}>");
        let start = body.find(&start_tag)? + start_tag.len();
        let end = body[start..].find(&end_tag)? + start;
        let value = body[start..end].trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }

    pub(super) fn raw_body_preview(body: &str) -> Option<String> {
        let normalized = body.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty() {
            None
        } else {
            Some(Self::truncate_for_log(
                &normalized,
                Self::ERROR_BODY_PREVIEW_LIMIT,
            ))
        }
    }

    pub(super) fn format_sdk_error<E>(err: &SdkError<E>) -> String
    where
        E: StdError + ProvideErrorMetadata + Send + Sync + 'static,
    {
        let mut details = Vec::new();
        let mut code = err.code().map(str::to_string);
        let mut message = err.message().map(str::to_string);
        let mut request_id = err.request_id().map(str::to_string);
        let mut extended_request_id = err.extended_request_id().map(str::to_string);
        let mut http_status = None;
        let mut content_type = None;
        let mut raw_body = None;

        if let Some(raw) = err.raw_response() {
            http_status = Some(raw.status().as_u16());
            content_type = raw.headers().get("content-type").map(str::to_string);
            request_id =
                request_id.or_else(|| raw.headers().get("x-amz-request-id").map(str::to_string));
            extended_request_id =
                extended_request_id.or_else(|| raw.headers().get("x-amz-id-2").map(str::to_string));

            if let Some(bytes) = raw.body().bytes()
                && let Ok(body) = std::str::from_utf8(bytes)
            {
                code = code.or_else(|| Self::extract_xml_tag(body, "Code"));
                message = message.or_else(|| Self::extract_xml_tag(body, "Message"));
                request_id = request_id.or_else(|| Self::extract_xml_tag(body, "RequestId"));
                extended_request_id =
                    extended_request_id.or_else(|| Self::extract_xml_tag(body, "HostId"));
                raw_body = Self::raw_body_preview(body);
            }
        }

        let has_structured_error = code.is_some() || message.is_some();

        if let Some(http_status) = http_status {
            details.push(format!("http_status={http_status}"));
        }
        if let Some(code) = code {
            details.push(format!("code={code}"));
        }
        if let Some(message) = message {
            details.push(format!("message={message}"));
        }
        if let Some(request_id) = request_id {
            details.push(format!("request_id={request_id}"));
        }
        if let Some(extended_request_id) = extended_request_id {
            details.push(format!("extended_request_id={extended_request_id}"));
        }
        if let Some(content_type) = content_type {
            details.push(format!("content_type={content_type}"));
        }
        if !has_structured_error && let Some(raw_body) = raw_body {
            details.push(format!("raw_body={raw_body}"));
        }

        if details.is_empty() {
            Self::error_chain(err)
        } else {
            details.join(", ")
        }
    }

    pub(super) fn map_sdk_error<E>(ctx: &str, err: SdkError<E>) -> AsterError
    where
        E: StdError + ProvideErrorMetadata + Send + Sync + 'static,
    {
        let kind = Self::classify_sdk_error(&err);
        storage_driver_error(kind, format!("{ctx}: {}", Self::format_sdk_error(&err)))
    }

    pub(super) fn classify_sdk_error<E>(err: &SdkError<E>) -> StorageErrorKind
    where
        E: StdError + ProvideErrorMetadata + Send + Sync + 'static,
    {
        match err {
            SdkError::ConstructionFailure(_) => return StorageErrorKind::Misconfigured,
            SdkError::TimeoutError(_)
            | SdkError::DispatchFailure(_)
            | SdkError::ResponseError(_) => {
                return StorageErrorKind::Transient;
            }
            SdkError::ServiceError(_) => {}
            _ => return StorageErrorKind::Unknown,
        }

        if let Some(code) = err.code() {
            match code {
                "InvalidAccessKeyId"
                | "SignatureDoesNotMatch"
                | "AuthorizationHeaderMalformed"
                | "ExpiredToken"
                | "InvalidToken" => return StorageErrorKind::Auth,
                "AccessDenied" => return StorageErrorKind::Permission,
                "NoSuchKey" | "NoSuchUpload" | "NoSuchVersion" | "NotFound" => {
                    return StorageErrorKind::NotFound;
                }
                "NoSuchBucket"
                | "InvalidBucketName"
                | "AuthorizationQueryParametersError"
                | "InvalidURI"
                | "PermanentRedirect" => return StorageErrorKind::Misconfigured,
                "OperationAborted" | "PreconditionFailed" | "ConditionalRequestConflict" => {
                    return StorageErrorKind::Precondition;
                }
                "SlowDown"
                | "Throttling"
                | "ThrottlingException"
                | "TooManyRequestsException"
                | "RequestLimitExceeded" => return StorageErrorKind::RateLimited,
                "RequestTimeout"
                | "RequestTimeoutException"
                | "InternalError"
                | "InternalFailure"
                | "ServiceUnavailable" => return StorageErrorKind::Transient,
                "MethodNotAllowed" | "NotImplemented" => return StorageErrorKind::Unsupported,
                _ => {}
            }
        }

        if let Some(status) = err.raw_response().map(|raw| raw.status()) {
            match status.as_u16() {
                401 => return StorageErrorKind::Auth,
                403 => return StorageErrorKind::Permission,
                404 => return StorageErrorKind::NotFound,
                405 | 501 => return StorageErrorKind::Unsupported,
                409 | 412 => return StorageErrorKind::Precondition,
                429 => return StorageErrorKind::RateLimited,
                500 | 502 | 503 | 504 => return StorageErrorKind::Transient,
                _ => {}
            }
        }

        StorageErrorKind::Unknown
    }
}
