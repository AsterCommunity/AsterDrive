//! Shared cleanup for provider auth schemes that reuse AWS S3 serialization.

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::orchestrator::HttpRequest;
use url::Url;

const AWS_SDK_CHECKSUM_HEADERS: &[&str] = &[
    "x-amz-sdk-checksum-algorithm",
    "x-amz-checksum-crc32",
    "x-amz-checksum-crc32c",
    "x-amz-checksum-sha1",
    "x-amz-checksum-sha256",
    "x-amz-checksum-crc64nvme",
];

/// Turn SDK path-style serialization into a request to a bucket-bound domain.
/// The endpoint host is already final; remove only the synthetic bucket path
/// before provider signing, preserving the encoded object key and query.
pub(super) fn normalize_cname_url(
    url: &mut Url,
    bucket: &str,
) -> std::result::Result<(), BoxError> {
    let bucket_path = format!("/{bucket}");
    let path = match url.path().strip_prefix(&bucket_path) {
        Some("") => "/".to_string(),
        Some(path) if path.starts_with('/') => path.to_string(),
        _ => return Err("CNAME request path is missing the SDK bucket prefix".into()),
    };
    url.set_path(&path);
    Ok(())
}

pub(super) fn normalize_aws_s3_vendor_request(
    request: &mut HttpRequest,
    header_renames: &'static [(&'static str, &'static str)],
    mutate_url: impl FnOnce(&mut Url) -> std::result::Result<(), BoxError>,
) -> std::result::Result<(), BoxError> {
    for (aws_name, provider_name) in header_renames {
        if let Some(value) = request.headers_mut().remove(aws_name) {
            request.headers_mut().insert(*provider_name, value);
        }
    }
    for header in AWS_SDK_CHECKSUM_HEADERS {
        request.headers_mut().remove(header);
    }

    let mut url = Url::parse(request.uri())?;
    let query = url
        .query_pairs()
        .filter(|(key, _)| !key.eq_ignore_ascii_case("x-id"))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    mutate_url(&mut url)?;
    request.set_uri(url.as_str())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::normalize_cname_url;
    use url::Url;

    #[test]
    fn cname_preserves_final_host_port_encoded_key_and_query() {
        let mut url = Url::parse(
            "https://archive-bucket.files.example.test:9443/archive-bucket/tenant/%E7%9B%AE%E5%BD%95/a%20b+%252F%23%3F.txt?uploadId=part%2B%2F%25&partNumber=7",
        )
        .unwrap();
        let query = url.query().unwrap().to_string();
        normalize_cname_url(&mut url, "archive-bucket").unwrap();
        assert_eq!(url.host_str(), Some("archive-bucket.files.example.test"));
        assert_eq!(url.port(), Some(9443));
        assert_eq!(
            url.path(),
            "/tenant/%E7%9B%AE%E5%BD%95/a%20b+%252F%23%3F.txt"
        );
        assert_eq!(url.query(), Some(query.as_str()));
    }

    #[test]
    fn cname_bucket_operations_use_the_domain_root() {
        for path in ["/archive-bucket", "/archive-bucket/"] {
            let mut url = Url::parse(&format!("https://files.example.test{path}?cors")).unwrap();
            normalize_cname_url(&mut url, "archive-bucket").unwrap();
            assert_eq!(url.path(), "/");
            assert_eq!(url.query(), Some("cors"));
        }
    }

    #[test]
    fn cname_removes_exactly_one_complete_bucket_segment() {
        let mut url =
            Url::parse("https://files.example.test/archive-bucket/archive-bucket/report.txt")
                .unwrap();
        normalize_cname_url(&mut url, "archive-bucket").unwrap();
        assert_eq!(url.path(), "/archive-bucket/report.txt");

        let mut url =
            Url::parse("https://files.example.test/archive-bucket-other/report.txt").unwrap();
        assert!(normalize_cname_url(&mut url, "archive-bucket").is_err());
    }
}
