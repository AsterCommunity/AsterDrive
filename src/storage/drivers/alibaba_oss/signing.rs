use std::collections::BTreeMap;
use std::time::{Duration, UNIX_EPOCH};

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
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::percent_decode_str;
use sha2::{Digest, Sha256};
use url::Url;

type HmacSha256 = Hmac<Sha256>;

const OSS_AUTH_SCHEME_ID: AuthSchemeId = aws_runtime::auth::sigv4::SCHEME_ID;
const OSS_SIGN_ALGORITHM: &str = "OSS4-HMAC-SHA256";
const OSS_PRODUCT: &str = "oss";
const OSS_TERMINATOR: &str = "aliyun_v4_request";
const OSS_UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";
pub(super) const OSS_PRESIGNED_PUT_CONTENT_TYPE: &str = "application/octet-stream";
const DEFAULT_OSS_AUTH_TTL: Duration = Duration::from_secs(60 * 60);

const OSS_HEADER_RENAMES: &[(&str, &str)] = &[
    ("x-amz-copy-source", "x-oss-copy-source"),
    ("x-amz-copy-source-range", "x-oss-copy-source-range"),
    ("x-amz-copy-source-if-match", "x-oss-copy-source-if-match"),
    (
        "x-amz-copy-source-if-none-match",
        "x-oss-copy-source-if-none-match",
    ),
    (
        "x-amz-copy-source-if-modified-since",
        "x-oss-copy-source-if-modified-since",
    ),
    (
        "x-amz-copy-source-if-unmodified-since",
        "x-oss-copy-source-if-unmodified-since",
    ),
    ("x-amz-metadata-directive", "x-oss-metadata-directive"),
    ("x-amz-tagging-directive", "x-oss-tagging-directive"),
    ("x-amz-storage-class", "x-oss-storage-class"),
    ("x-amz-acl", "x-oss-object-acl"),
    (
        "x-amz-server-side-encryption",
        "x-oss-server-side-encryption",
    ),
];

pub(super) fn configure_oss_auth(
    builder: aws_sdk_s3::config::Builder,
    bucket: String,
    region: String,
    use_cname: bool,
) -> aws_sdk_s3::config::Builder {
    builder
        .request_checksum_calculation(aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired)
        .response_checksum_validation(aws_sdk_s3::config::ResponseChecksumValidation::WhenRequired)
        .push_auth_scheme(OssAuthScheme::new(bucket, region, use_cname))
}

#[derive(Debug)]
struct OssAuthScheme {
    signer: OssSigner,
}

impl OssAuthScheme {
    fn new(bucket: String, region: String, use_cname: bool) -> Self {
        Self {
            signer: OssSigner {
                bucket,
                region,
                use_cname,
            },
        }
    }
}

impl AuthScheme for OssAuthScheme {
    fn scheme_id(&self) -> AuthSchemeId {
        OSS_AUTH_SCHEME_ID
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
struct OssSigner {
    bucket: String,
    region: String,
    use_cname: bool,
}

impl Sign for OssSigner {
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
            .ok_or("OSS signer requires AWS credential identity")?;
        let key = normalize_aws_request_for_oss(request, &self.bucket, self.use_cname)?;

        let operation_config = config_bag.load::<SigV4OperationSigningConfig>();
        let signature_type = operation_config
            .map(|config| config.signing_options.signature_type)
            .unwrap_or(HttpSignatureType::HttpRequestHeaders);
        let expires = operation_config
            .and_then(|config| config.signing_options.expires_in)
            .unwrap_or(DEFAULT_OSS_AUTH_TTL);
        let now = runtime_components.time_source().unwrap_or_default().now();
        let now = now.duration_since(UNIX_EPOCH)?;
        let now =
            DateTime::<Utc>::from_timestamp(i64::try_from(now.as_secs())?, now.subsec_nanos())
                .ok_or("OSS signing time is outside chrono range")?;

        match signature_type {
            HttpSignatureType::HttpRequestQueryParams => {
                self.sign_query(request, credentials, &key, now, expires)?
            }
            HttpSignatureType::HttpRequestHeaders => {
                self.sign_headers(request, credentials, &key, now)?
            }
        }

        Ok(())
    }
}

impl OssSigner {
    fn sign_headers(
        &self,
        request: &mut HttpRequest,
        credentials: &Credentials,
        key: &str,
        now: DateTime<Utc>,
    ) -> std::result::Result<(), BoxError> {
        let datetime = now.format("%Y%m%dT%H%M%SZ").to_string();
        request.headers_mut().insert("x-oss-date", datetime.clone());
        request
            .headers_mut()
            .insert("x-oss-content-sha256", OSS_UNSIGNED_PAYLOAD.to_string());
        if let Some(token) = credentials.session_token() {
            request
                .headers_mut()
                .insert("x-oss-security-token", token.to_string());
        }

        let date = now.format("%Y%m%d").to_string();
        let scope = credential_scope(&date, &self.region);
        let url = Url::parse(request.uri())?;
        let canonical_request = canonical_request(
            request.method(),
            &self.bucket,
            key,
            url.query().unwrap_or_default(),
            &collect_signed_headers(request),
        );
        let string_to_sign = string_to_sign(&datetime, &scope, &canonical_request);
        let signature = signature(
            credentials.secret_access_key(),
            &date,
            &self.region,
            &string_to_sign,
        )?;
        request.headers_mut().insert(
            "authorization",
            format!(
                "{OSS_SIGN_ALGORITHM} Credential={}/{scope},Signature={signature}",
                credentials.access_key_id()
            ),
        );
        Ok(())
    }

    fn sign_query(
        &self,
        request: &mut HttpRequest,
        credentials: &Credentials,
        key: &str,
        now: DateTime<Utc>,
        expires: Duration,
    ) -> std::result::Result<(), BoxError> {
        // OSS V4 always signs Content-Type when the request carries it. Browser
        // uploads use this fixed value, so it must be present while presigning
        // both PutObject and UploadPart requests.
        if request.method() == "PUT" && request.headers().get("content-type").is_none() {
            request
                .headers_mut()
                .insert("content-type", OSS_PRESIGNED_PUT_CONTENT_TYPE.to_string());
        }

        let datetime = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date = now.format("%Y%m%d").to_string();
        let scope = credential_scope(&date, &self.region);
        let mut url = Url::parse(request.uri())?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(token) = credentials.session_token() {
                query.append_pair("x-oss-security-token", token);
            }
            query.append_pair("x-oss-signature-version", OSS_SIGN_ALGORITHM);
            query.append_pair("x-oss-date", &datetime);
            query.append_pair("x-oss-expires", &expires.as_secs().to_string());
            query.append_pair(
                "x-oss-credential",
                &format!("{}/{scope}", credentials.access_key_id()),
            );
        }
        normalize_query_encoding(&mut url);

        let canonical_request = canonical_request(
            request.method(),
            &self.bucket,
            key,
            url.query().unwrap_or_default(),
            &collect_signed_headers(request),
        );
        let string_to_sign = string_to_sign(&datetime, &scope, &canonical_request);
        let signature = signature(
            credentials.secret_access_key(),
            &date,
            &self.region,
            &string_to_sign,
        )?;
        let mut query = url.query().unwrap_or_default().to_string();
        if !query.is_empty() {
            query.push('&');
        }
        query.push_str("x-oss-signature=");
        query.push_str(&signature);
        url.set_query(Some(&query));
        request.set_uri(url.as_str())?;
        Ok(())
    }
}

fn normalize_aws_request_for_oss(
    request: &mut HttpRequest,
    bucket: &str,
    use_cname: bool,
) -> std::result::Result<String, BoxError> {
    let metadata_headers = request
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            name.strip_prefix("x-amz-meta-").map(|suffix| {
                (
                    name.to_string(),
                    format!("x-oss-meta-{suffix}"),
                    value.to_string(),
                )
            })
        })
        .collect::<Vec<_>>();
    for (aws_name, oss_name, value) in metadata_headers {
        request.headers_mut().remove(&aws_name);
        request.headers_mut().insert(oss_name, value);
    }

    let mut key = None;
    crate::storage::drivers::s3_vendor::normalize_aws_s3_vendor_request(
        request,
        OSS_HEADER_RENAMES,
        |url| {
            let decoded_path = percent_decode_str(url.path()).decode_utf8()?;
            let decoded_path = decoded_path.trim_start_matches('/');
            let object_key = if use_cname {
                let prefix = format!("{bucket}/");
                decoded_path
                    .strip_prefix(&prefix)
                    .ok_or("OSS CNAME request path is missing the bucket prefix")?
                    .to_string()
            } else {
                decoded_path.to_string()
            };
            if use_cname {
                url.set_path(&format!("/{object_key}"));
            }
            normalize_query_encoding(url);
            key = Some(object_key);
            Ok(())
        },
    )?;

    let aws_headers = request
        .headers()
        .iter()
        .filter(|(name, _)| name.starts_with("x-amz-") || name.starts_with("amz-sdk-"))
        .map(|(name, _)| name.to_string())
        .collect::<Vec<_>>();
    for name in aws_headers {
        request.headers_mut().remove(&name);
    }

    if let Some(copy_source) = request.headers_mut().remove("x-oss-copy-source") {
        let copy_source = if copy_source.starts_with('/') {
            copy_source
        } else {
            format!("/{copy_source}")
        };
        request
            .headers_mut()
            .insert("x-oss-copy-source", copy_source);
    }

    key.ok_or_else(|| "OSS request path normalization did not produce an object key".into())
}

fn collect_signed_headers(request: &HttpRequest) -> BTreeMap<String, Vec<String>> {
    let mut headers = BTreeMap::<String, Vec<String>>::new();
    for (name, value) in request.headers() {
        let name = name.to_ascii_lowercase();
        if is_default_signed_header(&name) {
            headers
                .entry(name)
                .or_default()
                .push(value.trim().to_string());
        }
    }
    headers
}

fn is_default_signed_header(name: &str) -> bool {
    name.starts_with("x-oss-") || name == "content-type" || name == "content-md5"
}

fn canonical_request(
    method: &str,
    bucket: &str,
    key: &str,
    raw_query: &str,
    headers: &BTreeMap<String, Vec<String>>,
) -> String {
    let canonical_headers = headers
        .iter()
        .map(|(name, values)| format!("{name}:{}\n", values.join(",")))
        .collect::<String>();
    let payload_hash = headers
        .get("x-oss-content-sha256")
        .and_then(|values| values.first())
        .map(String::as_str)
        .unwrap_or(OSS_UNSIGNED_PAYLOAD);
    format!(
        "{method}\n{}\n{}\n{canonical_headers}\n\n{payload_hash}",
        canonical_uri(bucket, key),
        canonical_query(raw_query),
    )
}

fn canonical_uri(bucket: &str, key: &str) -> String {
    oss_percent_encode(&format!("/{bucket}/{key}"), false)
}

fn canonical_query(raw_query: &str) -> String {
    let mut values = BTreeMap::<String, String>::new();
    let mut names = Vec::<String>::new();
    for (name, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        let name = oss_percent_encode(&name, true);
        let value = oss_percent_encode(&value, true);
        values.insert(name.clone(), value);
        names.push(name);
    }
    names.sort();
    names
        .into_iter()
        .map(|name| match values.get(&name) {
            Some(value) if !value.is_empty() => format!("{name}={value}"),
            _ => name,
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn normalize_query_encoding(url: &mut Url) {
    let query = canonical_query(url.query().unwrap_or_default());
    url.set_query((!query.is_empty()).then_some(query.as_str()));
}

fn oss_percent_encode(value: &str, encode_slash: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || (!encode_slash && byte == b'/')
        {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}

fn credential_scope(date: &str, region: &str) -> String {
    format!("{date}/{region}/{OSS_PRODUCT}/{OSS_TERMINATOR}")
}

fn string_to_sign(datetime: &str, scope: &str, canonical_request: &str) -> String {
    format!(
        "{OSS_SIGN_ALGORITHM}\n{datetime}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    )
}

fn signature(
    secret_key: &str,
    date: &str,
    region: &str,
    string_to_sign: &str,
) -> std::result::Result<String, BoxError> {
    let date_key = hmac_sha256(format!("aliyun_v4{secret_key}").as_bytes(), date.as_bytes())?;
    let region_key = hmac_sha256(&date_key, region.as_bytes())?;
    let product_key = hmac_sha256(&region_key, OSS_PRODUCT.as_bytes())?;
    let signing_key = hmac_sha256(&product_key, OSS_TERMINATOR.as_bytes())?;
    Ok(hex::encode(hmac_sha256(
        &signing_key,
        string_to_sign.as_bytes(),
    )?))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> std::result::Result<Vec<u8>, BoxError> {
    let mut mac = HmacSha256::new_from_slice(key)?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
    use aws_sdk_s3::presigning::PresigningConfig;
    use aws_smithy_http_client::test_util::{CaptureRequestReceiver, capture_request};
    use aws_smithy_types::body::SdkBody;

    fn oss_sdk_client(
        endpoint: &str,
        bucket: &str,
        use_cname: bool,
        response: Option<http::Response<SdkBody>>,
    ) -> (aws_sdk_s3::Client, CaptureRequestReceiver) {
        let (http_client, receiver) = capture_request(response);
        let builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .http_client(http_client)
            .credentials_provider(Credentials::new("ak", "sk", None, None, "oss-auth-test"))
            .region(Region::new("cn-hangzhou"))
            .endpoint_url(endpoint)
            .force_path_style(use_cname);
        let config = configure_oss_auth(
            builder,
            bucket.to_string(),
            "cn-hangzhou".to_string(),
            use_cname,
        )
        .build();
        (aws_sdk_s3::Client::from_conf(config), receiver)
    }

    fn empty_success_response() -> http::Response<SdkBody> {
        http::Response::builder()
            .status(200)
            .body(SdkBody::empty())
            .expect("mock OSS response")
    }

    fn empty_list_response() -> http::Response<SdkBody> {
        http::Response::builder()
            .status(200)
            .header("content-type", "application/xml")
            .body(SdkBody::from(
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <ListBucketResult>
                    <IsTruncated>false</IsTruncated>
                </ListBucketResult>"#,
            ))
            .expect("mock OSS list response")
    }

    #[test]
    fn header_signature_matches_official_go_sdk_vector() {
        let headers = BTreeMap::from([
            ("content-type".to_string(), vec!["text/plain".to_string()]),
            (
                "x-oss-content-sha256".to_string(),
                vec![OSS_UNSIGNED_PAYLOAD.to_string()],
            ),
            (
                "x-oss-date".to_string(),
                vec!["20231216T162057Z".to_string()],
            ),
            ("x-oss-head1".to_string(), vec!["value".to_string()]),
        ]);
        let canonical = canonical_request(
            "PUT",
            "bucket",
            "1234+-/123/1.txt",
            "%2Bparam1=value3&%2Bparam2=&%7Cparam1=value4&%7Cparam2=&param1=value1&param2=",
            &headers,
        );
        let scope = credential_scope("20231216", "cn-hangzhou");
        let string_to_sign = string_to_sign("20231216T162057Z", &scope, &canonical);
        let signature =
            signature("sk", "20231216", "cn-hangzhou", &string_to_sign).expect("OSS signature");

        assert_eq!(
            signature,
            "e21d18daa82167720f9b1047ae7e7f1ce7cb77a31e8203a7d5f4624fa0284afe"
        );
    }

    #[test]
    fn canonical_uri_includes_bucket_and_encodes_key_once() {
        assert_eq!(
            canonical_uri("bucket", "目录/a b+%2F.txt"),
            "/bucket/%E7%9B%AE%E5%BD%95/a%20b%2B%252F.txt"
        );
    }

    #[test]
    fn canonical_query_uses_oss_v4_encoding_for_response_overrides() {
        assert_eq!(
            canonical_query(
                "response-cache-control=private%2C+max-age%3D0&response-content-disposition=inline%3B+filename*%3DUTF-8%27%27photo.jpeg"
            ),
            "response-cache-control=private%2C%20max-age%3D0&response-content-disposition=inline%3B%20filename%2A%3DUTF-8%27%27photo.jpeg"
        );
    }

    #[tokio::test]
    async fn aws_sdk_normal_request_uses_oss_v4_headers() {
        let (client, receiver) = oss_sdk_client(
            "https://oss-cn-hangzhou.aliyuncs.com",
            "bucket",
            false,
            Some(empty_success_response()),
        );

        client
            .get_object()
            .bucket("bucket")
            .key("docs/report.txt")
            .range("bytes=0-9")
            .send()
            .await
            .expect("mock OSS GET should deserialize");

        let request = receiver.expect_request();
        let request_url = Url::parse(request.uri()).expect("captured OSS URL");
        assert_eq!(
            request_url.host_str(),
            Some("bucket.oss-cn-hangzhou.aliyuncs.com")
        );
        let authorization = request
            .headers()
            .get("authorization")
            .expect("OSS Authorization header");
        assert!(authorization.starts_with("OSS4-HMAC-SHA256 Credential=ak/"));
        assert!(authorization.contains("/cn-hangzhou/oss/aliyun_v4_request,Signature="));
        assert_eq!(
            request.headers().get("x-oss-content-sha256"),
            Some(OSS_UNSIGNED_PAYLOAD)
        );
        assert!(request.headers().get("x-oss-date").is_some());
        assert!(!request.uri().to_string().contains("x-id="));
        let aws_protocol_headers = request
            .headers()
            .iter()
            // The SDK adds its telemetry header after signing. It is not part
            // of the OSS canonical request; AWS protocol/signature headers
            // must already have been removed by the OSS signer.
            .filter(|(name, _)| name.starts_with("x-amz-") && *name != "x-amz-user-agent")
            .collect::<Vec<_>>();
        assert!(
            aws_protocol_headers.is_empty(),
            "unexpected AWS protocol headers: {aws_protocol_headers:?}"
        );
    }

    #[tokio::test]
    async fn aws_sdk_presigned_request_uses_oss_v4_query() {
        let (client, _receiver) = oss_sdk_client(
            "https://oss-cn-hangzhou.aliyuncs.com",
            "bucket",
            false,
            None,
        );
        let presigned = client
            .put_object()
            .bucket("bucket")
            .key("docs/report 1.txt")
            .presigned(
                PresigningConfig::builder()
                    .expires_in(Duration::from_secs(599))
                    .build()
                    .expect("presign config"),
            )
            .await
            .expect("OSS presigned PUT");
        let url = Url::parse(presigned.uri()).expect("presigned URL");

        assert_eq!(url.host_str(), Some("bucket.oss-cn-hangzhou.aliyuncs.com"));
        assert_eq!(url.path(), "/docs/report%201.txt");
        assert_eq!(
            url.query_pairs()
                .find(|(name, _)| name == "x-oss-signature-version")
                .map(|(_, value)| value.into_owned()),
            Some(OSS_SIGN_ALGORITHM.to_string())
        );
        assert_eq!(
            url.query_pairs()
                .find(|(name, _)| name == "x-oss-expires")
                .map(|(_, value)| value.into_owned()),
            Some("599".to_string())
        );
        assert!(url.query_pairs().any(|(name, _)| name == "x-oss-signature"));
        assert!(
            !url.query_pairs()
                .any(|(name, _)| name.starts_with("X-Amz-"))
        );
        assert_eq!(
            presigned
                .headers()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                .map(|(_, value)| value),
            Some(OSS_PRESIGNED_PUT_CONTENT_TYPE)
        );
    }

    #[tokio::test]
    async fn aws_sdk_presigned_get_normalizes_response_override_query_encoding() {
        let (client, _receiver) = oss_sdk_client(
            "https://oss-cn-hangzhou.aliyuncs.com",
            "bucket",
            false,
            None,
        );
        let presigned = client
            .get_object()
            .bucket("bucket")
            .key("files/photo.jpeg")
            .response_cache_control("private, max-age=0, must-revalidate")
            .response_content_disposition("inline; filename*=UTF-8''photo.jpeg")
            .response_content_type("image/jpeg")
            .presigned(
                PresigningConfig::builder()
                    .expires_in(Duration::from_secs(300))
                    .build()
                    .expect("presign config"),
            )
            .await
            .expect("OSS presigned GET");
        let url = Url::parse(presigned.uri()).expect("presigned URL");
        let raw_query = url.query().expect("presigned GET query");

        assert!(raw_query.contains("private%2C%20max-age%3D0%2C%20must-revalidate"));
        assert!(raw_query.contains("filename%2A%3DUTF-8%27%27photo.jpeg"));
        assert!(!raw_query.contains('+'));
        assert!(!raw_query.contains("filename*"));
        assert!(url.query_pairs().any(|(name, _)| name == "x-oss-signature"));
    }

    #[tokio::test]
    async fn aws_sdk_presigned_upload_part_keeps_multipart_query() {
        let (client, receiver) = oss_sdk_client(
            "https://oss-cn-hangzhou.aliyuncs.com",
            "bucket",
            false,
            None,
        );
        let presigned = client
            .upload_part()
            .bucket("bucket")
            .key("video.bin")
            .upload_id("upload-id")
            .part_number(7)
            .presigned(
                PresigningConfig::builder()
                    .expires_in(Duration::from_secs(300))
                    .build()
                    .expect("presign config"),
            )
            .await
            .expect("OSS presigned upload part");

        receiver.expect_no_request();
        let url = Url::parse(presigned.uri()).expect("OSS presigned part URL");
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(query.get("partNumber").map(AsRef::as_ref), Some("7"));
        assert_eq!(query.get("uploadId").map(AsRef::as_ref), Some("upload-id"));
        assert_eq!(
            query.get("x-oss-signature-version").map(AsRef::as_ref),
            Some(OSS_SIGN_ALGORITHM)
        );
        assert!(query.contains_key("x-oss-signature"));
        assert!(!query.keys().any(|name| name.starts_with("X-Amz-")));
        assert_eq!(
            presigned
                .headers()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                .map(|(_, value)| value),
            Some(OSS_PRESIGNED_PUT_CONTENT_TYPE)
        );
    }

    #[tokio::test]
    async fn cname_request_removes_bucket_from_wire_path_but_signs_it() {
        let (client, receiver) = oss_sdk_client(
            "https://files.example.test",
            "bucket",
            true,
            Some(empty_success_response()),
        );

        client
            .head_object()
            .bucket("bucket")
            .key("docs/report.txt")
            .send()
            .await
            .expect("mock OSS CNAME HEAD should deserialize");

        let request = receiver.expect_request();
        let request_url = Url::parse(request.uri()).expect("captured OSS CNAME URL");
        assert_eq!(request_url.path(), "/docs/report.txt");
        assert_eq!(request_url.host_str(), Some("files.example.test"));
        assert!(
            request
                .headers()
                .get("authorization")
                .is_some_and(|value| value.starts_with(OSS_SIGN_ALGORITHM))
        );
    }

    #[tokio::test]
    async fn cname_bucket_root_request_uses_root_wire_path() {
        let (client, receiver) = oss_sdk_client(
            "https://files.example.test",
            "bucket",
            true,
            Some(empty_list_response()),
        );

        client
            .list_objects_v2()
            .bucket("bucket")
            .send()
            .await
            .expect("mock OSS CNAME list should deserialize");

        let request = receiver.expect_request();
        let request_url = Url::parse(request.uri()).expect("captured OSS CNAME list URL");
        assert_eq!(request_url.path(), "/");
        assert_eq!(request_url.host_str(), Some("files.example.test"));
        assert_eq!(
            request_url
                .query_pairs()
                .find(|(name, _)| name == "list-type")
                .map(|(_, value)| value.into_owned()),
            Some("2".to_string())
        );
        assert!(
            request
                .headers()
                .get("authorization")
                .is_some_and(|value| value.starts_with(OSS_SIGN_ALGORITHM))
        );
    }

    #[tokio::test]
    async fn copy_object_translates_copy_source_header() {
        let (client, receiver) = oss_sdk_client(
            "https://oss-cn-hangzhou.aliyuncs.com",
            "bucket",
            false,
            Some(empty_success_response()),
        );

        client
            .copy_object()
            .bucket("bucket")
            .key("dest.txt")
            .copy_source("bucket/source.txt")
            .send()
            .await
            .expect("mock OSS copy should deserialize");

        let request = receiver.expect_request();
        assert_eq!(
            request.headers().get("x-oss-copy-source"),
            Some("/bucket/source.txt")
        );
        assert!(request.headers().get("x-amz-copy-source").is_none());
    }
}
