use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aster_drive_storage::MapStorageErr;
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::object_key;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use md5::{Digest, Md5};
use sha1::Sha1;
use url::Url;

use super::{QiniuDownloadAuthMode, QiniuDriver, QiniuStaticCredentials};

type HmacSha1 = Hmac<Sha1>;

const QINIU_KEY_SAFE_BYTES: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~/|";

/// Validate the connector-level delivery URL independently of provider
/// semantics. `normalize_download_base_url` normally runs first; this check
/// keeps direct driver construction from accepting a URL that could change
/// host, credentials, or query semantics after signing.
pub(super) fn validate_download_base_url(value: &str) -> aster_drive_storage::Result<()> {
    let raw_uri = value.parse::<http::Uri>().map_storage_err_ctx(
        StorageErrorKind::Misconfigured,
        "parse Qiniu download base URL",
    )?;
    if raw_uri.path().split('/').any(|segment| {
        match percent_encoding::percent_decode_str(segment).decode_utf8() {
            Ok(decoded) => matches!(decoded.as_ref(), "." | ".."),
            Err(_) => true,
        }
    }) {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Qiniu download base URL path must not contain traversal segments",
        ));
    }
    let url = Url::parse(value).map_storage_err_ctx(
        StorageErrorKind::Misconfigured,
        "parse Qiniu download base URL",
    )?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Qiniu download base URL must be an HTTPS URL with a hostname and no credentials, non-default port, query, or fragment",
        ));
    }
    Ok(())
}

/// Qiniu's official `urlEncodeQuery` applies query escaping, restores `/` and
/// `|`, and replaces `+` with `%20`. This byte-level equivalent preserves
/// unreserved ASCII plus those separators and escapes every UTF-8 byte once.
/// https://github.com/qiniu/go-sdk/blob/e1fe1ce0ae2555a82b3b2143e7e69d57efea640c/storage/bucket.go
fn encode_qiniu_key(key: &str) -> String {
    let mut encoded = String::with_capacity(key.len());
    for byte in key.bytes() {
        if QINIU_KEY_SAFE_BYTES.contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}

/// Build a Kodo source-domain or CDN delivery URL. The S3 space name is never
/// part of this URL; the bucket remains bound to the configured domain.
///
/// A source-domain URL is `<domain>/<encoded key>`. Private Kodo authentication
/// signs the complete URL through `e`, including response override parameters,
/// then appends `token` last.
pub(super) fn build_download_url(
    download_base_url: &str,
    base_path: &str,
    path: &str,
    auth: QiniuDownloadAuthMode,
    credentials: &QiniuStaticCredentials,
    expires: Duration,
    options: &aster_drive_storage::DirectDownloadOptions,
) -> aster_drive_storage::Result<String> {
    let key = object_key::join_key_prefix(base_path, path);
    if key.split('/').any(|segment| matches!(segment, "." | "..")) {
        return Err(storage_driver_error(
            StorageErrorKind::Precondition,
            "Qiniu download object key must not contain traversal segments",
        ));
    }

    let base = download_base_url.trim_end_matches('/');
    if base.is_empty() {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Qiniu download base URL cannot be empty",
        ));
    }
    let mut url = format!("{base}/{}", encode_qiniu_key(&key));

    let mut query = Vec::<(&str, &str)>::new();
    if auth == QiniuDownloadAuthMode::Private {
        if let Some(value) = options.response_cache_control.as_deref() {
            query.push(("response-cache-control", value));
        }
        if let Some(value) = options.response_content_type.as_deref() {
            query.push(("response-content-type", value));
        }
        if let Some(value) = options.response_content_disposition.as_deref() {
            query.push(("response-content-disposition", value));
        }
    }
    append_query(&mut url, &query);

    match auth {
        QiniuDownloadAuthMode::Private => {
            let deadline = deadline_secs(expires)?;
            let separator = if url.contains('?') { '&' } else { '?' };
            url.push(separator);
            url.push_str("e=");
            url.push_str(&deadline.to_string());
            let token = qiniu_download_token(&credentials.secret_key, url.as_bytes());
            url.push_str("&token=");
            url.push_str(&credentials.access_key);
            url.push(':');
            url.push_str(&token);
        }
        QiniuDownloadAuthMode::Timestamp => {
            let cdn_key = credentials
                .download_cdn_timestamp_key
                .as_deref()
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "Qiniu CDN timestamp authentication requires a timestamp key",
                    )
                })?;
            let deadline = deadline_secs(expires)?;
            let escaped_path = escaped_url_path(&url)?;
            let mut hasher = Md5::new();
            hasher.update(format!("{cdn_key}{escaped_path}{deadline:x}").as_bytes());
            let sign = hex::encode(hasher.finalize());
            let separator = if url.contains('?') { '&' } else { '?' };
            url.push(separator);
            url.push_str("sign=");
            url.push_str(&sign);
            url.push_str("&t=");
            url.push_str(&format!("{deadline:x}"));
        }
        QiniuDownloadAuthMode::Public => {}
    }

    Ok(url)
}

fn append_query(url: &mut String, query: &[(&str, &str)]) {
    if query.is_empty() {
        return;
    }
    for (index, (key, value)) in query.iter().enumerate() {
        url.push(if index == 0 { '?' } else { '&' });
        url.push_str(key);
        url.push('=');
        url.push_str(&encode_query_component(value));
    }
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}

fn escaped_url_path(url: &str) -> aster_drive_storage::Result<String> {
    let parsed = Url::parse(url).map_storage_err_ctx(
        StorageErrorKind::Misconfigured,
        "parse final Qiniu download URL",
    )?;
    Ok(parsed.path().to_string())
}

fn deadline_secs(expires: Duration) -> aster_drive_storage::Result<u64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_storage_err(StorageErrorKind::Misconfigured)?
        .as_secs();
    now.checked_add(expires.as_secs()).ok_or_else(|| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Qiniu download expiration overflow",
        )
    })
}

fn qiniu_download_token(secret_key: &str, data: &[u8]) -> String {
    let mut mac =
        HmacSha1::new_from_slice(secret_key.as_bytes()).expect("HMAC can accept keys of any size");
    mac.update(data);
    base64::engine::general_purpose::URL_SAFE.encode(mac.finalize().into_bytes())
}

impl QiniuDriver {
    pub(super) fn custom_download_url(
        &self,
        path: &str,
        expires: Duration,
        options: &aster_drive_storage::DirectDownloadOptions,
    ) -> aster_drive_storage::Result<Option<String>> {
        let Some(base_url) = self.download_base_url.as_deref() else {
            return Ok(None);
        };
        build_download_url(
            base_url,
            &self.base_path,
            path,
            self.download_auth,
            &self.download_credentials,
            expires,
            options,
        )
        .map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> QiniuStaticCredentials {
        QiniuStaticCredentials {
            access_key: "ak".to_string(),
            secret_key: "sk".to_string(),
            download_cdn_timestamp_key: Some("cdn-key".to_string()),
        }
    }

    #[test]
    fn qiniu_private_token_matches_the_official_sdk_vector() {
        // qiniu/go-sdk `Credentials.Sign([]byte("hello"))` for ak/sk.
        // The SDK's public vector, expressed as the raw HMAC digest so secret
        // scanners do not rewrite the Base64 URL-safe assertion.
        let digest =
            hex::decode("34337c70cd2bc28b3184727a400708ee26a5af48").expect("HMAC-SHA1 digest");
        assert_eq!(
            qiniu_download_token("sk", b"hello"),
            base64::engine::general_purpose::URL_SAFE.encode(digest)
        );
    }

    #[test]
    fn source_domain_urls_encode_once_and_do_not_add_the_bucket_name() {
        let url = build_download_url(
            "https://files.example.com/assets",
            "tenant-a",
            "目录/报告 +#%.txt",
            QiniuDownloadAuthMode::Private,
            &credentials(),
            Duration::from_secs(300),
            &Default::default(),
        )
        .expect("source-domain URL");
        let url = Url::parse(&url).unwrap();

        assert_eq!(url.host_str(), Some("files.example.com"));
        assert_eq!(
            url.path(),
            "/assets/tenant-a/%E7%9B%AE%E5%BD%95/%E6%8A%A5%E5%91%8A%20%2B%23%25.txt"
        );
        assert!(!url.path().contains("archive"));
        assert_eq!(url.query_pairs().filter(|(key, _)| key == "e").count(), 1);
        assert_eq!(
            url.query_pairs().last().map(|(key, _)| key.into_owned()),
            Some("token".to_string())
        );
    }

    #[test]
    fn key_encoding_preserves_separators_and_avoids_double_encoding() {
        assert_eq!(encode_qiniu_key("a/b|c+d e%f"), "a/b|c%2Bd%20e%25f");
    }

    #[test]
    fn private_token_signs_response_parameters() {
        let options = aster_drive_storage::DirectDownloadOptions {
            response_cache_control: Some("private, max-age=0".to_string()),
            response_content_disposition: Some("attachment; filename=\"报告.txt\"".to_string()),
            ..Default::default()
        };
        let url = build_download_url(
            "https://files.example.com",
            "",
            "reports/2026.txt",
            QiniuDownloadAuthMode::Private,
            &credentials(),
            Duration::from_secs(300),
            &options,
        )
        .unwrap();
        assert!(url.contains("?response-cache-control=private%2C%20max-age%3D0&"));
        assert!(url.contains("response-content-disposition=attachment%3B%20filename%3D%22"));

        let (to_sign, token) = url.rsplit_once("&token=").expect("token is last");
        assert!(to_sign.contains("&e="));
        assert_eq!(
            token,
            format!("ak:{}", qiniu_download_token("sk", to_sign.as_bytes()))
        );
    }

    #[test]
    fn timestamp_mode_uses_md5_of_escaped_path_and_hex_deadline() {
        let url = build_download_url(
            "https://cdn.example.com",
            "",
            "图片/a b.txt",
            QiniuDownloadAuthMode::Timestamp,
            &credentials(),
            Duration::from_secs(300),
            &Default::default(),
        )
        .unwrap();
        let parsed = Url::parse(&url).unwrap();
        let sign = parsed
            .query_pairs()
            .find_map(|(key, value)| (key == "sign").then(|| value.into_owned()))
            .expect("timestamp signature");
        let deadline = parsed
            .query_pairs()
            .find_map(|(key, value)| (key == "t").then(|| value.into_owned()))
            .expect("timestamp deadline");

        let mut hasher = Md5::new();
        hasher.update(format!("cdn-key{}{deadline}", parsed.path()).as_bytes());
        assert_eq!(sign, hex::encode(hasher.finalize()));
    }

    #[test]
    fn public_mode_is_explicit_and_does_not_append_a_token() {
        let options = aster_drive_storage::DirectDownloadOptions {
            response_content_type: Some("application/octet-stream".to_string()),
            response_content_disposition: Some("attachment; filename=\"public.txt\"".to_string()),
            ..Default::default()
        };
        let url = build_download_url(
            "https://files.example.com",
            "",
            "public.txt",
            QiniuDownloadAuthMode::Public,
            &credentials(),
            Duration::from_secs(300),
            &options,
        )
        .unwrap();
        let url = Url::parse(&url).unwrap();
        assert!(!url.query_pairs().any(|(key, _)| key == "token"));
        assert!(!url.query_pairs().any(|(key, _)| key == "e"));
        assert!(
            !url.query_pairs()
                .any(|(key, _)| key.starts_with("response-"))
        );
    }

    #[test]
    fn timestamp_mode_omits_unsupported_response_overrides() {
        let options = aster_drive_storage::DirectDownloadOptions {
            response_cache_control: Some("private, max-age=0".to_string()),
            response_content_type: Some("application/octet-stream".to_string()),
            response_content_disposition: Some("attachment; filename=\"file.txt\"".to_string()),
            ..Default::default()
        };
        let url = build_download_url(
            "https://cdn.example.com",
            "",
            "file.txt",
            QiniuDownloadAuthMode::Timestamp,
            &credentials(),
            Duration::from_secs(300),
            &options,
        )
        .unwrap();
        let url = Url::parse(&url).unwrap();
        assert!(
            !url.query_pairs()
                .any(|(key, _)| key.starts_with("response-"))
        );
    }

    #[test]
    fn rejects_unsafe_delivery_url_shapes() {
        for value in [
            "http://cdn.example.com",
            "https://user@cdn.example.com",
            "https://cdn.example.com:8443",
            "https://cdn.example.com/assets?token=x",
            "https://cdn.example.com/assets#fragment",
            "https://cdn.example.com/%2e%2e/private",
        ] {
            assert!(
                validate_download_base_url(value).is_err(),
                "unsafe URL should be rejected: {value}"
            );
        }
    }
}
