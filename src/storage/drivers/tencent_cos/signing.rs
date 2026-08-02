use crate::errors::{AsterError, MapAsterErr, Result};
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
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, percent_encode};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha1::{Digest, Sha1};
use std::time::{Duration, UNIX_EPOCH};
use url::Url;

use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};

use super::TencentCosDriver;

type HmacSha1 = Hmac<Sha1>;

const COS_SIGN_ALGORITHM: &str = "sha1";
const COS_AUTH_SCHEME_ID: AuthSchemeId = aws_runtime::auth::sigv4::SCHEME_ID;
const DEFAULT_COS_AUTH_TTL: Duration = Duration::from_secs(60 * 60);

const COS_SIGNED_HEADERS: &[&str] = &[
    "host",
    "range",
    "x-cos-acl",
    "x-cos-grant-read",
    "x-cos-grant-write",
    "x-cos-grant-full-control",
    "cache-control",
    "content-disposition",
    "content-encoding",
    "content-type",
    "content-length",
    "content-md5",
    "transfer-encoding",
    "expect",
    "expires",
    "x-cos-content-sha1",
    "x-cos-storage-class",
    "if-match",
    "if-modified-since",
    "if-none-match",
    "if-unmodified-since",
    "origin",
    "access-control-request-method",
    "access-control-request-headers",
    "x-cos-object-type",
    "pic-operations",
];

const COS_HEADER_RENAMES: &[(&str, &str)] = &[
    ("x-amz-copy-source", "x-cos-copy-source"),
    ("x-amz-copy-source-range", "x-cos-copy-source-range"),
    ("x-amz-metadata-directive", "x-cos-metadata-directive"),
    ("x-amz-tagging-directive", "x-cos-tagging-directive"),
    ("x-amz-storage-class", "x-cos-storage-class"),
    ("x-amz-acl", "x-cos-acl"),
    ("x-amz-grant-read", "x-cos-grant-read"),
    ("x-amz-grant-write", "x-cos-grant-write"),
    ("x-amz-grant-full-control", "x-cos-grant-full-control"),
];

pub(super) fn configure_cos_auth(
    builder: aws_sdk_s3::config::Builder,
) -> aws_sdk_s3::config::Builder {
    builder
        .request_checksum_calculation(aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired)
        .response_checksum_validation(aws_sdk_s3::config::ResponseChecksumValidation::WhenRequired)
        .push_auth_scheme(CosAuthScheme::default())
}

#[derive(Debug, Default)]
struct CosAuthScheme {
    signer: CosSigner,
}

impl AuthScheme for CosAuthScheme {
    fn scheme_id(&self) -> AuthSchemeId {
        COS_AUTH_SCHEME_ID
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

#[derive(Debug, Default)]
struct CosSigner;

impl Sign for CosSigner {
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
            .ok_or("COS signer requires AWS credential identity")?;
        normalize_aws_request_for_cos(request)?;

        let operation_config = config_bag.load::<SigV4OperationSigningConfig>();
        let signature_type = operation_config
            .map(|config| config.signing_options.signature_type)
            .unwrap_or(HttpSignatureType::HttpRequestHeaders);
        let expires = operation_config
            .and_then(|config| config.signing_options.expires_in)
            .unwrap_or(DEFAULT_COS_AUTH_TTL);
        let now = runtime_components.time_source().unwrap_or_default().now();
        let start = now.duration_since(UNIX_EPOCH)?.as_secs();
        let end = start
            .checked_add(expires.as_secs())
            .ok_or("COS signing expiration overflow")?;
        let key_time = format!("{start};{end}");

        let mut url = Url::parse(request.uri())?;
        if signature_type == HttpSignatureType::HttpRequestQueryParams {
            if let Some(token) = credentials.session_token() {
                url.query_pairs_mut()
                    .append_pair("x-cos-security-token", token);
            }
        } else if let Some(token) = credentials.session_token() {
            request
                .headers_mut()
                .insert("x-cos-security-token", token.to_string());
        }

        let host = host_header_value(&url, "COS SDK request URL missing host")?;
        let signed_headers = collect_signed_headers(request, &host);
        let query = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        let header_refs = signed_headers
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let query_refs = query
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let canonical_path = canonical_url_path(&url)?;
        let authorization = cos_authorization(
            request.method(),
            &canonical_path,
            &query_refs,
            &header_refs,
            credentials.access_key_id(),
            credentials.secret_access_key(),
            &key_time,
        )?;

        if signature_type == HttpSignatureType::HttpRequestQueryParams {
            let mut query = url.query_pairs_mut();
            for component in authorization.split('&') {
                let (key, value) = component
                    .split_once('=')
                    .ok_or("invalid COS authorization component")?;
                query.append_pair(key, value);
            }
            drop(query);
            request.set_uri(url.as_str())?;
        } else {
            request.headers_mut().insert("authorization", authorization);
        }

        Ok(())
    }
}

// Tencent COS request-signature docs require UrlEncode for canonical query and
// header keys/values. Query/header keys are lowercased after encoding, while
// values keep their encoded case. The documented UrlEncode symbol table is:
// space ; ! < " = # > $ ? % @ & [ ' \ ( ] ) ^ * ` + { , | / } :
// Source: https://cloud.tencent.com/document/api/436/7778
const COS_QUERY_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'!')
    .add(b'$')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'|');

impl TencentCosDriver {
    pub(super) fn object_url(&self, path: &str) -> Result<(Url, String)> {
        let key = self.full_key(path);
        let mut url = Url::parse(&self.endpoint)
            .map_aster_err_ctx("parse COS endpoint", AsterError::storage_driver_error)?;
        let host = url.host_str().ok_or_else(|| {
            storage_driver_error(StorageErrorKind::Misconfigured, "COS endpoint missing host")
        })?;
        if !host.starts_with(&format!("{}.", self.bucket)) {
            let virtual_host = format!("{}.{}", self.bucket, host);
            url.set_host(Some(&virtual_host)).map_aster_err_ctx(
                "build COS virtual-hosted URL",
                AsterError::storage_driver_error,
            )?;
        }

        let endpoint_path = url.path().trim_matches('/');
        let object_path = if endpoint_path.is_empty() {
            key.clone()
        } else {
            format!("{endpoint_path}/{key}")
        };
        url.set_path(&format!("/{object_path}"));
        url.set_query(None);
        url.set_fragment(None);
        Ok((url, key))
    }

    pub(super) fn signed_cos_query_url(
        &self,
        path: &str,
        params: &[(&str, &str)],
        key_time: &str,
    ) -> Result<(Url, String)> {
        let (mut url, key) = self.object_url(path)?;
        let host = host_header_value(&url, "COS object URL missing host")?;
        let canonical_path = canonical_url_path(&url)?;
        let authorization = cos_authorization(
            "GET",
            &canonical_path,
            params,
            &[("host", host.as_str())],
            &self.access_key,
            &self.secret_key,
            key_time,
        );
        let authorization = authorization?;

        {
            let mut query = url.query_pairs_mut();
            for (key, value) in params {
                query.append_pair(key, value);
            }
            query.append_pair("sign", &authorization);
        }
        Ok((url, key))
    }

    pub(crate) fn bucket_cors_url(&self) -> Result<Url> {
        let mut url = Url::parse(&self.endpoint)
            .map_aster_err_ctx("parse COS endpoint", AsterError::storage_driver_error)?;
        let host = url.host_str().ok_or_else(|| {
            storage_driver_error(StorageErrorKind::Misconfigured, "COS endpoint missing host")
        })?;
        if !host.starts_with(&format!("{}.", self.bucket)) {
            let virtual_host = format!("{}.{}", self.bucket, host);
            url.set_host(Some(&virtual_host))
                .map_aster_err_ctx("build COS bucket URL", AsterError::storage_driver_error)?;
        }
        url.set_path("/");
        url.set_query(Some("cors"));
        url.set_fragment(None);
        Ok(url)
    }

    pub(crate) fn signed_cos_request_headers(
        &self,
        method: &str,
        url: &Url,
        headers: &[(&str, &str)],
        key_time: &str,
    ) -> Result<HeaderMap> {
        let host = host_header_value(url, "COS request URL missing host")?;
        let mut signed_headers = headers.to_vec();
        signed_headers.push(("host", host.as_str()));
        let params = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        let param_refs = params
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let canonical_path = canonical_url_path(url)?;
        let authorization = cos_authorization(
            method,
            &canonical_path,
            &param_refs,
            &signed_headers,
            &self.access_key,
            &self.secret_key,
            key_time,
        )?;

        let mut result = HeaderMap::new();
        for (key, value) in headers {
            let name = HeaderName::from_bytes(key.as_bytes()).map_aster_err_ctx(
                "build COS signed header name",
                AsterError::storage_driver_error,
            )?;
            let value = HeaderValue::from_str(value).map_aster_err_ctx(
                "build COS signed header value",
                AsterError::storage_driver_error,
            )?;
            result.insert(name, value);
        }
        result.insert(
            reqwest::header::AUTHORIZATION,
            HeaderValue::from_str(&authorization).map_aster_err_ctx(
                "build COS Authorization header",
                AsterError::storage_driver_error,
            )?,
        );
        Ok(result)
    }
}

fn normalize_aws_request_for_cos(request: &mut HttpRequest) -> std::result::Result<(), BoxError> {
    crate::storage::drivers::s3_vendor::normalize_aws_s3_vendor_request(
        request,
        COS_HEADER_RENAMES,
        |_| Ok(()),
    )
}

fn collect_signed_headers(request: &HttpRequest, host: &str) -> Vec<(String, String)> {
    let mut headers = request
        .headers()
        .iter()
        .filter(|(name, _)| is_cos_signed_header(name))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.to_string()))
        .collect::<Vec<_>>();
    if !headers.iter().any(|(name, _)| name == "host") {
        headers.push(("host".to_string(), host.to_string()));
    }
    headers
}

fn is_cos_signed_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    COS_SIGNED_HEADERS.contains(&name.as_str())
        || name.starts_with("x-cos-")
        || name.starts_with("x-ci-")
}

fn cos_authorization(
    method: &str,
    path: &str,
    params: &[(&str, &str)],
    headers: &[(&str, &str)],
    access_key: &str,
    secret_key: &str,
    key_time: &str,
) -> Result<String> {
    let header_list = canonical_header_list(headers);
    let http_headers = canonical_headers(headers);
    let url_param_list = canonical_param_list(params);
    let http_params = canonical_params(params);
    let http_string = format!(
        "{}\n{path}\n{http_params}\n{http_headers}\n",
        method.to_ascii_lowercase()
    );
    let string_to_sign = format!(
        "{COS_SIGN_ALGORITHM}\n{key_time}\n{}\n",
        sha1_hex(http_string.as_bytes())
    );
    let sign_key = hmac_sha1_hex(secret_key.as_bytes(), key_time.as_bytes())?;
    let signature = hmac_sha1_hex(sign_key.as_bytes(), string_to_sign.as_bytes())?;
    Ok(format!(
        "q-sign-algorithm={COS_SIGN_ALGORITHM}&q-ak={access_key}&q-sign-time={key_time}&q-key-time={key_time}&q-header-list={header_list}&q-url-param-list={url_param_list}&q-signature={signature}"
    ))
}

fn canonical_url_path(url: &Url) -> Result<String> {
    percent_decode_str(url.path())
        .decode_utf8()
        .map(|path| path.into_owned())
        .map_aster_err_ctx(
            "decode COS canonical request path",
            AsterError::storage_driver_error,
        )
}

fn host_header_value(url: &Url, missing_host_message: &'static str) -> Result<String> {
    let host = url.host().ok_or_else(|| {
        storage_driver_error(StorageErrorKind::Misconfigured, missing_host_message)
    })?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

pub(super) fn cos_virtual_hosted_s3_endpoint(endpoint: &str, bucket: &str) -> Result<String> {
    let mut url = Url::parse(endpoint)
        .map_aster_err_ctx("parse COS endpoint", AsterError::storage_driver_error)?;
    let host = url
        .host_str()
        .ok_or_else(|| {
            storage_driver_error(StorageErrorKind::Misconfigured, "COS endpoint missing host")
        })?
        .to_string();

    if let Some(root_host) = host.strip_prefix(&format!("{bucket}.")) {
        url.set_host(Some(root_host)).map_aster_err_ctx(
            "build COS S3 API endpoint",
            AsterError::storage_driver_error,
        )?;
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(String::from(url).trim_end_matches('/').to_string())
}

fn canonical_param_list(params: &[(&str, &str)]) -> String {
    let mut names = params
        .iter()
        .map(|(key, _)| percent_encode_query_key(key))
        .collect::<Vec<_>>();
    names.sort();
    names.join(";")
}

fn canonical_params(params: &[(&str, &str)]) -> String {
    let mut normalized = params
        .iter()
        .map(|(key, value)| {
            (
                percent_encode_query_key(key),
                percent_encode_query_value(value),
            )
        })
        .collect::<Vec<_>>();
    normalized.sort();
    normalized
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn canonical_header_list(headers: &[(&str, &str)]) -> String {
    let mut names = headers
        .iter()
        .map(|(key, _)| percent_encode_query_key(key.trim()))
        .collect::<Vec<_>>();
    names.sort();
    names.join(";")
}

fn canonical_headers(headers: &[(&str, &str)]) -> String {
    let mut normalized = headers
        .iter()
        .map(|(key, value)| {
            (
                percent_encode_query_key(key.trim()),
                percent_encode_query_value(&normalize_header_value(value)),
            )
        })
        .collect::<Vec<_>>();
    normalized.sort();
    normalized
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn normalize_header_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn percent_encode_query_key(value: &str) -> String {
    percent_encode(value.as_bytes(), COS_QUERY_ENCODE_SET)
        .to_string()
        .to_ascii_lowercase()
}

fn percent_encode_query_value(value: &str) -> String {
    percent_encode(value.as_bytes(), COS_QUERY_ENCODE_SET).to_string()
}

fn sha1_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn hmac_sha1_hex(key: &[u8], message: &[u8]) -> Result<String> {
    let mut mac = HmacSha1::new_from_slice(key)
        .map_aster_err_ctx("COS HMAC-SHA1 key", AsterError::storage_driver_error)?;
    mac.update(message);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
    use aws_sdk_s3::presigning::PresigningConfig;
    use aws_sdk_s3::primitives::ByteStream;
    use aws_smithy_http_client::test_util::{CaptureRequestReceiver, capture_request};
    use aws_smithy_types::body::SdkBody;
    use reqwest::header::AUTHORIZATION;
    use url::Url;

    use aster_drive_model::entities::storage_policy;
    use aster_drive_model::types::{
        DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions,
    };

    use super::TencentCosDriver;
    use super::{
        canonical_header_list, canonical_headers, canonical_param_list, canonical_params,
        canonical_url_path, configure_cos_auth, cos_authorization, host_header_value,
        percent_encode_query_key, percent_encode_query_value,
    };

    fn cos_sdk_client(
        response: Option<http::Response<SdkBody>>,
    ) -> (aws_sdk_s3::Client, CaptureRequestReceiver) {
        cos_sdk_client_with_session_token(response, None)
    }

    fn cos_sdk_client_with_session_token(
        response: Option<http::Response<SdkBody>>,
        session_token: Option<&str>,
    ) -> (aws_sdk_s3::Client, CaptureRequestReceiver) {
        let (http_client, receiver) = capture_request(response);
        let builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .http_client(http_client)
            .credentials_provider(Credentials::new(
                "AKIDEXAMPLE",
                "SECRETEXAMPLE",
                session_token.map(str::to_owned),
                None,
                "cos-auth-test",
            ))
            .region(Region::new("ap-guangzhou"))
            .endpoint_url("https://cos.ap-guangzhou.myqcloud.com")
            .force_path_style(false);
        let config = configure_cos_auth(builder).build();
        (aws_sdk_s3::Client::from_conf(config), receiver)
    }

    fn empty_success_response() -> http::Response<SdkBody> {
        http::Response::builder()
            .status(200)
            .body(SdkBody::empty())
            .expect("mock COS response")
    }

    fn authorization_key_time(authorization: &str) -> &str {
        authorization
            .split('&')
            .find_map(|component| component.strip_prefix("q-key-time="))
            .expect("q-key-time")
    }

    fn sample_driver(endpoint: &str) -> TencentCosDriver {
        TencentCosDriver::new(&storage_policy::Model {
            id: 1,
            name: "COS".to_string(),
            driver_type: DriverType::TencentCos,
            endpoint: endpoint.to_string(),
            bucket: "media-1250000000".to_string(),
            access_key: "AKIDEXAMPLE".to_string(),
            secret_key: "SECRETEXAMPLE".to_string(),
            base_path: String::new(),
            remote_node_id: None,
            remote_storage_target_key: None,
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::empty(),
            options: StoredStoragePolicyOptions::empty(),
            is_default: false,
            chunk_size: 5_242_880,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .expect("valid Tencent COS driver")
    }

    #[test]
    fn cos_authorization_matches_official_sdk_vector() {
        let authorization = cos_authorization(
            "PUT",
            "/testfile2",
            &[],
            &[
                ("host", "testbucket-125000000.cos.ap-guangzhou.myqcloud.com"),
                (
                    "x-cos-content-sha1",
                    "db8ac1c259eb89d4a131b253bacfca5f319d54f2",
                ),
                ("x-cos-stroage-class", "nearline"),
            ],
            "QmFzZTY0IGlzIGEgZ2VuZXJp",
            "AKIDZfbOA78asKUYBcXFrJD0a1ICvR98JM",
            "1480932292;1481012292",
        )
        .expect("COS authorization");

        assert_eq!(
            authorization,
            "q-sign-algorithm=sha1&q-ak=QmFzZTY0IGlzIGEgZ2VuZXJp&q-sign-time=1480932292;1481012292&q-key-time=1480932292;1481012292&q-header-list=host;x-cos-content-sha1;x-cos-stroage-class&q-url-param-list=&q-signature=ce4ac0ecbcdb30538b3fee0a97cc6389694ce53a"
        );
    }

    #[tokio::test]
    async fn aws_sdk_normal_request_uses_cos_authorization() {
        let (client, receiver) = cos_sdk_client(Some(empty_success_response()));

        client
            .get_object()
            .bucket("bucket-1250000000")
            .key("docs/report.txt")
            .range("bytes=0-9")
            .send()
            .await
            .expect("mock COS GET should deserialize");

        let request = receiver.expect_request();
        let authorization = request
            .headers()
            .get("authorization")
            .expect("COS Authorization header");
        assert!(authorization.starts_with("q-sign-algorithm=sha1&q-ak=AKIDEXAMPLE&"));
        assert!(authorization.contains("q-header-list=host;range"));
        assert!(authorization.contains("q-url-param-list="));
        assert!(!request.uri().contains("x-id="));
        let aws_headers = request
            .headers()
            .iter()
            // AWS SDK adds its telemetry header after signing. It is not part of
            // the COS canonical request; protocol/signature headers must be gone.
            .filter(|(name, _)| name.starts_with("x-amz-") && *name != "x-amz-user-agent")
            .collect::<Vec<_>>();
        assert!(
            aws_headers.is_empty(),
            "unexpected AWS headers: {aws_headers:?}"
        );
    }

    #[tokio::test]
    async fn aws_sdk_signs_decoded_cos_canonical_path_once() {
        let (client, receiver) = cos_sdk_client(Some(empty_success_response()));

        client
            .get_object()
            .bucket("bucket-1250000000")
            .key("\u{76ee}\u{5f55}/a b+%2F.txt")
            .send()
            .await
            .expect("mock COS GET should deserialize");

        let request = receiver.expect_request();
        let url = Url::parse(request.uri()).expect("captured COS URL");
        assert_eq!(url.path(), "/%E7%9B%AE%E5%BD%95/a%20b%2B%252F.txt");
        assert_eq!(
            canonical_url_path(&url).expect("canonical path"),
            "/\u{76ee}\u{5f55}/a b+%2F.txt"
        );

        let authorization = request
            .headers()
            .get("authorization")
            .expect("COS Authorization header");
        let key_time = authorization_key_time(authorization);
        let expected = cos_authorization(
            "GET",
            "/\u{76ee}\u{5f55}/a b+%2F.txt",
            &[],
            &[("host", "bucket-1250000000.cos.ap-guangzhou.myqcloud.com")],
            "AKIDEXAMPLE",
            "SECRETEXAMPLE",
            key_time,
        )
        .expect("expected COS authorization");
        let encoded_path_signature = cos_authorization(
            "GET",
            url.path(),
            &[],
            &[("host", "bucket-1250000000.cos.ap-guangzhou.myqcloud.com")],
            "AKIDEXAMPLE",
            "SECRETEXAMPLE",
            key_time,
        )
        .expect("encoded-path COS authorization");

        assert_eq!(authorization, expected);
        assert_ne!(authorization, encoded_path_signature);
    }

    #[tokio::test]
    async fn aws_sdk_put_request_has_no_aws_checksum_or_chunked_residue() {
        let (client, receiver) = cos_sdk_client(Some(empty_success_response()));

        client
            .put_object()
            .bucket("bucket-1250000000")
            .key("upload.bin")
            .content_type("application/octet-stream")
            .body(ByteStream::from_static(b"payload"))
            .send()
            .await
            .expect("mock COS PUT should deserialize");

        let request = receiver.expect_request();
        assert_eq!(request.method(), "PUT");
        assert_eq!(request.headers().get("content-length"), Some("7"));
        assert!(request.headers().get("transfer-encoding").is_none());
        assert!(request.headers().get("content-encoding").is_none());
        assert!(
            request
                .headers()
                .iter()
                .all(|(name, _)| !name.starts_with("x-amz-checksum-")
                    && name != "x-amz-sdk-checksum-algorithm")
        );
        let authorization = request
            .headers()
            .get("authorization")
            .expect("COS Authorization header");
        assert!(authorization.contains("q-header-list=content-length;content-type;host"));
    }

    #[tokio::test]
    async fn aws_sdk_delete_request_uses_cos_authorization() {
        let (client, receiver) = cos_sdk_client(Some(empty_success_response()));

        client
            .delete_object()
            .bucket("bucket-1250000000")
            .key("obsolete.bin")
            .send()
            .await
            .expect("mock COS DELETE should deserialize");

        let request = receiver.expect_request();
        assert_eq!(request.method(), "DELETE");
        assert!(!request.uri().contains("x-id="));
        assert!(
            request
                .headers()
                .get("authorization")
                .expect("COS Authorization header")
                .starts_with("q-sign-algorithm=sha1&q-ak=AKIDEXAMPLE&")
        );
    }

    #[tokio::test]
    async fn aws_sdk_session_token_is_signed_in_header_and_presigned_query() {
        let (client, receiver) =
            cos_sdk_client_with_session_token(Some(empty_success_response()), Some("SESSIONTOKEN"));

        client
            .head_object()
            .bucket("bucket-1250000000")
            .key("object.txt")
            .send()
            .await
            .expect("mock COS HEAD should deserialize");

        let request = receiver.expect_request();
        assert_eq!(
            request.headers().get("x-cos-security-token"),
            Some("SESSIONTOKEN")
        );
        assert!(
            request
                .headers()
                .get("authorization")
                .expect("COS Authorization header")
                .contains("q-header-list=host;x-cos-security-token")
        );

        let (client, receiver) = cos_sdk_client_with_session_token(None, Some("SESSIONTOKEN"));
        let presigned = client
            .get_object()
            .bucket("bucket-1250000000")
            .key("object.txt")
            .presigned(
                PresigningConfig::builder()
                    .start_time(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
                    .expires_in(Duration::from_secs(300))
                    .build()
                    .expect("presign config"),
            )
            .await
            .expect("COS presigned GET");

        receiver.expect_no_request();
        let query = Url::parse(presigned.uri())
            .expect("COS presigned URL")
            .query_pairs()
            .into_owned()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            query.get("x-cos-security-token").map(String::as_str),
            Some("SESSIONTOKEN")
        );
        assert_eq!(
            query.get("q-url-param-list").map(String::as_str),
            Some("x-cos-security-token")
        );
    }

    #[tokio::test]
    async fn aws_sdk_copy_request_renames_copy_source_before_cos_signing() {
        let (client, receiver) = cos_sdk_client(Some(empty_success_response()));

        let _ = client
            .copy_object()
            .bucket("bucket-1250000000")
            .key("dest.txt")
            .copy_source("bucket-1250000000/source.txt")
            .send()
            .await;

        let request = receiver.expect_request();
        assert_eq!(
            request.headers().get("x-cos-copy-source").map(str::trim),
            Some("bucket-1250000000/source.txt")
        );
        assert!(request.headers().get("x-amz-copy-source").is_none());
        let authorization = request
            .headers()
            .get("authorization")
            .expect("COS Authorization header");
        assert!(authorization.contains("q-header-list=host;x-cos-copy-source"));
    }

    #[tokio::test]
    async fn aws_sdk_presigned_get_uses_cos_query_signature_and_ttl() {
        let (client, receiver) = cos_sdk_client(None);
        let start = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let presigned = client
            .get_object()
            .bucket("bucket-1250000000")
            .key("docs/report.txt")
            .presigned(
                PresigningConfig::builder()
                    .start_time(start)
                    .expires_in(Duration::from_secs(600))
                    .build()
                    .expect("presign config"),
            )
            .await
            .expect("COS presigned GET");

        receiver.expect_no_request();
        let url = Url::parse(presigned.uri()).expect("COS presigned URL");
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            query.get("q-sign-algorithm").map(AsRef::as_ref),
            Some("sha1")
        );
        assert_eq!(query.get("q-ak").map(AsRef::as_ref), Some("AKIDEXAMPLE"));
        assert_eq!(
            query.get("q-sign-time").map(AsRef::as_ref),
            Some("1700000000;1700000600")
        );
        assert_eq!(query.get("q-header-list").map(AsRef::as_ref), Some("host"));
        assert_eq!(query.get("q-url-param-list").map(AsRef::as_ref), Some(""));
        assert!(!query.keys().any(|key| key.eq_ignore_ascii_case("x-id")));
        assert!(!query.keys().any(|key| key.starts_with("X-Amz-")));
    }

    #[tokio::test]
    async fn aws_sdk_presigned_upload_part_signs_multipart_query() {
        let (client, receiver) = cos_sdk_client(None);
        let presigned = client
            .upload_part()
            .bucket("bucket-1250000000")
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
            .expect("COS presigned upload part");

        receiver.expect_no_request();
        let url = Url::parse(presigned.uri()).expect("COS presigned part URL");
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(query.get("partNumber").map(AsRef::as_ref), Some("7"));
        assert_eq!(query.get("uploadId").map(AsRef::as_ref), Some("upload-id"));
        assert_eq!(
            query.get("q-url-param-list").map(AsRef::as_ref),
            Some("partnumber;uploadid")
        );
    }

    #[tokio::test]
    async fn aws_sdk_presigned_put_uses_cos_query_signature() {
        let (client, receiver) = cos_sdk_client(None);
        let presigned = client
            .put_object()
            .bucket("bucket-1250000000")
            .key("upload.bin")
            .content_type("application/octet-stream")
            .presigned(
                PresigningConfig::builder()
                    .start_time(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
                    .expires_in(Duration::from_secs(300))
                    .build()
                    .expect("presign config"),
            )
            .await
            .expect("COS presigned PUT");

        receiver.expect_no_request();
        let url = Url::parse(presigned.uri()).expect("COS presigned PUT URL");
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            query.get("q-sign-algorithm").map(AsRef::as_ref),
            Some("sha1")
        );
        assert_eq!(
            query.get("q-header-list").map(AsRef::as_ref),
            Some("content-type;host")
        );
        assert!(!query.keys().any(|key| key.starts_with("X-Amz-")));
    }

    #[tokio::test]
    async fn aws_sdk_multipart_operations_use_cos_signatures() {
        let (client, receiver) = cos_sdk_client(Some(empty_success_response()));
        let _ = client
            .create_multipart_upload()
            .bucket("bucket-1250000000")
            .key("video.bin")
            .send()
            .await;
        let request = receiver.expect_request();
        assert_eq!(request.method(), "POST");
        assert!(request.uri().contains("uploads="));
        assert!(
            request
                .headers()
                .get("authorization")
                .expect("COS Authorization header")
                .contains("q-url-param-list=uploads")
        );

        let (client, receiver) = cos_sdk_client(Some(empty_success_response()));
        let _ = client
            .upload_part()
            .bucket("bucket-1250000000")
            .key("video.bin")
            .upload_id("upload-id")
            .part_number(7)
            .body(ByteStream::from_static(b"part-data"))
            .send()
            .await;
        let request = receiver.expect_request();
        assert_eq!(request.method(), "PUT");
        assert!(
            request
                .headers()
                .get("authorization")
                .expect("COS Authorization header")
                .contains("q-url-param-list=partnumber;uploadid")
        );
        assert!(
            request
                .headers()
                .iter()
                .all(|(name, _)| !name.starts_with("x-amz-checksum-")
                    && name != "x-amz-sdk-checksum-algorithm")
        );

        let (client, receiver) = cos_sdk_client(Some(empty_success_response()));
        let _ = client
            .list_parts()
            .bucket("bucket-1250000000")
            .key("video.bin")
            .upload_id("upload-id")
            .send()
            .await;
        let request = receiver.expect_request();
        assert_eq!(request.method(), "GET");
        assert!(
            request
                .headers()
                .get("authorization")
                .expect("COS Authorization header")
                .contains("q-url-param-list=uploadid")
        );

        let completed_upload = aws_sdk_s3::types::CompletedMultipartUpload::builder()
            .parts(
                aws_sdk_s3::types::CompletedPart::builder()
                    .part_number(7)
                    .e_tag("etag-7")
                    .build(),
            )
            .build();
        let (client, receiver) = cos_sdk_client(Some(empty_success_response()));
        let _ = client
            .complete_multipart_upload()
            .bucket("bucket-1250000000")
            .key("video.bin")
            .upload_id("upload-id")
            .multipart_upload(completed_upload)
            .send()
            .await;
        let request = receiver.expect_request();
        assert_eq!(request.method(), "POST");
        assert!(
            request
                .headers()
                .get("authorization")
                .expect("COS Authorization header")
                .contains("q-url-param-list=uploadid")
        );
        assert!(request.headers().get("content-length").is_some());

        let (client, receiver) = cos_sdk_client(Some(empty_success_response()));
        let _ = client
            .abort_multipart_upload()
            .bucket("bucket-1250000000")
            .key("video.bin")
            .upload_id("upload-id")
            .send()
            .await;
        let request = receiver.expect_request();
        assert_eq!(request.method(), "DELETE");
        assert!(
            request
                .headers()
                .get("authorization")
                .expect("COS Authorization header")
                .contains("q-url-param-list=uploadid")
        );
    }

    #[test]
    fn query_percent_encode_set_matches_cos_urlencode_rules() {
        let cases = [
            (" ", "%20", "%20"),
            (";", "%3b", "%3B"),
            ("!", "%21", "%21"),
            ("<", "%3c", "%3C"),
            ("\"", "%22", "%22"),
            ("=", "%3d", "%3D"),
            ("#", "%23", "%23"),
            (">", "%3e", "%3E"),
            ("$", "%24", "%24"),
            ("?", "%3f", "%3F"),
            ("%", "%25", "%25"),
            ("@", "%40", "%40"),
            ("&", "%26", "%26"),
            ("[", "%5b", "%5B"),
            ("'", "%27", "%27"),
            ("\\", "%5c", "%5C"),
            ("(", "%28", "%28"),
            ("]", "%5d", "%5D"),
            (")", "%29", "%29"),
            ("^", "%5e", "%5E"),
            ("*", "%2a", "%2A"),
            ("`", "%60", "%60"),
            ("+", "%2b", "%2B"),
            ("{", "%7b", "%7B"),
            (",", "%2c", "%2C"),
            ("|", "%7c", "%7C"),
            ("/", "%2f", "%2F"),
            ("}", "%7d", "%7D"),
            (":", "%3a", "%3A"),
        ];

        for (input, expected_key, expected_value) in cases {
            assert_eq!(
                percent_encode_query_key(input),
                expected_key,
                "query key input={input:?}"
            );
            assert_eq!(
                percent_encode_query_value(input),
                expected_value,
                "query value input={input:?}"
            );
        }
    }

    #[test]
    fn canonical_cos_params_lowercase_encoded_keys_but_not_values() {
        let params = [
            ("imageMogr2/thumbnail/320x240>/format/webp", ""),
            (
                "response-content-disposition",
                "attachment; filename=\"报告 1.pdf\"",
            ),
        ];

        assert_eq!(
            canonical_param_list(&params),
            "imagemogr2%2fthumbnail%2f320x240%3e%2fformat%2fwebp;response-content-disposition"
        );
        assert_eq!(
            canonical_params(&params),
            "imagemogr2%2fthumbnail%2f320x240%3e%2fformat%2fwebp=&response-content-disposition=attachment%3B%20filename%3D%22%E6%8A%A5%E5%91%8A%201.pdf%22"
        );
    }

    #[test]
    fn canonical_cos_params_cover_empty_special_and_already_encoded_values() {
        let empty = [("", "")];
        assert_eq!(canonical_param_list(&empty), "");
        assert_eq!(canonical_params(&empty), "=");

        let special = [("KEY", "!@#$%^&*()")];
        assert_eq!(canonical_param_list(&special), "key");
        assert_eq!(
            canonical_params(&special),
            "key=%21%40%23%24%25%5E%26%2A%28%29"
        );

        let already_encoded = [("key", "value%20with%20encoded")];
        assert_eq!(canonical_param_list(&already_encoded), "key");
        assert_eq!(
            canonical_params(&already_encoded),
            "key=value%2520with%2520encoded"
        );

        let mixed_case = [("MiXeD/Key", "Value%2FCase")];
        assert_eq!(canonical_param_list(&mixed_case), "mixed%2fkey");
        assert_eq!(canonical_params(&mixed_case), "mixed%2fkey=Value%252FCase");
    }

    #[test]
    fn canonical_cos_headers_sort_lowercase_and_normalize_values() {
        let headers = [
            ("Content-Type", " application/xml;  charset=utf-8 "),
            ("Host", "bucket-1250000000.cos.ap-guangzhou.myqcloud.com"),
            ("x-cos-security-token", " token value "),
        ];

        assert_eq!(
            canonical_header_list(&headers),
            "content-type;host;x-cos-security-token"
        );
        assert_eq!(
            canonical_headers(&headers),
            "content-type=application%2Fxml%3B%20charset%3Dutf-8&host=bucket-1250000000.cos.ap-guangzhou.myqcloud.com&x-cos-security-token=token%20value"
        );
    }

    #[test]
    fn canonical_cos_headers_preserve_and_sort_duplicate_values() {
        let headers = [
            ("Host", "bucket-1250000000.cos.ap-guangzhou.myqcloud.com"),
            ("host", "duplicate.example.com"),
            ("Content-Type", " application/xml "),
            ("content-type", " text/plain "),
        ];

        assert_eq!(
            canonical_header_list(&headers),
            "content-type;content-type;host;host"
        );
        assert_eq!(
            canonical_headers(&headers),
            "content-type=application%2Fxml&content-type=text%2Fplain&host=bucket-1250000000.cos.ap-guangzhou.myqcloud.com&host=duplicate.example.com"
        );
    }

    #[test]
    fn canonical_cos_params_sort_duplicate_values_like_official_sdk() {
        let params = [("part", "z"), ("part", "a"), ("Part", "m")];

        assert_eq!(canonical_param_list(&params), "part;part;part");
        assert_eq!(canonical_params(&params), "part=a&part=m&part=z");
    }

    #[test]
    fn signed_cos_query_url_includes_non_default_port_in_host_signature() {
        let driver = sample_driver("http://cos.ap-guangzhou.myqcloud.com:9000");
        let default_port_driver = sample_driver("http://cos.ap-guangzhou.myqcloud.com");

        let (url, _) = driver
            .signed_cos_query_url("object.txt", &[], "1700000000;1700000600")
            .expect("signed URL");
        let (default_port_url, _) = default_port_driver
            .signed_cos_query_url("object.txt", &[], "1700000000;1700000600")
            .expect("signed URL without explicit port");
        let sign = url
            .query_pairs()
            .find_map(|(key, value)| (key == "sign").then_some(value.into_owned()))
            .expect("sign query parameter");
        let default_port_sign = default_port_url
            .query_pairs()
            .find_map(|(key, value)| (key == "sign").then_some(value.into_owned()))
            .expect("sign query parameter");

        assert!(url.as_str().contains(":9000/"));
        assert!(sign.contains("q-header-list=host"));
        assert_ne!(sign, default_port_sign);
    }

    #[test]
    fn signed_cos_headers_include_non_default_port_in_host_signature() {
        let driver = sample_driver("http://cos.ap-guangzhou.myqcloud.com:9000");
        let default_port_driver = sample_driver("http://cos.ap-guangzhou.myqcloud.com");
        let url = driver.bucket_cors_url().expect("bucket CORS URL");
        let default_port_url = default_port_driver
            .bucket_cors_url()
            .expect("bucket CORS URL without explicit port");

        let headers = driver
            .signed_cos_request_headers("PUT", &url, &[], "1700000000;1700000600")
            .expect("signed headers");
        let default_port_headers = default_port_driver
            .signed_cos_request_headers("PUT", &default_port_url, &[], "1700000000;1700000600")
            .expect("signed headers without explicit port");
        let authorization = headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .expect("authorization header");
        let default_port_authorization = default_port_headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .expect("authorization header");

        assert!(url.as_str().contains(":9000/"));
        assert!(authorization.contains("q-header-list=host"));
        assert_ne!(authorization, default_port_authorization);
    }

    #[test]
    fn signed_cos_headers_format_ipv6_host_with_brackets_and_port() {
        let driver = sample_driver("http://cos.ap-guangzhou.myqcloud.com");
        let url = Url::parse("http://[::1]:9000/").expect("valid IPv6 URL");

        let headers = driver
            .signed_cos_request_headers("PUT", &url, &[], "1700000000;1700000600")
            .expect("signed headers");
        let authorization = headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .expect("authorization header");
        let expected_host = "[::1]:9000";

        assert_eq!(
            host_header_value(&url, "missing host").expect("host"),
            expected_host
        );
        assert!(authorization.contains("q-header-list=host"));
    }

    #[test]
    fn host_header_value_omits_default_ports_and_formats_ipv6() {
        let cases = [
            ("http://example.com/", "example.com"),
            ("http://example.com:80/", "example.com"),
            ("https://example.com:443/", "example.com"),
            ("https://example.com:9443/", "example.com:9443"),
            ("http://[::1]/", "[::1]"),
            ("http://[::1]:9000/", "[::1]:9000"),
        ];

        for (input, expected) in cases {
            let url = Url::parse(input).expect("valid URL");
            assert_eq!(
                host_header_value(&url, "missing host").expect("host"),
                expected,
                "{input}"
            );
        }
    }
}
