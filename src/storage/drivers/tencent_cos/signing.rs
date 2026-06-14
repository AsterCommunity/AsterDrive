use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{AsciiSet, CONTROLS, percent_encode};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha1::{Digest, Sha1};
use url::Url;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};

use super::TencentCosDriver;

type HmacSha1 = Hmac<Sha1>;

const COS_SIGN_ALGORITHM: &str = "sha1";

// Tencent COS request-signature docs require UrlEncode for canonical query and
// header keys/values. Query/header keys are lowercased after encoding, while
// values keep their encoded case. The documented UrlEncode symbol table is:
// space ; ! < " = # > $ ? % @ & [ ' \ ( ] ) ^ * ` + { , | / } :
// Source: https://cloud.tencent.com/document/api/436/7778
const COS_PATH_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');
const COS_QUERY_ENCODE_SET: &AsciiSet = &COS_PATH_ENCODE_SET
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
        let path_for_sign = url.path().to_string();
        let url_param_list = canonical_param_list(params);
        let http_params = canonical_params(params);
        let http_headers = format!("host={}", percent_encode_path(&host));
        let http_string = format!("get\n{path_for_sign}\n{http_params}\n{http_headers}\n");
        let string_to_sign = format!(
            "{COS_SIGN_ALGORITHM}\n{key_time}\n{}\n",
            sha1_hex(http_string.as_bytes())
        );
        let sign_key = hmac_sha1_hex(self.secret_key.as_bytes(), key_time.as_bytes())?;
        let signature = hmac_sha1_hex(sign_key.as_bytes(), string_to_sign.as_bytes())?;
        let authorization = format!(
            "q-sign-algorithm={COS_SIGN_ALGORITHM}&q-ak={}&q-sign-time={key_time}&q-key-time={key_time}&q-header-list=host&q-url-param-list={url_param_list}&q-signature={signature}",
            self.access_key
        );

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

        let header_list = canonical_header_list(&signed_headers);
        let http_headers = canonical_headers(&signed_headers);
        let params = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        let param_refs = params
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let url_param_list = canonical_param_list(&param_refs);
        let http_params = canonical_params(&param_refs);
        let http_string = format!(
            "{}\n{}\n{}\n{}\n",
            method.to_ascii_lowercase(),
            url.path(),
            http_params,
            http_headers
        );
        let string_to_sign = format!(
            "{COS_SIGN_ALGORITHM}\n{key_time}\n{}\n",
            sha1_hex(http_string.as_bytes())
        );
        let sign_key = hmac_sha1_hex(self.secret_key.as_bytes(), key_time.as_bytes())?;
        let signature = hmac_sha1_hex(sign_key.as_bytes(), string_to_sign.as_bytes())?;
        let authorization = format!(
            "q-sign-algorithm={COS_SIGN_ALGORITHM}&q-ak={}&q-sign-time={key_time}&q-key-time={key_time}&q-header-list={header_list}&q-url-param-list={url_param_list}&q-signature={signature}",
            self.access_key
        );

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
    normalized.sort_by(|a, b| a.0.cmp(&b.0));
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
    names.dedup();
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
    normalized.sort_by(|a, b| a.0.cmp(&b.0));
    normalized.dedup_by(|a, b| a.0 == b.0);
    normalized
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn normalize_header_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn percent_encode_path(value: &str) -> String {
    percent_encode(value.as_bytes(), COS_PATH_ENCODE_SET).to_string()
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
    use reqwest::header::AUTHORIZATION;
    use url::Url;

    use crate::entities::storage_policy;
    use crate::types::{DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};

    use super::TencentCosDriver;
    use super::{
        canonical_header_list, canonical_headers, canonical_param_list, canonical_params,
        host_header_value, percent_encode_path, percent_encode_query_key,
        percent_encode_query_value,
    };

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
    fn path_percent_encode_set_matches_cos_path_rules() {
        let cases = [
            (" ", "%20"),
            ("\"", "%22"),
            ("#", "%23"),
            ("%", "%25"),
            ("<", "%3C"),
            (">", "%3E"),
            ("?", "%3F"),
            ("`", "%60"),
            ("{", "%7B"),
            ("}", "%7D"),
        ];

        for (input, expected) in cases {
            assert_eq!(percent_encode_path(input), expected, "input={input:?}");
        }
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
    fn canonical_cos_headers_deduplicate_names_consistently_with_header_list() {
        let headers = [
            ("Host", "bucket-1250000000.cos.ap-guangzhou.myqcloud.com"),
            ("host", "duplicate.example.com"),
            ("Content-Type", " application/xml "),
            ("content-type", " text/plain "),
        ];

        assert_eq!(canonical_header_list(&headers), "content-type;host");
        assert_eq!(
            canonical_headers(&headers),
            "content-type=application%2Fxml&host=bucket-1250000000.cos.ap-guangzhou.myqcloud.com"
        );
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
