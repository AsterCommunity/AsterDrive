use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{AsciiSet, CONTROLS, percent_encode};
use sha1::{Digest, Sha1};
use url::Url;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};

use super::TencentCosDriver;

type HmacSha1 = Hmac<Sha1>;

const COS_SIGN_ALGORITHM: &str = "sha1";
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
        let http_headers = format!("host={}", percent_encode_lower(host));
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
        .map(|(key, _)| percent_encode_query_lower(key))
        .collect::<Vec<_>>();
    names.sort();
    names.join(";")
}

fn canonical_params(params: &[(&str, &str)]) -> String {
    let mut normalized = params
        .iter()
        .map(|(key, value)| {
            (
                percent_encode_query_lower(key),
                percent_encode_query_lower(value),
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

fn percent_encode_lower(value: &str) -> String {
    percent_encode(value.as_bytes(), COS_PATH_ENCODE_SET)
        .to_string()
        .to_ascii_lowercase()
}

fn percent_encode_query_lower(value: &str) -> String {
    percent_encode(value.as_bytes(), COS_QUERY_ENCODE_SET)
        .to_string()
        .to_ascii_lowercase()
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
