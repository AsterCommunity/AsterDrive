//! 存储错误分类与编码。

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::AsterError;

const STORAGE_ERROR_KIND_PREFIX: &str = "__ASTER_STORAGE_KIND__=";
const STORAGE_ERROR_KIND_SEPARATOR: &str = "::";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageErrorKind {
    Auth,
    Misconfigured,
    NotFound,
    Permission,
    Precondition,
    RateLimited,
    Transient,
    Unsupported,
    #[default]
    Unknown,
}

impl StorageErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::Misconfigured => "misconfigured",
            Self::NotFound => "not_found",
            Self::Permission => "permission",
            Self::Precondition => "precondition",
            Self::RateLimited => "rate_limited",
            Self::Transient => "transient",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "auth" => Some(Self::Auth),
            "misconfigured" => Some(Self::Misconfigured),
            "not_found" => Some(Self::NotFound),
            "permission" => Some(Self::Permission),
            "precondition" => Some(Self::Precondition),
            "rate_limited" => Some(Self::RateLimited),
            "transient" => Some(Self::Transient),
            "unsupported" => Some(Self::Unsupported),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

pub fn storage_driver_error(kind: StorageErrorKind, message: impl Into<String>) -> AsterError {
    AsterError::storage_driver_error(encode_storage_driver_error_message(kind, message.into()))
}

pub fn storage_driver_error_with_code(
    kind: StorageErrorKind,
    api_code: ApiErrorCode,
    message: impl Into<String>,
) -> AsterError {
    storage_driver_error(kind, message).with_api_error_code(api_code)
}

pub fn storage_driver_error_display_message(raw_message: &str) -> &str {
    split_encoded_storage_error_message(raw_message)
        .map(|(_, message)| message)
        .unwrap_or(raw_message)
}

pub fn storage_driver_error_kind_from_message(raw_message: &str) -> StorageErrorKind {
    split_encoded_storage_error_message(raw_message)
        .map(|(kind, _)| kind)
        .unwrap_or_else(|| infer_storage_error_kind(raw_message))
}

fn encode_storage_driver_error_message(kind: StorageErrorKind, message: String) -> String {
    format!(
        "{STORAGE_ERROR_KIND_PREFIX}{}{STORAGE_ERROR_KIND_SEPARATOR}{message}",
        kind.as_str()
    )
}

fn split_encoded_storage_error_message(raw_message: &str) -> Option<(StorageErrorKind, &str)> {
    let encoded = raw_message.strip_prefix(STORAGE_ERROR_KIND_PREFIX)?;
    let (kind, message) = encoded.split_once(STORAGE_ERROR_KIND_SEPARATOR)?;
    Some((StorageErrorKind::parse(kind)?, message))
}

fn infer_storage_error_kind(message: &str) -> StorageErrorKind {
    let message = message.to_ascii_lowercase();

    if contains_any(
        &message,
        &[
            "invalidaccesskeyid",
            "signaturedoesnotmatch",
            "authentication failed",
            "invalid credentials",
            "access_key cannot be empty",
            "secret_key cannot be empty",
        ],
    ) {
        return StorageErrorKind::Auth;
    }

    if contains_any(
        &message,
        &[
            "access forbidden",
            "accessdenied",
            "permission denied",
            "operation not permitted",
        ],
    ) {
        return StorageErrorKind::Permission;
    }

    if contains_any(
        &message,
        &[
            "remote node base_url must use",
            "invalid remote node base_url",
            "namespace cannot be empty",
            "missing remote_node_id",
            "not loaded in registry",
            "no such bucket",
            "nosuchbucket",
            "invalid bucket",
            "invalid storage path",
            "escapes base path",
            "base path",
            "not a directory",
            "local path has no",
            "cloudflare r2 endpoint",
            "does not match bucket field",
            "bucket is required",
            "base_url is required",
        ],
    ) {
        return StorageErrorKind::Misconfigured;
    }

    if contains_any(
        &message,
        &[
            "does not support multipart upload",
            "presigned put not supported",
            "stream upload not supported",
            "ingress policy does not support",
            "ingress target does not support",
        ],
    ) {
        return StorageErrorKind::Unsupported;
    }

    if contains_any(
        &message,
        &[
            "is disabled",
            "precondition failed",
            "master binding is disabled",
        ],
    ) {
        return StorageErrorKind::Precondition;
    }

    if contains_any(
        &message,
        &[
            "not found",
            "no such key",
            "nosuchkey",
            "no such upload",
            "nosuchupload",
            "404",
            "os error 2",
        ],
    ) {
        return StorageErrorKind::NotFound;
    }

    if contains_any(
        &message,
        &[
            "too many requests",
            "429",
            "slowdown",
            "slow down",
            "throttl",
        ],
    ) {
        return StorageErrorKind::RateLimited;
    }

    if contains_any(
        &message,
        &[
            "timed out",
            "request failed",
            "error sending request",
            "connection refused",
            "connection reset",
            "connection aborted",
            "broken pipe",
            "unexpected eof",
            "network is unreachable",
            "temporarily unavailable",
            "temporary failure",
            "dns error",
            "name or service not known",
            "failed to lookup address information",
            "connection closed before message completed",
            "service unavailable",
            "502",
            "503",
            "504",
            "500",
            "dispatch failure",
            "requesttimeout",
        ],
    ) {
        return StorageErrorKind::Transient;
    }

    StorageErrorKind::Unknown
}

fn contains_any(message: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| message.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tagged_storage_error_round_trips_kind_and_display_message() {
        let error = storage_driver_error(StorageErrorKind::Transient, "remote timeout");
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Transient)
        );
        assert_eq!(error.message(), "remote timeout");
    }

    #[test]
    fn untagged_storage_error_still_infers_kind_from_message() {
        let error = AsterError::storage_driver_error(
            "remote storage request failed: error sending request",
        );
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Transient)
        );
        assert_eq!(
            storage_driver_error_display_message(error.message()),
            "remote storage request failed: error sending request"
        );
    }

    #[test]
    fn untagged_local_storage_configuration_errors_infer_misconfigured() {
        for message in [
            "local storage readiness: base path '/tmp/file' is not a directory",
            "local path has no existing ancestor: /missing/path",
            "resolved storage path escapes base path: ../secret",
        ] {
            let error = AsterError::storage_driver_error(message);
            assert_eq!(
                error.storage_error_kind(),
                Some(StorageErrorKind::Misconfigured),
                "{message}"
            );
            assert_eq!(error.api_error_code(), ApiErrorCode::StorageMisconfigured);
        }
    }

    #[test]
    fn ingress_target_unsupported_message_stays_stable() {
        let error = AsterError::storage_driver_error("ingress target does not support");
        assert_eq!(error.message(), "ingress target does not support");
        assert_eq!(
            error.to_string(),
            "Storage Driver Error: ingress target does not support"
        );
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Unsupported)
        );
    }

    #[test]
    fn tagged_storage_error_preserves_nested_api_code() {
        let error = storage_driver_error_with_code(
            StorageErrorKind::Transient,
            ApiErrorCode::StorageTransient,
            "remote timeout",
        );
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Transient)
        );
        assert_eq!(
            error.api_error_code_override(),
            Some(ApiErrorCode::StorageTransient)
        );
        assert_eq!(error.message(), "remote timeout");
    }
}
