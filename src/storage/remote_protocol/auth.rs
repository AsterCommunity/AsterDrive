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
) -> String {
    let digest = internal_request_mac(
        secret_key,
        method,
        path_and_query,
        timestamp,
        nonce,
        content_length,
    )
    .finalize()
    .into_bytes();
    hex::encode(digest)
}

#[allow(clippy::expect_used)]
pub(crate) fn internal_request_mac(
    secret_key: &str,
    method: &str,
    path_and_query: &str,
    timestamp: i64,
    nonce: &str,
    content_length: Option<u64>,
) -> Hmac<Sha256> {
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}",
        method,
        path_and_query,
        timestamp,
        nonce,
        content_length
            .map(|value| value.to_string())
            .unwrap_or_default()
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret_key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(canonical.as_bytes());
    mac
}

#[allow(clippy::expect_used)]
pub fn sign_presigned_request(
    secret_key: &str,
    method: &str,
    request_target: &str,
    access_key: &str,
    expires_at: i64,
) -> String {
    let canonical = format!(
        "{}\n{}\n{}\n{}",
        method, request_target, access_key, expires_at
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret_key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(canonical.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}
