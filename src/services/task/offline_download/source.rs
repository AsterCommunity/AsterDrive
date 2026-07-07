use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration as StdDuration;

use url::Url;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::services::task::types::CreateOfflineDownloadTaskParams;
use crate::utils::numbers::{i64_to_u64, u128_to_u64};

use super::THROTTLED_DOWNLOAD_TIMEOUT_SLACK_SECS;

#[derive(Debug)]
pub(super) struct NormalizedOfflineDownloadRequest {
    pub(super) url: Url,
    pub(super) filename: Option<String>,
    pub(super) target_folder_id: Option<i64>,
    pub(super) expected_sha256: Option<String>,
}

pub(in crate::services::task) struct ResolvedSourceHost {
    pub(super) domain: String,
    pub(super) socket_addrs: Vec<SocketAddr>,
}

pub(super) fn normalize_offline_download_request(
    params: CreateOfflineDownloadTaskParams,
) -> Result<NormalizedOfflineDownloadRequest> {
    let url = parse_and_validate_source_url(&params.url)?;
    let filename = match params.filename {
        Some(filename) => {
            let trimmed = filename.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(crate::utils::normalize_validate_name(trimmed)?)
            }
        }
        None => None,
    };
    let expected_sha256 = match params.expected_sha256 {
        Some(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                None
            } else {
                validate_sha256_hex(&normalized)?;
                Some(normalized)
            }
        }
        None => None,
    };

    Ok(NormalizedOfflineDownloadRequest {
        url,
        filename,
        target_folder_id: params.target_folder_id,
        expected_sha256,
    })
}

pub(super) fn parse_and_validate_source_url(raw: &str) -> Result<Url> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error("url cannot be empty"));
    }
    let url = Url::parse(trimmed)
        .map_aster_err_ctx("parse offline download url", AsterError::validation_error)?;
    match url.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(AsterError::validation_error(
                "offline download only supports http and https URLs",
            ));
        }
    }
    if url.host_str().is_none() {
        return Err(AsterError::validation_error(
            "offline download url must include a host",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AsterError::validation_error(
            "offline download url must not include credentials",
        ));
    }
    Ok(url)
}

pub(super) fn effective_offline_download_request_timeout(
    configured_timeout: StdDuration,
    max_bytes: i64,
    max_bytes_per_sec: Option<u64>,
) -> Result<StdDuration> {
    let Some(max_bytes_per_sec) = max_bytes_per_sec else {
        return Ok(configured_timeout);
    };
    if max_bytes <= 0 || max_bytes_per_sec == 0 {
        return Ok(configured_timeout);
    }

    let max_bytes = i64_to_u64(max_bytes, "offline download max file size")?;
    let expected_secs = u128::from(max_bytes).div_ceil(u128::from(max_bytes_per_sec));
    let expected_secs =
        expected_secs.saturating_add(u128::from(THROTTLED_DOWNLOAD_TIMEOUT_SLACK_SECS));
    let expected_secs = u128_to_u64(expected_secs, "offline download effective timeout")?;
    Ok(configured_timeout.max(StdDuration::from_secs(expected_secs)))
}

pub(in crate::services::task) async fn resolve_source_host(
    url: &Url,
) -> Result<ResolvedSourceHost> {
    let host = url
        .host_str()
        .ok_or_else(|| AsterError::validation_error("offline download url must include a host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| AsterError::validation_error("offline download url has no port"))?;
    let socket_addrs = tokio::net::lookup_host((host, port))
        .await
        .map_aster_err_ctx(
            "resolve offline download host",
            AsterError::validation_error,
        )?
        .collect::<Vec<_>>();
    if socket_addrs.is_empty() {
        return Err(AsterError::validation_error(
            "offline download host did not resolve to any address",
        ));
    }

    let mut unique_ips = BTreeSet::new();
    let mut safe_addrs = Vec::new();
    for socket_addr in socket_addrs {
        let ip = socket_addr.ip();
        if !unique_ips.insert(ip) {
            continue;
        }
        validate_public_download_ip(ip)?;
        safe_addrs.push(SocketAddr::new(ip, port));
    }

    Ok(ResolvedSourceHost {
        domain: host.to_ascii_lowercase(),
        socket_addrs: safe_addrs,
    })
}

pub(super) fn validate_public_download_ip(ip: IpAddr) -> Result<()> {
    let blocked = match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || octets[0] == 0
                || octets[0] >= 240
                || octets == [169, 254, 169, 254]
                || (octets[0] == 100 && (octets[1] & 0xc0 == 0x40))
                || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return validate_public_download_ip(IpAddr::V4(mapped));
            }
            ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || ip.segments()[0] & 0xfe00 == 0xfc00
                || ip.segments()[0] & 0xffc0 == 0xfe80
                || ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8
        }
    };
    if blocked {
        return Err(AsterError::validation_error(
            "offline download host resolves to a blocked address",
        ));
    }
    Ok(())
}

fn validate_sha256_hex(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AsterError::validation_error(
            "expected_sha256 must be a 64-character hex string",
        ));
    }
    Ok(())
}

pub(in crate::services::task) fn redact_url_for_display(url: &Url) -> String {
    let mut redacted = url.clone();
    let _ = redacted.set_username("");
    let _ = redacted.set_password(None);
    redacted.set_query(None);
    redacted.set_fragment(None);
    redacted.to_string()
}
