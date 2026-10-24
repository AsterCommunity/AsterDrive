use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::errors::{AsterError, Result};

pub fn normalize_remote_base_url(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let mut url = reqwest::Url::parse(trimmed)
        .map_err(|e| AsterError::validation_error(format!("invalid remote node base_url: {e}")))?;
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(AsterError::validation_error(format!(
                "remote node base_url must use http/https, got '{other}'"
            )));
        }
    }
    url.set_query(None);
    url.set_fragment(None);
    while url.path().ends_with('/') && url.path() != "/" {
        let next = url.path().trim_end_matches('/').to_string();
        url.set_path(&next);
    }
    Ok(url.to_string().trim_end_matches('/').to_string())
}

pub fn sign_internal_request(
    secret_key: &str,
    method: &str,
    path_and_query: &str,
    timestamp: i64,
    nonce: &str,
    content_length: Option<u64>,
    body_sha256: &str,
) -> String {
    let digest = internal_request_mac(
        secret_key,
        method,
        path_and_query,
        timestamp,
        nonce,
        content_length,
        body_sha256,
    )
    .finalize()
    .into_bytes();
    hex::encode(digest)
}

/// 空请求体的规范 SHA-256 摘要（sha256("")），无 body 的签名请求必须携带
/// 该值，使"有 body"与"无 body"在签名层面不可互换。
pub fn empty_body_sha256() -> &'static str {
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}

/// 计算请求 body 的规范 SHA-256 摘要（hex）。签名输入格式的一部分，
/// 集成测试需要用它构造带 body 的合法签名。
pub fn body_sha256_hex(body: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(body);
    hex::encode(hasher.finalize())
}

#[expect(
    clippy::expect_used,
    reason = "HMAC-SHA256 accepts keys of every length, so construction with secret bytes is infallible"
)]
pub(crate) fn internal_request_mac(
    secret_key: &str,
    method: &str,
    path_and_query: &str,
    timestamp: i64,
    nonce: &str,
    content_length: Option<u64>,
    body_sha256: &str,
) -> Hmac<Sha256> {
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method,
        path_and_query,
        timestamp,
        nonce,
        content_length
            .map(|value| value.to_string())
            .unwrap_or_default(),
        body_sha256
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret_key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(canonical.as_bytes());
    mac
}

/// 验证内部协议签名，覆盖请求 body。
///
/// - `body_sha256: Some(digest)`：调用方已知 body digest（JSON 控制面的完整
///   body，或流式收完后的 tee digest），只接受与该 digest 精确匹配的签名；
/// - `body_sha256: None`：digest 发送时不可预知（relay 直推占位签）或端点无
///   body，接受流式占位与空 body digest 两种签名形态。占位请求的内容完整性
///   由响应签名对账补齐。
///
/// 不接受旧版 5 行 canonical：primary 与 follower 必须同时升级，混合版本
/// 部署的内部请求一律 401，不做滚动窗口。
#[expect(
    clippy::expect_used,
    clippy::too_many_arguments,
    reason = "HMAC-SHA256 accepts keys of every length, so construction with secret bytes is infallible; the eight-parameter form mirrors the six-line canonical plus verifier inputs"
)]
pub(crate) fn verify_internal_request_signature(
    secret_key: &str,
    method: &str,
    path_and_query: &str,
    timestamp: i64,
    nonce: &str,
    content_length: Option<u64>,
    body_sha256: Option<&str>,
    provided_signature: &str,
) -> bool {
    let mut decoded = [0u8; 32];
    if hex::decode_to_slice(provided_signature, &mut decoded).is_err() {
        return false;
    }

    let content_length = content_length
        .map(|value| value.to_string())
        .unwrap_or_default();
    let new_canonical = |body_sha256: &str| {
        format!("{method}\n{path_and_query}\n{timestamp}\n{nonce}\n{content_length}\n{body_sha256}")
    };
    let mut canonicals: Vec<String> = Vec::with_capacity(2);
    match body_sha256 {
        // 已知 digest：只接受精确覆盖该 digest 的签名，不回退占位
        Some(digest) => canonicals.push(new_canonical(digest)),
        // digest 未知：流式占位与空 body digest 两种形态
        None => {
            canonicals.push(new_canonical(""));
            canonicals.push(new_canonical(empty_body_sha256()));
        }
    }

    for canonical in canonicals {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret_key.as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(canonical.as_bytes());
        if mac.verify_slice(&decoded).is_ok() {
            return true;
        }
    }
    false
}

/// 对象上传响应的 body digest 签名：follower 收完流式 body 后把它算出的
/// sha256 签在响应头回传，primary 与本地 tee digest 比对，闭环 relay 直推
/// 这类发送时 digest 不可知路径的内容完整性。
///
/// canonical 复用请求 nonce，把响应绑定到单个请求，防重放与跨请求搬移。
pub fn sign_internal_response_signature(
    secret_key: &str,
    nonce: &str,
    body_sha256: &str,
) -> String {
    let mac = internal_response_mac(secret_key, nonce, body_sha256);
    hex::encode(mac.finalize().into_bytes())
}

pub fn verify_internal_response_signature(
    secret_key: &str,
    nonce: &str,
    body_sha256: &str,
    provided_signature: &str,
) -> bool {
    let mut decoded = [0u8; 32];
    if hex::decode_to_slice(provided_signature, &mut decoded).is_err() {
        return false;
    }
    let mac = internal_response_mac(secret_key, nonce, body_sha256);
    mac.verify_slice(&decoded).is_ok()
}

#[expect(
    clippy::expect_used,
    reason = "HMAC-SHA256 accepts keys of every length, so construction with secret bytes is infallible"
)]
fn internal_response_mac(secret_key: &str, nonce: &str, body_sha256: &str) -> Hmac<Sha256> {
    let canonical = format!("response\n{nonce}\n{body_sha256}");
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret_key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(canonical.as_bytes());
    mac
}

pub fn sign_presigned_request(
    secret_key: &str,
    method: &str,
    request_target: &str,
    access_key: &str,
    expires_at: i64,
) -> String {
    hex::encode(
        presigned_request_mac(secret_key, method, request_target, access_key, expires_at)
            .finalize()
            .into_bytes(),
    )
}

/// Verifies a presigned request signature in constant time relative to the
/// provided signature bytes. Keep this next to [`sign_presigned_request`] so the
/// canonical string stays a single source of truth.
pub fn verify_presigned_request_signature(
    secret_key: &str,
    method: &str,
    request_target: &str,
    access_key: &str,
    expires_at: i64,
    provided_signature: &str,
) -> bool {
    let mut decoded = [0u8; 32];
    if hex::decode_to_slice(provided_signature, &mut decoded).is_err() {
        return false;
    }

    presigned_request_mac(secret_key, method, request_target, access_key, expires_at)
        .verify_slice(&decoded)
        .is_ok()
}

#[expect(
    clippy::expect_used,
    reason = "HMAC-SHA256 accepts keys of every length, so construction with secret bytes is infallible"
)]
fn presigned_request_mac(
    secret_key: &str,
    method: &str,
    request_target: &str,
    access_key: &str,
    expires_at: i64,
) -> Hmac<Sha256> {
    let canonical = format!(
        "{}\n{}\n{}\n{}",
        method, request_target, access_key, expires_at
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret_key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(canonical.as_bytes());
    mac
}

#[cfg(test)]
mod tests {
    use super::{
        body_sha256_hex, empty_body_sha256, sign_internal_request, sign_presigned_request,
        verify_internal_request_signature, verify_presigned_request_signature,
    };

    #[test]
    fn internal_signature_covers_body_digest() {
        let body = br#"{"connector":"s3","secret":"rotate-me"}"#;
        let content_length = u64::try_from(body.len()).expect("test body length should fit u64");
        let args = (
            "secret",
            "POST",
            "/internal/storage/targets",
            1_700_000_i64,
            "nonce-1",
        );
        let signature = sign_internal_request(
            args.0,
            args.1,
            args.2,
            args.3,
            args.4,
            Some(content_length),
            &body_sha256_hex(body),
        );

        // 真实 body digest 通过
        assert!(verify_internal_request_signature(
            args.0,
            args.1,
            args.2,
            args.3,
            args.4,
            Some(content_length),
            Some(body_sha256_hex(body).as_str()),
            &signature
        ));
        // 声明 digest 被篡改后签名不再匹配
        assert!(!verify_internal_request_signature(
            args.0,
            args.1,
            args.2,
            args.3,
            args.4,
            Some(content_length),
            Some(body_sha256_hex(br#"{"connector":"s3","secret":"hijacked"}"#).as_str()),
            &signature
        ));
    }

    #[test]
    fn internal_signature_distinguishes_empty_body_from_absent_body_line() {
        let args = (
            "secret",
            "GET",
            "/internal/storage/capabilities",
            1_700_000_i64,
            "nonce-2",
        );

        // 无 body 请求签空 digest；接收端无论按 Some(空) 还是 None 验证都通过
        let empty = sign_internal_request(
            args.0,
            args.1,
            args.2,
            args.3,
            args.4,
            None,
            empty_body_sha256(),
        );
        assert!(verify_internal_request_signature(
            args.0,
            args.1,
            args.2,
            args.3,
            args.4,
            None,
            Some(empty_body_sha256()),
            &empty
        ));
        assert!(verify_internal_request_signature(
            args.0, args.1, args.2, args.3, args.4, None, None, &empty
        ));

        // 流式占位（body 行为空串）只在接收端无 body 时可用
        let streaming = sign_internal_request(args.0, args.1, args.2, args.3, args.4, None, "");
        assert!(verify_internal_request_signature(
            args.0, args.1, args.2, args.3, args.4, None, None, &streaming
        ));

        // 两种占位与真实 digest 互不相同
        assert_ne!(empty, streaming);
    }

    #[test]
    fn internal_signature_rejects_tampered_canonical_parts() {
        let body = b"payload".as_slice();
        let signature = sign_internal_request(
            "secret",
            "POST",
            "/internal/storage/compose",
            1_700_000,
            "nonce-4",
            Some(7),
            &body_sha256_hex(body),
        );

        assert!(!verify_internal_request_signature(
            "secret",
            "POST",
            "/internal/storage/compose",
            1_700_000,
            "nonce-4",
            Some(8),
            Some(body_sha256_hex(body).as_str()),
            &signature
        ));
        assert!(!verify_internal_request_signature(
            "secret",
            "POST",
            "/internal/storage/compose",
            1_700_000,
            "nonce-4",
            Some(7),
            Some(body_sha256_hex(b"payload2").as_str()),
            &signature
        ));
        assert!(!verify_internal_request_signature(
            "attacker",
            "POST",
            "/internal/storage/compose",
            1_700_000,
            "nonce-4",
            Some(7),
            Some(body_sha256_hex(body).as_str()),
            &signature
        ));
        assert!(!verify_internal_request_signature(
            "secret",
            "POST",
            "/internal/storage/compose",
            1_700_000,
            "nonce-4",
            Some(7),
            Some(body_sha256_hex(body).as_str()),
            "zz"
        ));
    }

    #[test]
    fn presigned_verification_accepts_issued_signature_and_rejects_tampering() {
        let signature = sign_presigned_request("secret", "PUT", "/objects/key", "ak", 1_700_000);

        assert!(verify_presigned_request_signature(
            "secret",
            "PUT",
            "/objects/key",
            "ak",
            1_700_000,
            &signature
        ));

        assert!(!verify_presigned_request_signature(
            "secret",
            "GET",
            "/objects/key",
            "ak",
            1_700_000,
            &signature
        ));
        assert!(!verify_presigned_request_signature(
            "secret",
            "PUT",
            "/objects/other",
            "ak",
            1_700_000,
            &signature
        ));
        assert!(!verify_presigned_request_signature(
            "secret",
            "PUT",
            "/objects/key",
            "other",
            1_700_000,
            &signature
        ));
        assert!(!verify_presigned_request_signature(
            "secret",
            "PUT",
            "/objects/key",
            "ak",
            1_700_001,
            &signature
        ));
        assert!(!verify_presigned_request_signature(
            "other-secret",
            "PUT",
            "/objects/key",
            "ak",
            1_700_000,
            &signature
        ));
        assert!(!verify_presigned_request_signature(
            "secret",
            "PUT",
            "/objects/key",
            "ak",
            1_700_000,
            "zz"
        ));
    }
}
