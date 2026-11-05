//! Bucket CORS wire contract: qiniu/go-sdk v7.30.0 / f278ca5c9cc5d4a85f304e4b73f2b0b36437c66e.
//! UC owns /corsRules/{get,set}/{bucket}; neither RS nor upload/download hosts
//! provide this management API. Rules have no ID and are never claimed/deleted.
use super::{QiniuDriver, http, invalid};
use aster_drive_storage::{Result, StorageErrorKind, storage_driver_error};
use aster_forge_http::read_reqwest_body_limited;
use reqwest::{Method, Response};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const MAX_RULES: usize = 10;
// AsterDrive's defensive control-body limit; the SDK does not document a
// provider byte limit. Never truncate a larger configuration and write it back.
const MAX_JSON_BYTES: usize = 64 * 1024;
const EXPOSED_HEADERS: &[&str] = &[
    "ETag",
    "Content-Length",
    "Content-Range",
    "Content-Disposition",
    "Accept-Ranges",
    "X-Reqid",
    "X-Log",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct CorsRule {
    allowed_origin: Vec<String>,
    allowed_method: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    allowed_header: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exposed_header: Vec<String>,
    #[serde(default)]
    max_age: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct QiniuCorsApplyResult {
    pub bucket: String,
    pub allowed_origins: Vec<String>,
    pub changed: bool,
    pub preserved_rule_count: usize,
    pub request_id: Option<String>,
}

impl QiniuDriver {
    async fn cors_request(&self, method: Method, body: Option<Vec<u8>>) -> Result<Response> {
        let operation = if method == Method::GET { "get" } else { "set" };
        let mut url = self.bucket_api.clone();
        url.path_segments_mut()
            .map_err(|_| invalid("invalid Kodo UC endpoint"))?
            .clear()
            .extend(["corsRules", operation, self.config.bucket.as_str()]);
        let mut request = self
            .control_client
            .request(method, url)
            .timeout(Duration::from_secs(30));
        if let Some(body) = body {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let mut request = request.build().map_err(http::transport_error)?;
        self.sign_management(&mut request)?;
        http::checked(self.control_client.execute(request).await.map_err(http::transport_error)?)
            .await.map_err(|error| {
                if matches!(error.kind(), StorageErrorKind::Permission | StorageErrorKind::Auth) {
                    storage_driver_error(error.kind(), format!(
                        "{}. Grant this credential permission to read and set CORS on the target bucket; an upload token cannot configure bucket CORS.", error.message(),
                    ))
                } else { error }
            })
    }

    async fn get_bucket_cors(&self) -> Result<(Vec<CorsRule>, Option<String>)> {
        let response = self.cors_request(Method::GET, None).await?;
        let request_id = http::request_id(&response);
        let body = read_reqwest_body_limited(response, "Kodo CORS configuration", MAX_JSON_BYTES,
            |_| invalid("Kodo CORS response exceeds AsterDrive's 64 KiB limit or could not be read; configuration was not truncated"),
        ).await?;
        Ok((parse_rules(&body)?, request_id))
    }

    pub(crate) async fn configure_asterdrive_cors(
        &self,
        origins: &[String],
    ) -> Result<QiniuCorsApplyResult> {
        validate_origins(origins)?;
        let (mut rules, mut request_id) = self.get_bucket_cors().await?;
        let preserved_rule_count = rules.len();
        let changed = needs_rule(&rules, origins)?;
        if changed {
            if rules.len() == MAX_RULES {
                return Err(invalid(
                    "Kodo supports at most 10 CORS rules; adjust existing rules in the Qiniu console before retrying",
                ));
            }
            rules.push(asterdrive_rule(origins));
            let body =
                serde_json::to_vec(&rules).map_err(|_| invalid("cannot encode Kodo CORS rules"))?;
            if body.len() > MAX_JSON_BYTES {
                return Err(invalid(
                    "merged Kodo CORS exceeds AsterDrive's 64 KiB limit; existing rules were not changed",
                ));
            }
            let response = self.cors_request(Method::POST, Some(body)).await?;
            request_id = http::request_id(&response);
            // Read back the complete ordered rule set. A successful write is
            // configuration acceptance, never browser/upload/CDN acceptance.
            let (observed, read_id) = self.get_bucket_cors().await.map_err(|error| {
                storage_driver_error(error.kind(), format!(
                    "Kodo accepted the CORS write but verification failed: {}. Inspect the bucket configuration before retrying.", error.message(),
                ))
            })?;
            if !rules_equivalent(&rules, &observed) {
                return Err(storage_driver_error(
                    StorageErrorKind::Precondition,
                    "Kodo accepted the CORS write but readback differs; inspect the bucket configuration and avoid concurrent CORS edits",
                ));
            }
            request_id = request_id.or(read_id);
        }
        Ok(QiniuCorsApplyResult {
            bucket: self.config.bucket.clone(),
            allowed_origins: origins.to_vec(),
            changed,
            preserved_rule_count,
            request_id,
        })
    }
}

fn validate_origins(origins: &[String]) -> Result<()> {
    if origins.is_empty() {
        return Err(invalid("Kodo CORS requires explicit public site origins"));
    }
    for origin in origins {
        let url = url::Url::parse(origin).map_err(|_| invalid("invalid Kodo CORS origin"))?;
        if !matches!(url.scheme(), "http" | "https")
            || origin.contains('*')
            || url.origin().ascii_serialization() != *origin
        {
            return Err(invalid(
                "Kodo CORS requires canonical HTTP(S) origins without wildcards, paths or credentials",
            ));
        }
    }
    Ok(())
}

fn parse_rules(body: &[u8]) -> Result<Vec<CorsRule>> {
    // Go SDK's array decoder also accepts null for an unconfigured bucket.
    let rules = serde_json::from_slice::<Option<Vec<CorsRule>>>(body)
        .map_err(|_| {
            invalid(
                "invalid or unsupported Kodo CORS configuration; existing rules were not changed",
            )
        })?
        .unwrap_or_default();
    if rules.len() > MAX_RULES {
        return Err(invalid("Kodo CORS configuration exceeds 10 rules"));
    }
    if rules
        .iter()
        .any(|r| r.allowed_origin.is_empty() || r.allowed_method.is_empty() || r.max_age < 0)
    {
        return Err(invalid(
            "invalid Kodo CORS rule; existing rules were not changed",
        ));
    }
    Ok(rules)
}

fn asterdrive_rule(origins: &[String]) -> CorsRule {
    CorsRule {
        allowed_origin: origins.to_vec(),
        allowed_method: ["POST", "PUT", "GET", "HEAD"].map(str::to_owned).into(),
        allowed_header: ["Authorization", "Content-Type", "Range"]
            .map(str::to_owned)
            .into(),
        exposed_header: EXPOSED_HEADERS.iter().map(|s| (*s).to_owned()).collect(),
        max_age: 600,
    }
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => {
            !suffix.contains('*')
                && value.len() >= prefix.len() + suffix.len()
                && value.starts_with(prefix)
                && value.ends_with(suffix)
        }
        None => pattern == value,
    }
}

fn needs_rule(rules: &[CorsRule], origins: &[String]) -> Result<bool> {
    // POST form upload, native v2 part PUT, and private Blob GET/HEAD/Range.
    // Model simple GET separately: an earlier matching rule can shadow exposed
    // headers even when a later rule would satisfy its preflight.
    let requests: &[(&str, &[&str])] = &[
        ("POST", &[]),
        ("POST", &["content-type"]),
        ("PUT", &["authorization", "content-type"]),
        ("GET", &[]),
        ("GET", &["range"]),
        ("HEAD", &[]),
    ];
    let mut missing = false;
    for origin in origins {
        for (method, headers) in requests {
            let matched = rules.iter().find(|rule| {
                rule.allowed_origin
                    .iter()
                    .any(|pattern| wildcard_matches(pattern, origin))
                    && rule
                        .allowed_method
                        .iter()
                        .any(|m| m.eq_ignore_ascii_case(method))
                    && headers.iter().all(|h| {
                        rule.allowed_header
                            .iter()
                            .any(|p| p == "*" || p.eq_ignore_ascii_case(h))
                    })
            });
            match matched {
                None => missing = true,
                Some(rule)
                    if matches!(*method, "GET" | "HEAD")
                        && EXPOSED_HEADERS.iter().any(|required| {
                            // X-Reqid and X-Log are always exposed by Kodo per the SDK.
                            !matches!(*required, "X-Reqid" | "X-Log" | "Content-Length")
                                && !rule
                                    .exposed_header
                                    .iter()
                                    .any(|h| h.eq_ignore_ascii_case(required))
                        }) =>
                {
                    return Err(storage_driver_error(
                        StorageErrorKind::Precondition,
                        "an earlier matching Kodo CORS rule lacks required exposed headers; adjust that rule in the Qiniu console (no rules were written)",
                    ));
                }
                Some(_) => {}
            }
        }
    }
    Ok(missing)
}

fn rules_equivalent(left: &[CorsRule], right: &[CorsRule]) -> bool {
    fn normalized(rule: &CorsRule) -> CorsRule {
        let mut rule = rule.clone();
        rule.allowed_origin.sort();
        rule.allowed_origin.dedup();
        for values in [
            &mut rule.allowed_method,
            &mut rule.allowed_header,
            &mut rule.exposed_header,
        ] {
            *values = values.iter().map(|v| v.to_ascii_lowercase()).collect();
            values.sort();
            values.dedup();
        }
        rule
    }
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| normalized(a) == normalized(b))
}

#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod tests;
