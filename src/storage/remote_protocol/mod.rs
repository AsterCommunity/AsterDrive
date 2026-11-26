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

/// 远端节点内部存储协议所有端点的公共路径前缀。
pub const INTERNAL_STORAGE_BASE_PATH: &str = "/api/v1/internal/storage";
/// Primary 暴露给已注册 follower 轮询 binding 期望状态的控制面端点。
pub const REMOTE_NODE_BINDING_STATE_PATH: &str =
    "/api/v1/internal/remote-node-control/binding-state";
/// 内部协议签名头：follower 节点的 access key（同时作为连接注册键）。
pub const INTERNAL_AUTH_ACCESS_KEY_HEADER: &str = "x-aster-access-key";
/// 内部协议签名头：签名时的 Unix 时间戳（秒），与 [`INTERNAL_AUTH_SKEW_SECS`] 一起防重放。
pub const INTERNAL_AUTH_TIMESTAMP_HEADER: &str = "x-aster-timestamp";
/// 内部协议签名头：一次性 nonce，消费后在 nonce TTL 内拒绝重放。
pub const INTERNAL_AUTH_NONCE_HEADER: &str = "x-aster-nonce";
/// 内部协议签名头：六行 canonical 的 HMAC-SHA256 hex 值。
pub const INTERNAL_AUTH_SIGNATURE_HEADER: &str = "x-aster-signature";
/// 签名声明的 body digest：与 canonical 第 6 行同值，follower 在收 body 前
/// 即可用它重建 canonical 验签，收完 tee 对账。
pub const INTERNAL_CONTENT_SHA256_HEADER: &str = "x-aster-content-sha256";
/// Primary-to-broker metadata consumed before a reverse-tunnel request reaches a follower.
pub const INTERNAL_EXPECTED_RESPONSE_NONCE_HEADER: &str = "x-aster-expected-response-nonce";

/// 读取请求声明的 body digest（缺失按空串：只可能来自非 v7 客户端，验签
/// 会以空串 canonical 自然失败）。
pub fn declared_content_sha256(headers: &actix_web::http::header::HeaderMap) -> String {
    headers
        .get(INTERNAL_CONTENT_SHA256_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

/// Verify a bounded control-plane body's actual bytes against the digest covered
/// by request authentication. Call before deserialization or any completion effect.
pub fn verify_buffered_content_sha256(
    headers: &actix_web::http::header::HeaderMap,
    body: &[u8],
) -> crate::errors::Result<()> {
    let declared = declared_content_sha256(headers);
    if declared.is_empty() || declared != body_sha256_hex(body) {
        return Err(crate::errors::AsterError::auth_invalid_credentials(
            "request body does not match the signed digest",
        ));
    }
    Ok(())
}

/// 对象上传响应携带的 body digest 与响应签名（v7：占位签请求靠响应对账闭环）。
/// 对象上传/组合响应携带的 body digest（hex，响应签名 canonical 的一部分）。
pub const INTERNAL_RESPONSE_OBJECT_SHA256_HEADER: &str = "x-aster-object-sha256";
/// 对象上传/组合响应携带的响应签名（HMAC hex，canonical 见
/// [`sign_internal_response_signature`]）。
pub const INTERNAL_RESPONSE_SIGNATURE_HEADER: &str = "x-aster-response-signature";
/// 响应签名 canonical 复用的 nonce，回传请求方用于对账与防重放绑定。
pub const INTERNAL_RESPONSE_NONCE_HEADER: &str = "x-aster-response-nonce";
/// 请求签名时间戳允许的时钟偏差（秒），超出即拒绝。
pub const INTERNAL_AUTH_SKEW_SECS: i64 = 300;
/// Nonce entries must outlive the timestamp acceptance window so a replay near
/// the window edge cannot race the nonce expiry. Keep this at least 2x the skew.
pub const INTERNAL_AUTH_NONCE_TTL_SECS: u64 = 2 * INTERNAL_AUTH_SKEW_SECS.unsigned_abs();
/// Presigned URL 认证 query 参数：access key。
pub const PRESIGNED_AUTH_ACCESS_KEY_QUERY: &str = "aster_access_key";
/// Presigned URL 认证 query 参数：过期时间（Unix 秒）。
pub const PRESIGNED_AUTH_EXPIRES_QUERY: &str = "aster_expires";
/// Presigned URL 认证 query 参数：presigned 签名（HMAC hex）。
pub const PRESIGNED_AUTH_SIGNATURE_QUERY: &str = "aster_signature";
/// Presigned GET 响应覆盖 query 参数：覆盖响应的 `Cache-Control`。
pub const PRESIGNED_RESPONSE_CACHE_CONTROL_QUERY: &str = "response-cache-control";
/// Presigned GET 响应覆盖 query 参数：覆盖响应的 `Content-Disposition`。
pub const PRESIGNED_RESPONSE_CONTENT_DISPOSITION_QUERY: &str = "response-content-disposition";
/// Presigned GET 响应覆盖 query 参数：覆盖响应的 `Content-Type`。
pub const PRESIGNED_RESPONSE_CONTENT_TYPE_QUERY: &str = "response-content-type";
/// 对象面请求限定存储 target 的 query 参数（值经 URL 编码）。
pub const REMOTE_STORAGE_TARGET_KEY_QUERY: &str = "target_key";
/// 对象面请求携带策略侧单文件大小上限的 query 参数（字节）。
pub const REMOTE_POLICY_MAX_FILE_SIZE_QUERY: &str = "max_file_size";
/// 控制面 JSON 请求/响应 body 的统一字节上限；反向隧道与代理路径均按它预算。
pub const REMOTE_CONTROL_PLANE_BODY_LIMIT: usize = 1024 * 1024;
/// Bounds each list page while allowing the client to aggregate an arbitrarily long listing.
pub const REMOTE_LIST_PAGE_SIZE: usize = 1000;
/// Eight MiB gives each item in a full page roughly eight KiB of serialized budget while staying
/// well below the reverse-tunnel buffered response ceiling.
pub const REMOTE_LIST_PAGE_BODY_LIMIT: usize = 8 * 1024 * 1024;
