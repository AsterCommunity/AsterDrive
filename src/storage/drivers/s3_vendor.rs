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
