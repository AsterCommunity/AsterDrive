//! 远端节点内部对象协议与客户端。

mod auth;
mod client;
mod errors;
mod follower_auth;
mod models;
mod observation;
mod primary_auth;
mod runtime;
#[cfg(test)]
mod tests;
mod transport;
pub mod tunnel;

pub(crate) use auth::verify_internal_request_signature;
pub use auth::verify_internal_response_signature;
pub use auth::{
    body_sha256_hex, empty_body_sha256, normalize_remote_base_url, sign_internal_request,
    sign_internal_response_signature, sign_presigned_request, verify_presigned_request_signature,
};
pub use client::RemoteStorageClient;
pub(crate) use follower_auth::send_signed_master_request;
pub(crate) use models::ApiEnvelope;
pub use models::{
    INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION,
    INTERNAL_STORAGE_MIN_SUPPORTED_PROTOCOL_VERSION_LABEL, INTERNAL_STORAGE_PROTOCOL_VERSION,
    INTERNAL_STORAGE_PROTOCOL_VERSION_LABEL, REMOTE_BROWSER_PRESIGNED_CORS_ALLOWED_HEADERS,
    REMOTE_BROWSER_PRESIGNED_CORS_GET_EXPOSE_HEADERS,
    REMOTE_BROWSER_PRESIGNED_CORS_PUT_EXPOSE_HEADERS, RemoteBindingDesiredState,
    RemoteBindingSyncRequest, RemoteCreateStorageTargetRequest, RemoteMultipartCapabilities,
    RemoteStorageBrowserCorsContract, RemoteStorageCapabilities, RemoteStorageCapacityResponse,
    RemoteStorageComposeRequest, RemoteStorageComposeResponse, RemoteStorageFeatureFlags,
    RemoteStorageListResponse, RemoteStorageObjectMetadata, RemoteStorageProtocolLimits,
    RemoteStorageTargetCapabilities, RemoteStorageTargetConnectorCatalog, RemoteStorageTargetInfo,
    RemoteStorageTargetRuntimeCapabilities, RemoteUpdateStorageTargetRequest,
};
pub(crate) use primary_auth::authorize_remote_node_request;
pub use runtime::RemoteProtocolRuntime;
pub(crate) use transport::{path_and_query_for_url, read_reqwest_response_body_limited};

pub const INTERNAL_STORAGE_BASE_PATH: &str = "/api/v1/internal/storage";
pub const REMOTE_NODE_BINDING_STATE_PATH: &str =
    "/api/v1/internal/remote-node-control/binding-state";
pub const INTERNAL_AUTH_ACCESS_KEY_HEADER: &str = "x-aster-access-key";
pub const INTERNAL_AUTH_TIMESTAMP_HEADER: &str = "x-aster-timestamp";
pub const INTERNAL_AUTH_NONCE_HEADER: &str = "x-aster-nonce";
pub const INTERNAL_AUTH_SIGNATURE_HEADER: &str = "x-aster-signature";
/// 签名声明的 body digest：与 canonical 第 6 行同值，follower 在收 body 前
/// 即可用它重建 canonical 验签，收完 tee 对账。
pub const INTERNAL_CONTENT_SHA256_HEADER: &str = "x-aster-content-sha256";

/// 读取请求声明的 body digest（缺失按空串：只可能来自非 v7 客户端，验签
/// 会以空串 canonical 自然失败）。
pub fn declared_content_sha256(headers: &actix_web::http::header::HeaderMap) -> String {
    headers
        .get(INTERNAL_CONTENT_SHA256_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}
/// 对象上传响应携带的 body digest 与响应签名（v7：占位签请求靠响应对账闭环）。
pub const INTERNAL_RESPONSE_OBJECT_SHA256_HEADER: &str = "x-aster-object-sha256";
pub const INTERNAL_RESPONSE_SIGNATURE_HEADER: &str = "x-aster-response-signature";
pub const INTERNAL_RESPONSE_NONCE_HEADER: &str = "x-aster-response-nonce";
pub const INTERNAL_AUTH_SKEW_SECS: i64 = 300;
/// Nonce entries must outlive the timestamp acceptance window so a replay near
/// the window edge cannot race the nonce expiry. Keep this at least 2x the skew.
pub const INTERNAL_AUTH_NONCE_TTL_SECS: u64 = 2 * INTERNAL_AUTH_SKEW_SECS.unsigned_abs();
pub const PRESIGNED_AUTH_ACCESS_KEY_QUERY: &str = "aster_access_key";
pub const PRESIGNED_AUTH_EXPIRES_QUERY: &str = "aster_expires";
pub const PRESIGNED_AUTH_SIGNATURE_QUERY: &str = "aster_signature";
pub const PRESIGNED_RESPONSE_CACHE_CONTROL_QUERY: &str = "response-cache-control";
pub const PRESIGNED_RESPONSE_CONTENT_DISPOSITION_QUERY: &str = "response-content-disposition";
pub const PRESIGNED_RESPONSE_CONTENT_TYPE_QUERY: &str = "response-content-type";
pub const REMOTE_STORAGE_TARGET_KEY_QUERY: &str = "target_key";
pub const REMOTE_POLICY_MAX_FILE_SIZE_QUERY: &str = "max_file_size";
pub const REMOTE_CONTROL_PLANE_BODY_LIMIT: usize = 1024 * 1024;
/// Bounds each list page while allowing the client to aggregate an arbitrarily long listing.
pub const REMOTE_LIST_PAGE_SIZE: usize = 1000;
/// Eight MiB gives each item in a full page roughly eight KiB of serialized budget while staying
/// well below the reverse-tunnel buffered response ceiling.
pub const REMOTE_LIST_PAGE_BODY_LIMIT: usize = 8 * 1024 * 1024;
