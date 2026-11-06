//! Go SDK auth/credentials.go: padded URL-safe Base64 and HMAC-SHA1.
use super::{QiniuDriver, invalid};
use aster_drive_storage::Result;
use base64::{Engine, engine::general_purpose::URL_SAFE};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha1::Sha1;

pub(super) fn encode(value: impl AsRef<[u8]>) -> String {
    URL_SAFE.encode(value)
}

impl QiniuDriver {
    pub(super) fn sign(&self, data: &[u8]) -> Result<String> {
        let mut mac = Hmac::<Sha1>::new_from_slice(self.credentials.secret_key.as_bytes())
            .map_err(|_| invalid("invalid Qiniu signing key"))?;
        mac.update(data);
        Ok(format!(
            "{}:{}",
            self.credentials.access_key,
            encode(mac.finalize().into_bytes())
        ))
    }

    pub(super) fn upload_token(
        &self,
        key: &str,
        deadline: i64,
        max_size: Option<u64>,
        insert_only: bool,
    ) -> Result<String> {
        let mut policy =
            json!({"scope":format!("{}:{key}", self.config.bucket),"deadline":deadline});
        if let Some(size) = max_size {
            policy["fsizeLimit"] = json!(size);
        }
        if insert_only {
            policy["insertOnly"] = json!(1);
        }
        let encoded =
            encode(serde_json::to_vec(&policy).map_err(|_| invalid("invalid upload policy"))?);
        Ok(format!("{}:{encoded}", self.sign(encoded.as_bytes())?))
    }

    pub(super) fn sign_management(&self, request: &mut reqwest::Request) -> Result<()> {
        let date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let content_type = request
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(|value| value.to_str().map(str::to_owned))
            .transpose()
            .map_err(|_| invalid("invalid management content type"))?
            .unwrap_or_else(|| "application/x-www-form-urlencoded".into());
        let canonical = management_signing_payload(request, &date, &content_type)?;
        let authorization = format!("Qiniu {}", self.sign(canonical.as_bytes())?);
        for (name, value) in [
            ("content-type", content_type.as_str()),
            ("x-qiniu-date", date.as_str()),
            ("authorization", authorization.as_str()),
        ] {
            request.headers_mut().insert(
                reqwest::header::HeaderName::from_static(name),
                reqwest::header::HeaderValue::from_str(value)
                    .map_err(|_| invalid("invalid authentication header"))?,
            );
        }
        Ok(())
    }
}

/// Canonical Qiniu v2 payload. JSON bucket configuration and form management
/// requests must sign their actual Content-Type and body, per the pinned SDK.
pub(super) fn management_signing_payload(
    request: &reqwest::Request,
    date: &str,
    content_type: &str,
) -> Result<String> {
    let host = request
        .url()
        .host_str()
        .ok_or_else(|| invalid("management URL has no host"))?;
    let host = match request.url().port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    };
    let path = match request.url().query() {
        Some(q) => format!("{}?{q}", request.url().path()),
        None => request.url().path().to_string(),
    };
    let mut canonical = format!(
        "{} {path}\nHost: {host}\nContent-Type: {content_type}\nX-Qiniu-Date: {date}\n\n",
        request.method()
    );
    if matches!(
        content_type,
        "application/json" | "application/x-www-form-urlencoded"
    ) && let Some(body) = request.body().and_then(reqwest::Body::as_bytes)
    {
        canonical
            .push_str(std::str::from_utf8(body).map_err(|_| invalid("invalid management body"))?);
    }
    Ok(canonical)
}
