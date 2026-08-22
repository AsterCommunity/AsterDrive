use aws_credential_types::Credentials;
use aws_runtime::auth::{HttpSignatureType, SigV4OperationSigningConfig};
use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::auth::{
    AuthScheme, AuthSchemeEndpointConfig, AuthSchemeId, Sign,
};
use aws_smithy_runtime_api::client::identity::{Identity, SharedIdentityResolver};
use aws_smithy_runtime_api::client::orchestrator::HttpRequest;
use aws_smithy_runtime_api::client::runtime_components::{GetIdentityResolver, RuntimeComponents};
use aws_smithy_types::config_bag::ConfigBag;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use std::collections::BTreeMap;
use std::time::{Duration, UNIX_EPOCH};
use url::Url;

use super::HuaweiObsAddressingMode;

type HmacSha1 = Hmac<Sha1>;

// Native signing and request normalization are pinned against Huawei's official
// Go OBS SDK v3.26.6 at fd2b44881f0cd9bd41ffff2fabeb94c783ccc321:
// - obs/auth.go + obs/authV2.go: SignatureObs header/query authentication
// - obs/conf.go: virtual-hosted and CNAME canonical-resource construction
// - obs/trait_object.go + obs/trait_part.go + obs/convert.go: request headers,
//   multipart parameters, and CompleteMultipartUpload XML
// https://github.com/huaweicloud/huaweicloud-sdk-go-obs/tree/fd2b44881f0cd9bd41ffff2fabeb94c783ccc321/obs

const OBS_AUTH_SCHEME_ID: AuthSchemeId = aws_runtime::auth::sigv4::SCHEME_ID;
const DEFAULT_OBS_AUTH_TTL: Duration = Duration::from_secs(60 * 60);
pub(super) const OBS_PRESIGNED_PUT_CONTENT_TYPE: &str = "application/octet-stream";

const OBS_HEADER_RENAMES: &[(&str, &str)] = &[
    ("x-amz-copy-source", "x-obs-copy-source"),
    ("x-amz-copy-source-range", "x-obs-copy-source-range"),
    ("x-amz-metadata-directive", "x-obs-metadata-directive"),
    ("x-amz-tagging-directive", "x-obs-tagging-directive"),
    ("x-amz-tagging", "x-obs-tagging"),
    ("x-amz-storage-class", "x-obs-storage-class"),
    ("x-amz-acl", "x-obs-acl"),
    ("x-amz-grant-read", "x-obs-grant-read"),
    ("x-amz-grant-write", "x-obs-grant-write"),
    ("x-amz-grant-read-acp", "x-obs-grant-read-acp"),
    ("x-amz-grant-write-acp", "x-obs-grant-write-acp"),
    ("x-amz-grant-full-control", "x-obs-grant-full-control"),
    ("x-amz-security-token", "x-obs-security-token"),
];

const OBS_CANONICAL_RESOURCE_PARAMS: &[&str] = &[
    "acl",
    "append",
    "attname",
    "backtosource",
    "bucketstatus",
    "cdnnotifyconfiguration",
    "cors",
    "customdomain",
    "delete",
    "deletebucket",
    "directcoldaccess",
    "dispolicy",
    "encryption",
    "inventory",
    "length",
    "lifecycle",
    "location",
    "logging",
    "metadata",
    "mirrorbacktosource",
    "modify",
    "name",
    "notification",
    "object-lock",
    "obscompresspolicy",
    "partnumber",
    "policy",
    "policystatus",
    "position",
    "publicaccessblock",
    "quota",
    "rename",
    "replication",
    "requestpayment",
    "response-cache-control",
    "response-content-disposition",
    "response-content-encoding",
    "response-content-language",
    "response-content-type",
    "response-expires",
    "restore",
    "retention",
    "storageclass",
    "storageinfo",
    "storagepolicy",
    "tagging",
    "torrent",
    "truncate",
    "uploadid",
    "uploads",
    "versionid",
    "versioning",
    "versions",
    "website",
    "x-image-process",
    "x-obs-accesslabel",
    "x-obs-security-token",
];

pub(super) fn configure_obs_auth(
    builder: aws_sdk_s3::config::Builder,
    bucket: String,
    addressing_mode: HuaweiObsAddressingMode,
) -> aws_sdk_s3::config::Builder {
    builder
        .request_checksum_calculation(aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired)
        .response_checksum_validation(aws_sdk_s3::config::ResponseChecksumValidation::WhenRequired)
        .push_auth_scheme(ObsAuthScheme {
            signer: ObsSigner {
                bucket,
                addressing_mode,
            },
        })
}

#[derive(Debug)]
struct ObsAuthScheme {
    signer: ObsSigner,
}

impl AuthScheme for ObsAuthScheme {
    fn scheme_id(&self) -> AuthSchemeId {
        OBS_AUTH_SCHEME_ID
    }

    fn identity_resolver(
        &self,
        identity_resolvers: &dyn GetIdentityResolver,
    ) -> Option<SharedIdentityResolver> {
        identity_resolvers.identity_resolver(self.scheme_id())
    }

    fn signer(&self) -> &dyn Sign {
        &self.signer
    }
}

#[derive(Debug)]
struct ObsSigner {
    bucket: String,
    addressing_mode: HuaweiObsAddressingMode,
}

impl Sign for ObsSigner {
    fn sign_http_request(
        &self,
        request: &mut HttpRequest,
        identity: &Identity,
        _auth_scheme_endpoint_config: AuthSchemeEndpointConfig<'_>,
        runtime_components: &RuntimeComponents,
        config_bag: &ConfigBag,
    ) -> std::result::Result<(), BoxError> {
        let credentials = identity
            .data::<Credentials>()
            .ok_or("Huawei OBS signer requires AWS credential identity")?;
        normalize_aws_request_for_obs(request, &self.bucket, self.addressing_mode)?;

        let operation_config = config_bag.load::<SigV4OperationSigningConfig>();
        let signature_type = operation_config
            .map(|config| config.signing_options.signature_type)
            .unwrap_or(HttpSignatureType::HttpRequestHeaders);
        let expires = operation_config
            .and_then(|config| config.signing_options.expires_in)
            .unwrap_or(DEFAULT_OBS_AUTH_TTL);
        let now = runtime_components.time_source().unwrap_or_default().now();
        let mut url = Url::parse(request.uri())?;

        match signature_type {
            HttpSignatureType::HttpRequestQueryParams => {
                // OBS includes Content-Type in the V2 StringToSign. The browser
                // upload adapter forwards this exact header from the returned
                // request descriptor, so it must be present before signing.
                if request.method() == "PUT" && request.headers().get("content-type").is_none() {
                    request
                        .headers_mut()
                        .insert("content-type", OBS_PRESIGNED_PUT_CONTENT_TYPE.to_string());
                }
                if let Some(token) = credentials.session_token() {
                    url.query_pairs_mut()
                        .append_pair("x-obs-security-token", token);
                }
                let expires_at = now
                    .duration_since(UNIX_EPOCH)?
                    .as_secs()
                    .checked_add(expires.as_secs())
                    .ok_or("Huawei OBS presign expiration overflow")?;
                let canonical_resource =
                    canonical_resource(&url, &self.bucket, self.addressing_mode);
                let string_to_sign = obs_string_to_sign(
                    request.method(),
                    request,
                    &canonical_resource,
                    &expires_at.to_string(),
                );
                let signature = obs_signature(credentials.secret_access_key(), &string_to_sign)?;
                url.query_pairs_mut()
                    .append_pair("AccessKeyId", credentials.access_key_id())
                    .append_pair("Expires", &expires_at.to_string())
                    .append_pair("Signature", &signature);
                request.set_uri(url.as_str())?;
            }
            HttpSignatureType::HttpRequestHeaders => {
                if let Some(token) = credentials.session_token() {
                    request
                        .headers_mut()
                        .insert("x-obs-security-token", token.to_string());
                }
                let date = format_obs_date(now);
                request.headers_mut().insert("date", date.clone());
                let canonical_resource =
                    canonical_resource(&url, &self.bucket, self.addressing_mode);
                let string_to_sign =
                    obs_string_to_sign(request.method(), request, &canonical_resource, &date);
                let signature = obs_signature(credentials.secret_access_key(), &string_to_sign)?;
                request.headers_mut().insert(
                    "authorization",
                    format!("OBS {}:{signature}", credentials.access_key_id()),
                );
            }
        }

        Ok(())
    }
}

fn normalize_aws_request_for_obs(
    request: &mut HttpRequest,
    bucket: &str,
    addressing_mode: HuaweiObsAddressingMode,
) -> std::result::Result<(), BoxError> {
    crate::storage::drivers::s3_vendor::normalize_aws_s3_vendor_request(
        request,
        OBS_HEADER_RENAMES,
        |url| {
            if addressing_mode == HuaweiObsAddressingMode::CustomDomain {
                let host = url
                    .host_str()
                    .ok_or("Huawei OBS custom-domain request URL is missing a host")?
                    .to_string();
                let prefixed = format!("{bucket}.");
                let custom_host = host.strip_prefix(&prefixed).ok_or(
                    "Huawei OBS custom-domain request did not contain the SDK bucket host prefix",
                )?;
                url.set_host(Some(custom_host))
                    .map_err(|_| "failed to rewrite Huawei OBS custom-domain request host")?;
            }
            Ok(())
        },
    )?;
    // The AWS serializer emits user metadata as x-amz-meta-*; OBS signs and
    // stores the same values under x-obs-meta-* (the official SDK's
    // `HEADER_PREFIX_META_OBS` path in trait_object.go).
    let metadata_headers = request
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            let name = name.to_ascii_lowercase();
            let obs_name = format!("x-obs-meta-{}", name.strip_prefix("x-amz-meta-")?);
            Some((name, obs_name, value.to_string()))
        })
        .collect::<Vec<_>>();
    for (aws_name, obs_name, value) in metadata_headers {
        request.headers_mut().remove(aws_name);
        request.headers_mut().append(obs_name, value);
    }
    request.headers_mut().remove("x-amz-content-sha256");
    request.headers_mut().remove("x-amz-date");
    Ok(())
}

fn canonical_resource(url: &Url, bucket: &str, addressing_mode: HuaweiObsAddressingMode) -> String {
    let resource_bucket = match addressing_mode {
        HuaweiObsAddressingMode::VirtualHosted => bucket,
        HuaweiObsAddressingMode::CustomDomain => url.host_str().unwrap_or(bucket),
    };
    let mut resource = format!("/{resource_bucket}{}", url.path());
    let mut params = url
        .query_pairs()
        .filter(|(key, _)| is_canonical_resource_param(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    params.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    if !params.is_empty() {
        resource.push('?');
        resource.push_str(
            &params
                .into_iter()
                .map(|(key, value)| {
                    if value.is_empty() {
                        key
                    } else {
                        format!("{key}={value}")
                    }
                })
                .collect::<Vec<_>>()
                .join("&"),
        );
    }
    resource
}

fn is_canonical_resource_param(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    OBS_CANONICAL_RESOURCE_PARAMS.contains(&name.as_str()) || name.starts_with("x-obs-")
}

fn obs_string_to_sign(
    method: &str,
    request: &HttpRequest,
    canonical_resource: &str,
    date_or_expires: &str,
) -> String {
    let content_md5 = request.headers().get("content-md5").unwrap_or_default();
    let content_type = request.headers().get("content-type").unwrap_or_default();
    let date = if request
        .headers()
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("x-obs-date"))
    {
        ""
    } else {
        date_or_expires
    };
    let canonical_headers = canonical_obs_headers(request);
    format!(
        "{method}\n{content_md5}\n{content_type}\n{date}\n{canonical_headers}{canonical_resource}"
    )
}

fn canonical_obs_headers(request: &HttpRequest) -> String {
    let mut headers = BTreeMap::<String, Vec<&str>>::new();
    for (name, value) in request.headers().iter() {
        let name = name.to_ascii_lowercase();
        if name.starts_with("x-obs-") {
            headers.entry(name).or_default().push(value);
        }
    }
    headers
        .into_iter()
        .map(|(name, values)| {
            let value = if name.starts_with("x-obs-meta-") {
                values
                    .into_iter()
                    .map(str::trim)
                    .collect::<Vec<_>>()
                    .join(",")
            } else {
                values.join(",")
            };
            format!("{name}:{value}\n")
        })
        .collect()
}

fn obs_signature(secret_key: &str, string_to_sign: &str) -> std::result::Result<String, BoxError> {
    let mut mac = HmacSha1::new_from_slice(secret_key.as_bytes())?;
    mac.update(string_to_sign.as_bytes());
    Ok(base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()))
}

fn format_obs_date(time: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time)
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::{Duration, UNIX_EPOCH};

    use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
    use aws_sdk_s3::presigning::PresigningConfig;
    use aws_sdk_s3::primitives::ByteStream;
    use aws_smithy_http_client::test_util::{CaptureRequestReceiver, capture_request};
    use aws_smithy_types::body::SdkBody;
    use url::Url;

    use super::{
        OBS_PRESIGNED_PUT_CONTENT_TYPE, canonical_obs_headers, configure_obs_auth,
        normalize_aws_request_for_obs, obs_signature, obs_string_to_sign,
    };
    use crate::storage::drivers::huawei_obs::HuaweiObsAddressingMode;

    fn obs_sdk_client(
        endpoint: &str,
        bucket: &str,
        addressing_mode: HuaweiObsAddressingMode,
        response: Option<http::Response<SdkBody>>,
    ) -> (aws_sdk_s3::Client, CaptureRequestReceiver) {
        let (http_client, receiver) = capture_request(response);
        let builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .http_client(http_client)
            .credentials_provider(Credentials::new(
                "AccessKeyExample",
                "SecretKeyExample",
                None,
                None,
                "obs-auth-test",
            ))
            .region(Region::new("cn-north-4"))
            .endpoint_url(endpoint)
            .force_path_style(false);
        let config = configure_obs_auth(builder, bucket.to_string(), addressing_mode).build();
        (aws_sdk_s3::Client::from_conf(config), receiver)
    }

    fn empty_success_response() -> http::Response<SdkBody> {
        http::Response::builder()
            .status(200)
            .body(SdkBody::empty())
            .expect("mock OBS response")
    }

    #[test]
    fn obs_signature_uses_hmac_sha1_base64() {
        // The StringToSign shape follows Huawei's API reference; the fixed
        // secret makes the HMAC result a deterministic local vector.
        // https://support.huaweicloud.com/intl/en-us/api-obs/obs_04_0010.html
        let mut request =
            aws_smithy_runtime_api::client::orchestrator::HttpRequest::new(SdkBody::empty());
        request.set_method("PUT").expect("request method");
        request
            .set_uri("https://example.test/newbucketname2")
            .expect("request URI");
        request.headers_mut().insert("x-obs-acl", "private");
        let string_to_sign = obs_string_to_sign(
            "PUT",
            &request,
            "/newbucketname2",
            "Sat, 12 Oct 2015 08:12:38 GMT",
        );
        assert_eq!(
            string_to_sign,
            "PUT\n\n\nSat, 12 Oct 2015 08:12:38 GMT\nx-obs-acl:private\n/newbucketname2"
        );
        assert_eq!(
            obs_signature("SecretKeyExample", &string_to_sign).expect("OBS signature"),
            "kwG73X0JKy2qftn+1NLHE6iqCsY="
        );
    }

    #[test]
    fn canonical_obs_headers_match_official_sdk_value_joining() {
        let mut request =
            aws_smithy_runtime_api::client::orchestrator::HttpRequest::new(SdkBody::empty());
        request
            .headers_mut()
            .insert("x-obs-meta-note", "alpha  beta");
        request.headers_mut().append("x-obs-meta-note", "gamma");
        request
            .headers_mut()
            .insert("x-obs-test-header", "one  two");

        assert_eq!(
            canonical_obs_headers(&request),
            "x-obs-meta-note:alpha  beta,gamma\nx-obs-test-header:one  two\n"
        );
    }

    #[test]
    fn metadata_header_translation_preserves_duplicate_values() {
        let mut request =
            aws_smithy_runtime_api::client::orchestrator::HttpRequest::new(SdkBody::empty());
        request
            .set_uri("https://archive-bucket.obs.cn-north-4.myhuaweicloud.com/object")
            .expect("request URI");
        request.headers_mut().append("x-amz-meta-note", "alpha");
        request.headers_mut().append("x-amz-meta-note", "beta");

        normalize_aws_request_for_obs(
            &mut request,
            "archive-bucket",
            HuaweiObsAddressingMode::VirtualHosted,
        )
        .expect("metadata headers should normalize");

        let values = request
            .headers()
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("x-obs-meta-note"))
            .map(|(_, value)| value)
            .collect::<Vec<_>>();
        assert_eq!(values, vec!["alpha", "beta"]);
        assert!(request.headers().get("x-amz-meta-note").is_none());
    }

    #[tokio::test]
    async fn aws_sdk_normal_request_uses_obs_authorization_and_range() {
        let (client, receiver) = obs_sdk_client(
            "https://obs.cn-north-4.myhuaweicloud.com",
            "archive-bucket",
            HuaweiObsAddressingMode::VirtualHosted,
            Some(empty_success_response()),
        );

        client
            .get_object()
            .bucket("archive-bucket")
            .key("docs/report.txt")
            .range("bytes=0-9")
            .send()
            .await
            .expect("mock OBS GET should deserialize");

        let request = receiver.expect_request();
        let request_url = Url::parse(request.uri()).expect("captured OBS URL");
        assert_eq!(
            request_url.host_str(),
            Some("archive-bucket.obs.cn-north-4.myhuaweicloud.com")
        );
        assert_eq!(request.headers().get("range"), Some("bytes=0-9"));
        assert!(
            request
                .headers()
                .get("authorization")
                .expect("OBS Authorization header")
                .starts_with("OBS AccessKeyExample:")
        );
        assert!(request.headers().get("date").is_some());
        assert!(!request.uri().to_string().contains("x-id="));
    }

    #[tokio::test]
    async fn aws_sdk_user_metadata_is_rewritten_to_obs_headers() {
        let (client, receiver) = obs_sdk_client(
            "https://obs.cn-north-4.myhuaweicloud.com",
            "archive-bucket",
            HuaweiObsAddressingMode::VirtualHosted,
            Some(empty_success_response()),
        );

        client
            .put_object()
            .bucket("archive-bucket")
            .key("docs/report.txt")
            .metadata("origin", "asterdrive")
            .body(ByteStream::from_static(b"report"))
            .send()
            .await
            .expect("mock OBS PUT should deserialize");

        let request = receiver.expect_request();
        assert_eq!(
            request.headers().get("x-obs-meta-origin"),
            Some("asterdrive")
        );
        assert!(request.headers().get("x-amz-meta-origin").is_none());
        assert!(
            request
                .headers()
                .get("authorization")
                .expect("OBS Authorization header")
                .starts_with("OBS AccessKeyExample:")
        );
    }

    #[tokio::test]
    async fn aws_sdk_list_objects_uses_obs_marker_contract() {
        let response = http::Response::builder()
            .status(200)
            .header("content-type", "application/xml")
            .body(SdkBody::from(
                r#"<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>"#,
            ))
            .expect("mock OBS list response");
        let (client, receiver) = obs_sdk_client(
            "https://obs.cn-north-4.myhuaweicloud.com",
            "archive-bucket",
            HuaweiObsAddressingMode::VirtualHosted,
            Some(response),
        );

        client
            .list_objects()
            .bucket("archive-bucket")
            .prefix("tenant-a/")
            .marker("tenant-a/previous.txt")
            .send()
            .await
            .expect("mock OBS ListObjects should deserialize");

        let request = receiver.expect_request();
        let request_url = Url::parse(request.uri()).expect("captured OBS list URL");
        let query = request_url
            .query_pairs()
            .into_owned()
            .collect::<HashMap<_, _>>();
        assert_eq!(query.get("prefix").map(String::as_str), Some("tenant-a/"));
        assert_eq!(
            query.get("marker").map(String::as_str),
            Some("tenant-a/previous.txt")
        );
        assert!(!query.contains_key("list-type"));
        assert!(!query.contains_key("continuation-token"));
    }

    #[tokio::test]
    async fn aws_sdk_copy_and_multipart_requests_use_obs_names_and_resources() {
        let (client, receiver) = obs_sdk_client(
            "https://obs.cn-north-4.myhuaweicloud.com",
            "archive-bucket",
            HuaweiObsAddressingMode::VirtualHosted,
            Some(empty_success_response()),
        );
        let _ = client
            .copy_object()
            .bucket("archive-bucket")
            .key("dest.txt")
            .copy_source("archive-bucket/source.txt")
            .send()
            .await;
        let request = receiver.expect_request();
        assert_eq!(
            request.headers().get("x-obs-copy-source").map(str::trim),
            Some("archive-bucket/source.txt")
        );
        assert!(request.headers().get("x-amz-copy-source").is_none());

        let (client, receiver) = obs_sdk_client(
            "https://obs.cn-north-4.myhuaweicloud.com",
            "archive-bucket",
            HuaweiObsAddressingMode::VirtualHosted,
            Some(empty_success_response()),
        );
        let _ = client
            .upload_part()
            .bucket("archive-bucket")
            .key("video.bin")
            .upload_id("upload-id")
            .part_number(7)
            .body(ByteStream::from_static(b"part-data"))
            .send()
            .await;
        let request = receiver.expect_request();
        let request_url = Url::parse(request.uri()).expect("captured OBS multipart URL");
        assert!(
            request_url
                .query()
                .unwrap_or_default()
                .contains("partNumber=7")
        );
        assert!(
            request_url
                .query()
                .unwrap_or_default()
                .contains("uploadId=upload-id")
        );
        assert!(request.headers().get("authorization").is_some());
        assert!(
            request
                .headers()
                .iter()
                .all(|(name, _)| !name.starts_with("x-amz-checksum-")
                    && name != "x-amz-sdk-checksum-algorithm")
        );
    }

    #[tokio::test]
    async fn aws_sdk_presigned_get_and_part_use_obs_v2_query_signature() {
        let (client, receiver) = obs_sdk_client(
            "https://obs.cn-north-4.myhuaweicloud.com",
            "archive-bucket",
            HuaweiObsAddressingMode::VirtualHosted,
            None,
        );
        let presigned = client
            .get_object()
            .bucket("archive-bucket")
            .key("docs/report.txt")
            .response_content_disposition("attachment")
            .presigned(
                PresigningConfig::builder()
                    .start_time(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
                    .expires_in(Duration::from_secs(600))
                    .build()
                    .expect("presign config"),
            )
            .await
            .expect("OBS presigned GET");
        receiver.expect_no_request();
        let query = Url::parse(presigned.uri())
            .expect("OBS presigned URL")
            .query_pairs()
            .into_owned()
            .collect::<HashMap<_, _>>();
        assert_eq!(
            query.get("AccessKeyId").map(String::as_str),
            Some("AccessKeyExample")
        );
        assert_eq!(query.get("Expires").map(String::as_str), Some("1700000600"));
        assert!(query.contains_key("Signature"));
        assert_eq!(
            query
                .get("response-content-disposition")
                .map(String::as_str),
            Some("attachment")
        );
        assert!(!query.keys().any(|key| key.starts_with("X-Amz-")));

        let (client, receiver) = obs_sdk_client(
            "https://obs.cn-north-4.myhuaweicloud.com",
            "archive-bucket",
            HuaweiObsAddressingMode::VirtualHosted,
            None,
        );
        let part = client
            .upload_part()
            .bucket("archive-bucket")
            .key("video.bin")
            .upload_id("upload-id")
            .part_number(7)
            .presigned(
                PresigningConfig::builder()
                    .start_time(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
                    .expires_in(Duration::from_secs(300))
                    .build()
                    .expect("presign config"),
            )
            .await
            .expect("OBS presigned part");
        receiver.expect_no_request();
        let query = Url::parse(part.uri())
            .expect("OBS presigned part URL")
            .query_pairs()
            .into_owned()
            .collect::<HashMap<_, _>>();
        assert_eq!(query.get("partNumber").map(String::as_str), Some("7"));
        assert_eq!(query.get("uploadId").map(String::as_str), Some("upload-id"));
        assert!(query.contains_key("Signature"));
    }

    #[tokio::test]
    async fn aws_sdk_presigned_put_signs_and_returns_content_type() {
        let (client, receiver) = obs_sdk_client(
            "https://obs.cn-north-4.myhuaweicloud.com",
            "archive-bucket",
            HuaweiObsAddressingMode::VirtualHosted,
            None,
        );
        let put = client
            .put_object()
            .bucket("archive-bucket")
            .key("docs/report.txt")
            .presigned(
                PresigningConfig::builder()
                    .start_time(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
                    .expires_in(Duration::from_secs(600))
                    .build()
                    .expect("presign config"),
            )
            .await
            .expect("OBS presigned PUT");
        receiver.expect_no_request();

        let query = Url::parse(put.uri())
            .expect("OBS presigned PUT URL")
            .query_pairs()
            .into_owned()
            .collect::<HashMap<_, _>>();
        assert_eq!(
            query.get("AccessKeyId").map(String::as_str),
            Some("AccessKeyExample")
        );
        assert_eq!(query.get("Expires").map(String::as_str), Some("1700000600"));
        assert!(query.contains_key("Signature"));
        assert_eq!(
            put.headers()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                .map(|(_, value)| value),
            Some(OBS_PRESIGNED_PUT_CONTENT_TYPE)
        );
    }

    #[tokio::test]
    async fn custom_domain_requests_remove_the_sdk_bucket_host_prefix() {
        let (client, receiver) = obs_sdk_client(
            "https://files.example.com",
            "archive-bucket",
            HuaweiObsAddressingMode::CustomDomain,
            Some(empty_success_response()),
        );
        client
            .head_object()
            .bucket("archive-bucket")
            .key("docs/report.txt")
            .send()
            .await
            .expect("mock custom-domain HEAD should deserialize");

        let request = receiver.expect_request();
        let request_url = Url::parse(request.uri()).expect("captured custom-domain OBS URL");
        assert_eq!(request_url.host_str(), Some("files.example.com"));
        assert_eq!(request_url.path(), "/docs/report.txt");
        assert!(
            request
                .headers()
                .get("authorization")
                .expect("OBS Authorization header")
                .starts_with("OBS AccessKeyExample:")
        );
    }
}
