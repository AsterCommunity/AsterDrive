//! Structured storage errors shared by drivers and product adapters.

/// Classifies a storage failure using a stable, transport-independent kind.
///
/// Drivers should assign a kind when they create an error so callers can make
/// retry, cleanup, and presentation decisions without parsing the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageErrorKind {
    /// Credentials were missing, rejected, or otherwise failed authentication.
    Auth,
    /// The storage policy or driver configuration is invalid.
    Misconfigured,
    /// The requested object, upload, or path does not exist.
    NotFound,
    /// The credentials are valid, but the operation is not permitted.
    Permission,
    /// The operation conflicts with a required state or precondition.
    Precondition,
    /// The provider asked the caller to slow down or retry after a delay.
    RateLimited,
    /// The failure is expected to be temporary and may be retried.
    Transient,
    /// The selected driver or provider does not implement the requested operation.
    Unsupported,
    /// No more reliable structured classification is available.
    #[default]
    Unknown,
}

/// Provider-specific diagnostic data attached to a storage error.
///
/// Context is machine-readable metadata for callers that need more detail
/// than [`StorageErrorKind`]. It must not contain secrets or credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageErrorContext {
    /// The SFTP server rejected the presented host key.
    SftpHostKeyRejected {
        /// The configured expected fingerprint, when one was configured.
        expected: Option<String>,
        /// The fingerprint presented by the server.
        actual: String,
    },
}

/// Structured error returned by the storage contract.
///
/// This type deliberately has no dependency on HTTP status codes, API error
/// codes, or the root `aster_drive` product error type. Product adapters can
/// map it at their boundary while drivers retain a stable storage contract.
#[derive(Debug, Clone)]
pub struct StorageError {
    kind: StorageErrorKind,
    message: String,
    context: Option<StorageErrorContext>,
}

impl StorageErrorKind {
    /// Returns the stable lowercase identifier used in logs and diagnostics.
    pub const fn as_str(self) -> &'static str {
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
}

impl StorageError {
    /// Creates a storage error with a structured kind and human-readable message.
    pub fn new(kind: StorageErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            context: None,
        }
    }

    /// Attaches provider-specific diagnostic context to this error.
    pub fn with_context(mut self, context: StorageErrorContext) -> Self {
        self.context = Some(context);
        self
    }

    /// Returns the structured classification without allocating.
    pub const fn kind(&self) -> StorageErrorKind {
        self.kind
    }

    /// Returns the human-readable error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns provider-specific context, when the driver supplied any.
    pub fn context(&self) -> Option<&StorageErrorContext> {
        self.context.as_ref()
    }

    /// Consumes the error and moves out its fields without cloning.
    ///
    /// This is intended for cross-crate error conversion where the product
    /// adapter needs to take ownership of the message and context.
    pub fn into_parts(self) -> (StorageErrorKind, String, Option<StorageErrorContext>) {
        (self.kind, self.message, self.context)
    }
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StorageError {}

/// Result alias for operations implemented against the storage contract.
pub type Result<T> = std::result::Result<T, StorageError>;

/// Creates a [`StorageError`] for a driver operation.
pub fn storage_driver_error(kind: StorageErrorKind, message: impl Into<String>) -> StorageError {
    StorageError::new(kind, message)
}

/// Creates a [`StorageError`] with provider-specific diagnostic context.
pub fn storage_driver_error_with_context(
    kind: StorageErrorKind,
    message: impl Into<String>,
    context: StorageErrorContext,
) -> StorageError {
    StorageError::new(kind, message).with_context(context)
}

/// Infers a structured kind from a legacy error message.
///
/// This is a compatibility fallback for old product errors that do not carry
/// a structured kind. It lowercases and scans the message, so it has extra
/// allocation and matching cost and must not be used on a new driver's normal
/// path. New code should pass [`StorageErrorKind`] directly when constructing
/// the error. [`StorageErrorKind::Unknown`] means that no reliable
/// classification was available; it is not a default success or retry state.
pub fn infer_storage_error_kind(message: &str) -> StorageErrorKind {
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
            "host key",
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

/// Converts arbitrary displayable provider errors into [`StorageError`].
pub trait MapStorageErr<T> {
    /// Maps an error while assigning the supplied structured kind.
    fn map_storage_err(self, kind: StorageErrorKind) -> Result<T>;

    /// Maps an error while prefixing its message with a static operation context.
    fn map_storage_err_ctx(self, kind: StorageErrorKind, context: &'static str) -> Result<T>;
}

impl<T, E> MapStorageErr<T> for std::result::Result<T, E>
where
    E: std::fmt::Display,
{
    fn map_storage_err(self, kind: StorageErrorKind) -> Result<T> {
        self.map_err(|error| StorageError::new(kind, error.to_string()))
    }

    fn map_storage_err_ctx(self, kind: StorageErrorKind, context: &'static str) -> Result<T> {
        self.map_err(|error| StorageError::new(kind, format!("{context}: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_storage_error_preserves_fields() {
        let error = storage_driver_error_with_context(
            StorageErrorKind::Precondition,
            "host key rejected",
            StorageErrorContext::SftpHostKeyRejected {
                expected: Some("SHA256:expected".to_string()),
                actual: "SHA256:actual".to_string(),
            },
        );

        assert_eq!(error.kind(), StorageErrorKind::Precondition);
        assert_eq!(error.message(), "host key rejected");
        assert!(matches!(
            error.context(),
            Some(StorageErrorContext::SftpHostKeyRejected { .. })
        ));
    }

    #[test]
    fn legacy_message_inference_remains_available_at_product_boundary() {
        assert_eq!(
            infer_storage_error_kind("remote request failed: connection reset"),
            StorageErrorKind::Transient
        );
    }
}
