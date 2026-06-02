use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{AsciiSet, CONTROLS, percent_encode};
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
        let host = url.host_str().ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "COS object URL missing host",
            )
        })?;
        let path_for_sign = url.path().to_string();
        let url_param_list = canonical_param_list(params);
        let http_params = canonical_params(params);
        let http_headers = format!("host={}", percent_encode_path(host));
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
    use super::{
        canonical_param_list, canonical_params, percent_encode_path, percent_encode_query_key,
        percent_encode_query_value,
    };

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
}
