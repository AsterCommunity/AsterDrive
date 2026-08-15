//! 存储驱动实现：`s3_config`。

use crate::errors::AsterError;
use http::Uri;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedS3Config {
    pub endpoint: String,
    pub bucket: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum S3ConfigError {
    MissingBucket,
    InvalidEndpoint(String),
}

/// A SigV4 signing region that does not satisfy the shared S3-compatible format contract.
///
/// Provider-facing validation messages deliberately remain at the caller boundary so that
/// reusing this validator does not couple provider error mappings to [`S3ConfigError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSigV4Region;

impl S3ConfigError {
    pub fn into_aster_error(self) -> AsterError {
        match self {
            Self::MissingBucket => {
                AsterError::validation_error("bucket is required for S3-compatible storage")
            }
            Self::InvalidEndpoint(message) => AsterError::validation_error(message),
        }
    }
}

pub fn validate_sigv4_region(region: &str) -> std::result::Result<(), InvalidSigV4Region> {
    if region.is_empty()
        || region.len() > 128
        || !region.is_ascii()
        || region
            .bytes()
            .any(|byte| !(b'!'..=b'~').contains(&byte) || byte == b'/')
    {
        return Err(InvalidSigV4Region);
    }
    Ok(())
}

pub fn normalize_s3_endpoint_and_bucket(
    endpoint: &str,
    bucket: &str,
) -> std::result::Result<NormalizedS3Config, S3ConfigError> {
    let endpoint = endpoint.trim();
    let bucket = bucket.trim().to_string();

    if endpoint.is_empty() {
        if bucket.is_empty() {
            return Err(S3ConfigError::MissingBucket);
        }

        return Ok(NormalizedS3Config {
            endpoint: String::new(),
            bucket,
        });
    }

    let uri: Uri = endpoint.parse().map_err(|_| {
        S3ConfigError::InvalidEndpoint(format!("invalid S3 endpoint URL: '{endpoint}'"))
    })?;

    let scheme = uri.scheme_str().ok_or_else(|| {
        S3ConfigError::InvalidEndpoint(format!(
            "S3 endpoint must include http:// or https://: '{endpoint}'"
        ))
    })?;
    if scheme != "http" && scheme != "https" {
        return Err(S3ConfigError::InvalidEndpoint(format!(
            "S3 endpoint must use http:// or https://: '{endpoint}'"
        )));
    }

    uri.authority().ok_or_else(|| {
        S3ConfigError::InvalidEndpoint(format!("S3 endpoint must include a hostname: '{endpoint}'"))
    })?;

    if bucket.is_empty() {
        return Err(S3ConfigError::MissingBucket);
    }

    Ok(NormalizedS3Config {
        endpoint: endpoint.to_string(),
        bucket,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        InvalidSigV4Region, S3ConfigError, normalize_s3_endpoint_and_bucket, validate_sigv4_region,
    };

    #[test]
    fn allows_standard_s3_endpoint_without_rewriting() {
        let normalized =
            normalize_s3_endpoint_and_bucket("https://s3.example.com/custom/path", "archive")
                .expect("normalized S3 config");

        assert_eq!(normalized.endpoint, "https://s3.example.com/custom/path");
        assert_eq!(normalized.bucket, "archive");
    }

    #[test]
    fn rejects_missing_bucket_for_any_s3_compatible_endpoint() {
        assert_eq!(
            normalize_s3_endpoint_and_bucket("https://s3.example.com", "")
                .expect_err("missing bucket should fail"),
            S3ConfigError::MissingBucket
        );
    }

    #[test]
    fn accepts_valid_sigv4_regions_at_format_boundaries() {
        for region in [
            "a",
            "auto",
            "us-east-1",
            "account-id.r2.cloudflarestorage.com:443~custom",
            &"r".repeat(128),
        ] {
            assert_eq!(validate_sigv4_region(region), Ok(()), "region: {region}");
        }
    }

    #[test]
    fn rejects_invalid_sigv4_regions() {
        for region in [
            "".to_string(),
            "r".repeat(129),
            " region".to_string(),
            "region ".to_string(),
            "region with spaces".to_string(),
            "region\twith-tab".to_string(),
            "region\nwith-newline".to_string(),
            "region/name".to_string(),
            "region\u{7f}".to_string(),
            "中国".to_string(),
        ] {
            assert_eq!(
                validate_sigv4_region(&region),
                Err(InvalidSigV4Region),
                "region: {region:?}"
            );
        }
    }
}
