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
    pub id: String,
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum BrowserUploadMethod {
    Get,
    Put,
    Post,
    Patch,
    Delete,
}

/// Half-open file range [start, end). None means an empty request body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BrowserUploadRange {
    pub start: u64,
    pub end: u64,
}

/// A capability URL, never an administrator credential. Do not log this value.
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BrowserUploadRequest {
    pub url: String,
    pub method: BrowserUploadMethod,
    pub headers: BTreeMap<String, String>,
    /// Driver-issued, narrowly scoped upload capability, never an administrator credential.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<BrowserUploadAuthorization>,
    /// Numbered-part completion receipt; sequential session protocols leave this unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion: Option<BrowserUploadCompletion>,
    pub body: Option<BrowserUploadRange>,
    /// Only these response headers may be reported to the driver.
    pub response_headers: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BrowserUploadAuthorization {
    pub scheme: String,
    pub token: String,
}

/// Finite, provider-owned receipt rules consumed by the common browser executor.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum BrowserUploadCompletion {
    ResponseHeader { name: String },
    Fixed { token: String },
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
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
    pub body_truncated: bool,
}

pub struct BrowserUploadContext<'a> {
    pub protocol: &'a BrowserUploadProtocol,
    pub upload_url: &'a str,
    pub storage_path: &'a str,
    pub total_size: u64,
    pub fragment_size: u64,
}

pub struct BrowserUploadReceipt<'a> {
    pub request: &'a BrowserUploadRequest,
    pub observation: &'a BrowserUploadObservation,
}

pub struct BrowserUploadTransition {
    /// Must be reconstructible by advance(None) after an interrupted browser request.
    pub private_state: serde_json::Value,
    /// Contiguous bytes received, not bytes merely sent by the browser.
    pub confirmed_offset: u64,
    /// Provider completion is distinct from product publication/finalization.
    pub provider_completed: bool,
    pub request: Option<BrowserUploadRequest>,
}

/// Optional provider protocol state machine. New implementations need no frontend parser.
/// advance(None) reconciles authoritative provider state, including after response loss.
/// Calls can race or be retried: do not perform non-idempotent writes here. Private state
/// and the next request are committed together by the product's optimistic concurrency guard.
#[async_trait]
pub trait BrowserUploadDriver: Send + Sync {
    fn protocol(&self) -> BrowserUploadProtocol;
    fn supports_protocol(&self, protocol: &BrowserUploadProtocol) -> bool {
        *protocol == self.protocol()
    }
    async fn advance(
        &self,
        context: BrowserUploadContext<'_>,
        private_state: serde_json::Value,
        receipt: Option<BrowserUploadReceipt<'_>>,
    ) -> Result<BrowserUploadTransition>;
}
