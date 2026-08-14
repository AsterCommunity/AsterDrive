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
    InvalidRegion,
}

impl S3ConfigError {
    pub fn into_aster_error(self) -> AsterError {
        match self {
            Self::MissingBucket => {
                AsterError::validation_error("bucket is required for S3-compatible storage")
            }
            Self::InvalidEndpoint(message) => AsterError::validation_error(message),
            Self::InvalidRegion => AsterError::validation_error(
                "s3_region must be 1-128 printable ASCII characters without whitespace or '/'",
            ),
        }
    }
}

pub fn validate_s3_region(region: &str) -> std::result::Result<(), S3ConfigError> {
    if region.is_empty()
        || region.len() > 128
        || !region.is_ascii()
        || region
            .bytes()
            .any(|byte| !(b'!'..=b'~').contains(&byte) || byte == b'/')
    {
        return Err(S3ConfigError::InvalidRegion);
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
    use super::{S3ConfigError, normalize_s3_endpoint_and_bucket, validate_s3_region};

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
    fn rejects_invalid_sigv4_regions() {
        for region in ["", "region with spaces", "region/name", "\u{4e2d}\u{56fd}"] {
            assert_eq!(
                validate_s3_region(region),
                Err(S3ConfigError::InvalidRegion)
            );
        }
        assert!(validate_s3_region("cn-east-1").is_ok());
    }
}
