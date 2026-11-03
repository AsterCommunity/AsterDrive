use std::time::Duration;

use aster_drive_storage::DirectDownloadOptions;
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::object_key;
use chrono::{DateTime, Utc};
use md5::{Digest, Md5};
use sha2::Sha256;
use url::Url;

const CDN_AUTH_TYPE_A: &str = "type_a";
const CDN_AUTH_TYPE_B: &str = "type_b";
const CDN_AUTH_TYPE_C1: &str = "type_c1";
const CDN_AUTH_TYPE_C2: &str = "type_c2";
const CDN_AUTH_PUBLIC: &str = "public";

pub(super) fn signed_download_url(
    base_url: &Url,
    base_path: &str,
    path: &str,
    expires: Duration,
    auth_type: &str,
    sign_arg: &str,
    time_arg: &str,
    signing_algorithm: &str,
    primary_key: Option<&str>,
    backup_key: Option<&str>,
    _options: &DirectDownloadOptions,
) -> aster_drive_storage::Result<String> {
    let now = Utc::now();
    let rand = random_token();
    build_signed_download_url(
        base_url,
        base_path,
        path,
        now,
        &rand,
        expires,
        auth_type,
        sign_arg,
        time_arg,
        signing_algorithm,
        primary_key,
        backup_key,
        _options,
    )
}

fn build_signed_download_url(
    base_url: &Url,
    base_path: &str,
    path: &str,
    timestamp: DateTime<Utc>,
    rand: &str,
    expires: Duration,
    auth_type: &str,
    sign_arg: &str,
    time_arg: &str,
    signing_algorithm: &str,
    primary_key: Option<&str>,
    backup_key: Option<&str>,
    _options: &DirectDownloadOptions,
) -> aster_drive_storage::Result<String> {
    validate_download_base_url(base_url)?;

    let key = object_key::join_key_prefix(base_path, path);
    let delivery_path = if key.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", key.trim_start_matches('/'))
    };
    if key.split('/').any(|segment| matches!(segment, "." | "..")) {
        return Err(storage_driver_error(
            StorageErrorKind::Precondition,
            "Huawei CDN download object key must not contain traversal segments",
        ));
    }

    let expiry_secs = expires
        .as_secs()
        .checked_add(u64::from(expires.subsec_nanos() > 0))
        .ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Huawei CDN download expiration overflow",
            )
        })?;
    let current_secs = timestamp.timestamp().max(0) as u64;
    let deadline = current_secs.checked_add(expiry_secs).ok_or_else(|| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Huawei CDN download expiration overflow",
        )
    })?;
    let trimmed = auth_type.trim();
    match trimmed {
        CDN_AUTH_PUBLIC => Ok(public_url(base_url, &delivery_path)),
        CDN_AUTH_TYPE_A => {
            let key = resolve_key(primary_key, backup_key)?;
            let timestamp = deadline.to_string();
            let signature = digest(
                signing_algorithm,
                &[&delivery_path, "-", &timestamp, "-", rand, "-0-", key],
            )?;
            let sign_arg = normalize_arg_name(sign_arg, "sign")?;
            let time_arg = normalize_arg_name(time_arg, "timestamp")?;
            let query =
                format!("{sign_arg}={timestamp}-{rand}-0-{signature}&{time_arg}={timestamp}");
            Ok(with_query(base_url, &delivery_path, &query))
        }
        CDN_AUTH_TYPE_B => {
            let key = resolve_key(primary_key, backup_key)?;
            let timestamp = timestamp
                .with_timezone(&chrono_tz::Asia::Shanghai)
                .format("%Y%m%d%H%M")
                .to_string();
            let signature = digest(signing_algorithm, &[key, &timestamp, &delivery_path])?;
            Ok(path_prefix_url(
                base_url,
                &format!("/{timestamp}/{signature}{delivery_path}"),
            ))
        }
        CDN_AUTH_TYPE_C1 => {
            let key = resolve_key(primary_key, backup_key)?;
            let timestamp = format!("{deadline:x}");
            let signature = digest(signing_algorithm, &[key, &delivery_path, &timestamp])?;
            Ok(path_prefix_url(
                base_url,
                &format!("/{signature}/{timestamp}{delivery_path}"),
            ))
        }
        CDN_AUTH_TYPE_C2 => {
            let key = resolve_key(primary_key, backup_key)?;
            let timestamp = deadline.to_string();
            let signature = digest(signing_algorithm, &[key, &delivery_path, &timestamp])?;
            let sign_arg = normalize_arg_name(sign_arg, "sign")?;
            let time_arg = normalize_arg_name(time_arg, "timestamp")?;
            let query = format!("{sign_arg}={signature}&{time_arg}={timestamp}");
            Ok(with_query(base_url, &delivery_path, &query))
        }
        _ => Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Huawei CDN auth type must be type_a, type_b, type_c1, type_c2, or public",
        )),
    }
}

fn validate_download_base_url(base_url: &Url) -> aster_drive_storage::Result<()> {
    if base_url.scheme() != "https"
        || base_url.host_str().is_none()
        || !base_url.username().is_empty()
        || base_url.password().is_some()
        || base_url.port().is_some()
        || base_url.query().is_some()
        || base_url.fragment().is_some()
    {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Huawei CDN download URL must be an HTTPS URL with a hostname and no credentials, non-default port, query, or fragment",
        ));
    }
    if base_url.path().split('/').any(|segment| {
        match percent_encoding::percent_decode_str(segment).decode_utf8() {
            Ok(decoded) => matches!(decoded.as_ref(), "." | ".."),
            Err(_) => true,
        }
    }) {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Huawei CDN download URL path must not contain traversal segments",
        ));
    }
    Ok(())
}

fn resolve_key<'a>(
    primary_key: Option<&'a str>,
    backup_key: Option<&'a str>,
) -> aster_drive_storage::Result<&'a str> {
    primary_key
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .or_else(|| backup_key.map(str::trim).filter(|key| !key.is_empty()))
        .ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Huawei CDN authenticated downloads require a primary or backup authentication key",
            )
        })
}

fn normalize_arg_name(value: &str, fallback: &str) -> aster_drive_storage::Result<String> {
    let value = value.trim();
    let value = if value.is_empty() { fallback } else { value };
    if !value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Huawei CDN authentication parameter names must start with a letter and contain only letters, digits, and underscores",
        ));
    }
    Ok(value.to_string())
}

fn digest(algorithm: &str, parts: &[&str]) -> aster_drive_storage::Result<String> {
    match algorithm.trim() {
        "md5" => {
            let mut hasher = Md5::new();
            for part in parts {
                hasher.update(part.as_bytes());
            }
            Ok(hex::encode(hasher.finalize()))
        }
        "sha256" => {
            use sha2::Digest as _;
            let mut hasher = Sha256::new();
            for part in parts {
                hasher.update(part.as_bytes());
            }
            Ok(hex::encode(hasher.finalize()))
        }
        _ => Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Huawei CDN signing algorithm must be md5 or sha256",
        )),
    }
}

fn public_url(base_url: &Url, delivery_path: &str) -> String {
    format!(
        "{}{}",
        base_url.as_str().trim_end_matches('/'),
        delivery_path
    )
}

fn with_query(base_url: &Url, delivery_path: &str, query: &str) -> String {
    format!(
        "{}{}?{}",
        base_url.as_str().trim_end_matches('/'),
        delivery_path,
        query
    )
}

fn path_prefix_url(base_url: &Url, prefixed_path: &str) -> String {
    let base = base_url.as_str().trim_end_matches('/');
    format!("{base}{prefixed_path}")
}

fn random_token() -> String {
    use rand::RngExt;
    const SYMBOLS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut rng = rand::rng();
    (0..16)
        .map(|_| {
            let index = rng.random_range(0..SYMBOLS.len());
            char::from(SYMBOLS[index])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::build_signed_download_url;
    use aster_drive_storage::DirectDownloadOptions;
    use chrono::TimeZone;
    use url::Url;

    fn base_url() -> Url {
        Url::parse("https://cdn.example.test/assets").expect("valid CDN base URL")
    }

    fn timestamp() -> chrono::DateTime<chrono::Utc> {
        chrono_tz::Asia::Shanghai
            .with_ymd_and_hms(2017, 6, 30, 8, 0, 0)
            .single()
            .expect("fixed Asia/Shanghai datetime")
            .with_timezone(&chrono::Utc)
    }

    fn official_key() -> &'static str {
        "Test123"
    }

    fn fixed_timestamp(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .timestamp_opt(secs, 0)
            .single()
            .expect("fixed UTC timestamp")
    }

    #[test]
    fn type_a_matches_official_vector() {
        let path = "/T128_2_1_0_sdk/0210/M00/82/3E/test.mp3";
        let url = build_signed_download_url(
            &Url::parse("https://cdn.example.test").expect("root base"),
            "",
            path.trim_start_matches('/'),
            fixed_timestamp(1498755600),
            "0",
            std::time::Duration::from_secs(0),
            "type_a",
            "auth_key",
            "timestamp",
            "md5",
            Some(official_key()),
            None,
            &DirectDownloadOptions::default(),
        )
        .expect("signed URL");

        assert_eq!(
            url,
            "https://cdn.example.test/T128_2_1_0_sdk/0210/M00/82/3E/test.mp3?auth_key=1498755600-0-0-b2a5d7ee62ebbe6555eac50a7eac9caa&timestamp=1498755600"
        );
    }

    #[test]
    fn type_c1_matches_official_vector() {
        let path = "/T128_2_1_0_sdk/0210/M00/82/3E/test.mp3";
        let url = build_signed_download_url(
            &Url::parse("https://cdn.example.test").expect("root base"),
            "",
            path.trim_start_matches('/'),
            fixed_timestamp(1498755600),
            "0",
            std::time::Duration::from_secs(32_400),
            "type_c1",
            "auth_key",
            "timestamp",
            "md5",
            Some(official_key()),
            None,
            &DirectDownloadOptions::default(),
        )
        .expect("signed URL");

        assert_eq!(
            url,
            "https://cdn.example.test/e6470b0cc62edfa6b7f79331af9c6db2/5955b0a0/T128_2_1_0_sdk/0210/M00/82/3E/test.mp3"
        );
    }

    #[test]
    fn type_b_uses_key_timestamp_path_order() {
        let url = build_signed_download_url(
            &Url::parse("https://cdn.example.test").expect("root base"),
            "",
            "nested/file.txt",
            timestamp(),
            "0",
            std::time::Duration::from_secs(3600),
            "type_b",
            "auth_key",
            "timestamp",
            "md5",
            Some("TimeKey"),
            None,
            &DirectDownloadOptions::default(),
        )
        .expect("signed URL");

        assert_eq!(
            url,
            "https://cdn.example.test/201706300800/79098c4d41f57a301ed7c980420a0f5b/nested/file.txt"
        );
    }

    #[test]
    fn public_requires_no_key() {
        let url = build_signed_download_url(
            &base_url(),
            "",
            "docs/file.txt",
            timestamp(),
            "0",
            std::time::Duration::from_secs(3600),
            "public",
            "sign",
            "timestamp",
            "md5",
            None,
            None,
            &DirectDownloadOptions::default(),
        )
        .expect("public URL");
        assert_eq!(url, "https://cdn.example.test/assets/docs/file.txt");
    }

    #[test]
    fn authenticated_requires_a_key() {
        let error = build_signed_download_url(
            &base_url(),
            "",
            "docs/file.txt",
            timestamp(),
            "0",
            std::time::Duration::from_secs(3600),
            "type_a",
            "sign",
            "timestamp",
            "md5",
            None,
            None,
            &DirectDownloadOptions::default(),
        )
        .expect_err("authenticated URL must fail without a key");
        assert!(error.message().contains("authentication key"));
    }

    #[test]
    fn rejects_traversal_segments() {
        let error = build_signed_download_url(
            &base_url(),
            "",
            "../outside",
            timestamp(),
            "0",
            std::time::Duration::from_secs(3600),
            "public",
            "sign",
            "timestamp",
            "md5",
            None,
            None,
            &DirectDownloadOptions::default(),
        )
        .expect_err("traversal must fail");
        assert!(error.message().contains("traversal"));
    }

    #[test]
    fn validates_parameter_names() {
        let error = build_signed_download_url(
            &base_url(),
            "",
            "docs/file.txt",
            timestamp(),
            "0",
            std::time::Duration::from_secs(3600),
            "type_c2",
            "1bad",
            "timestamp",
            "md5",
            Some("TimeKey"),
            None,
            &DirectDownloadOptions::default(),
        )
        .expect_err("non-alphabetic leading name must fail");
        assert!(error.message().contains("parameter names"));
    }
}
