//! 远端节点内部对象协议与客户端。

mod auth;
mod client;
mod errors;
mod models;
#[cfg(test)]
mod tests;

pub use auth::{normalize_remote_base_url, sign_internal_request, sign_presigned_request};
pub use client::RemoteStorageClient;
pub use models::{
    INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION,
    INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL, INTERNAL_STORAGE_PROTOCOL_VERSION,
    INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL, REMOTE_BROWSER_PRESIGNED_CORS_ALLOWED_HEADERS,
    REMOTE_BROWSER_PRESIGNED_CORS_GET_EXPOSE_HEADERS,
    REMOTE_BROWSER_PRESIGNED_CORS_PUT_EXPOSE_HEADERS, RemoteBindingSyncRequest,
    RemoteCreateIngressProfileRequest, RemoteCreateLocalIngressProfileRequest,
    RemoteCreateS3IngressProfileRequest, RemoteIngressProfileInfo,
    RemoteStorageBrowserCorsContract, RemoteStorageCapabilities, RemoteStorageComposeRequest,
    RemoteStorageComposeResponse, RemoteStorageFeatureFlags, RemoteStorageListResponse,
    RemoteStorageObjectMetadata, RemoteStorageProtocolLimits, RemoteUpdateIngressProfileRequest,
};

pub const INTERNAL_STORAGE_BASE_PATH: &str = "/api/v1/internal/storage";
pub const INTERNAL_AUTH_ACCESS_KEY_HEADER: &str = "x-aster-access-key";
pub const INTERNAL_AUTH_TIMESTAMP_HEADER: &str = "x-aster-timestamp";
pub const INTERNAL_AUTH_NONCE_HEADER: &str = "x-aster-nonce";
pub const INTERNAL_AUTH_SIGNATURE_HEADER: &str = "x-aster-signature";
pub const INTERNAL_AUTH_SKEW_SECS: i64 = 300;
pub const INTERNAL_AUTH_NONCE_TTL_SECS: u64 = 300;
pub const PRESIGNED_AUTH_ACCESS_KEY_QUERY: &str = "aster_access_key";
pub const PRESIGNED_AUTH_EXPIRES_QUERY: &str = "aster_expires";
pub const PRESIGNED_AUTH_SIGNATURE_QUERY: &str = "aster_signature";
pub const PRESIGNED_RESPONSE_CACHE_CONTROL_QUERY: &str = "response-cache-control";
pub const PRESIGNED_RESPONSE_CONTENT_DISPOSITION_QUERY: &str = "response-content-disposition";
pub const PRESIGNED_RESPONSE_CONTENT_TYPE_QUERY: &str = "response-content-type";
