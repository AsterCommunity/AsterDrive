//! Browser execution is deliberately finite; provider protocol logic stays in the driver.
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Opaque driver-owned protocol identity. Versions are interpreted by the driver, not the browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BrowserUploadProtocol {
    /// Driver-namespaced protocol identifier (opaque to the browser executor).
    pub id: String,
    /// Protocol revision; the driver decides whether it can service a pair.
    pub version: u32,
}

/// HTTP method the browser must use for one provider upload request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum BrowserUploadMethod {
    /// `GET` request.
    Get,
    /// `PUT` request.
    Put,
    /// `POST` request.
    Post,
    /// `PATCH` request.
    Patch,
    /// `DELETE` request.
    Delete,
}

/// Half-open file range [start, end). None means an empty request body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BrowserUploadRange {
    /// Byte offset of the first byte the browser sends (inclusive).
    pub start: u64,
    /// Byte offset one past the last byte the browser sends (exclusive).
    pub end: u64,
}

/// A capability URL, never an administrator credential. Do not log this value.
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BrowserUploadRequest {
    /// Provider URL to fetch; fully signed by the driver and never rewritten
    /// by callers.
    pub url: String,
    /// HTTP verb to use for this request.
    pub method: BrowserUploadMethod,
    /// Headers the browser must send verbatim (part of the signature).
    pub headers: BTreeMap<String, String>,
    /// Driver-issued, narrowly scoped upload capability, never an administrator credential.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<BrowserUploadAuthorization>,
    /// Numbered-part completion receipt; sequential session protocols leave this unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion: Option<BrowserUploadCompletion>,
    /// Byte range of the file the browser sends as the body; `None` sends an
    /// empty body.
    pub body: Option<BrowserUploadRange>,
    /// Only these response headers may be reported to the driver.
    pub response_headers: Vec<String>,
}

/// Credential material the browser attaches to one upload request.
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BrowserUploadAuthorization {
    /// Authentication scheme name (e.g. `Bearer`), sent as the prefix of the
    /// `Authorization` header.
    pub scheme: String,
    /// Capability token scoped to this upload; a secret that must not be
    /// logged.
    pub token: String,
}

/// Finite, provider-owned receipt rules consumed by the common browser executor.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum BrowserUploadCompletion {
    /// Completion token is read from a named response header (e.g. `ETag`).
    ResponseHeader {
        /// Response header whose value is the completion token.
        name: String,
    },
    /// Completion token is the driver-declared fixed value.
    Fixed {
        /// Token to report regardless of the response.
        token: String,
    },
    /// No token is derivable from the response; completion is decided later
    /// by reconciling authoritative provider state.
    ProviderReconciled,
}

impl std::fmt::Debug for BrowserUploadRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserUploadRequest")
            .finish_non_exhaustive()
    }
}

/// Untrusted browser observation. Status zero means the browser could not observe a response.
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BrowserUploadObservation {
    /// HTTP status observed by the browser; 0 means no response was observable.
    pub status: u16,
    /// Response headers, restricted to the request's declared allowlist.
    pub headers: BTreeMap<String, String>,
    /// Response body as captured by the browser layer; untrusted.
    pub body: String,
    /// Whether `body` was cut off before the full response was captured.
    pub body_truncated: bool,
}

/// Session-wide facts a driver needs to plan the next browser request.
pub struct BrowserUploadContext<'a> {
    /// Protocol identity negotiated for this upload session.
    pub protocol: &'a BrowserUploadProtocol,
    /// Provider session URL owning the upload.
    pub upload_url: &'a str,
    /// Final AsterDrive storage path this upload publishes into.
    pub storage_path: &'a str,
    /// Total upload size in bytes.
    pub total_size: u64,
    /// Size in bytes of each browser-side fragment request.
    pub fragment_size: u64,
}

/// What the browser observed for the request the driver previously issued.
pub struct BrowserUploadReceipt<'a> {
    /// The request that was executed.
    pub request: &'a BrowserUploadRequest,
    /// The untrusted observation returned by the browser for that request.
    pub observation: &'a BrowserUploadObservation,
}

/// Next step of a browser upload as decided by the driver.
pub struct BrowserUploadTransition {
    /// Must be reconstructible by advance(None) after an interrupted browser request.
    pub private_state: serde_json::Value,
    /// Contiguous bytes received, not bytes merely sent by the browser.
    pub confirmed_offset: u64,
    /// Provider completion is distinct from product publication/finalization.
    pub provider_completed: bool,
    /// Next browser request to execute; `None` when provider completion
    /// leaves nothing further to send.
    pub request: Option<BrowserUploadRequest>,
}

/// Optional provider protocol state machine. New implementations need no frontend parser.
/// advance(None) reconciles authoritative provider state, including after response loss.
/// Calls can race or be retried: do not perform non-idempotent writes here. Private state
/// and the next request are committed together by the product's optimistic concurrency guard.
#[async_trait]
pub trait BrowserUploadDriver: Send + Sync {
    /// Returns the protocol identity this driver implements.
    fn protocol(&self) -> BrowserUploadProtocol;

    /// Reports whether this driver can execute `protocol`.
    ///
    /// Defaults to identity comparison against [`Self::protocol`]; override
    /// when a driver deliberately serves several protocol versions.
    fn supports_protocol(&self, protocol: &BrowserUploadProtocol) -> bool {
        *protocol == self.protocol()
    }

    /// Advances the provider protocol state machine by one step.
    ///
    /// `receipt` carries what the browser observed for the previously issued
    /// request; `None` means the request was interrupted or never observed, in
    /// which case the driver must reconcile authoritative provider state (per
    /// the trait-level contract) rather than assume failure. Implementations
    /// return the updated private state, the confirmed byte offset, provider
    /// completion status, and the next request for the browser.
    async fn advance(
        &self,
        context: BrowserUploadContext<'_>,
        private_state: serde_json::Value,
        receipt: Option<BrowserUploadReceipt<'_>>,
    ) -> Result<BrowserUploadTransition>;
}
