//! CSRF 中间件子模块：`source`。

use actix_web::{
    HttpRequest,
    dev::ServiceRequest,
    http::{Method, header},
};

use crate::api::subcode::ApiSubcode;
use crate::config::{RuntimeConfig, cors, site_url};
use crate::errors::{
    MapAsterErr, Result, auth_forbidden_with_subcode, validation_error_with_subcode,
};

const MAX_REQUEST_SCHEME_LEN: usize = 16;
const MAX_REQUEST_HOST_LEN: usize = 512;
const MAX_REFERER_AUTHORITY_LEN: usize = MAX_REQUEST_HOST_LEN + 16;
const MAX_SOURCE_HEADER_LEN: usize = 2048;
const MAX_SEC_FETCH_SITE_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSourceMode {
    OptionalWhenPresent,
    Required,
}

pub fn is_unsafe_method(method: &Method) -> bool {
    !matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    )
}

pub fn ensure_request_source_allowed(
    req: &HttpRequest,
    runtime_config: &RuntimeConfig,
    mode: RequestSourceMode,
) -> Result<()> {
    let conn = req.connection_info();
    let request_origin = request_origin(conn.scheme(), conn.host())?;
    ensure_headers_allowed(
        header_value(req, header::ORIGIN),
        header_value(req, header::REFERER),
        header_value(req, header::HeaderName::from_static("sec-fetch-site")),
        &request_origin,
        &site_url::public_site_urls(runtime_config),
        mode,
    )
}

pub fn ensure_service_request_source_allowed(
    req: &ServiceRequest,
    runtime_config: &RuntimeConfig,
    mode: RequestSourceMode,
) -> Result<()> {
    let conn = req.connection_info();
    let request_origin = request_origin(conn.scheme(), conn.host())?;
    ensure_headers_allowed(
        header_value(req.request(), header::ORIGIN),
        header_value(req.request(), header::REFERER),
        header_value(
            req.request(),
            header::HeaderName::from_static("sec-fetch-site"),
        ),
        &request_origin,
        &site_url::public_site_urls(runtime_config),
        mode,
    )
}

pub(super) fn ensure_headers_allowed(
    origin: Option<&str>,
    referer: Option<&str>,
    sec_fetch_site: Option<&str>,
    request_origin: &str,
    public_site_origins: &[String],
    mode: RequestSourceMode,
) -> Result<()> {
    let fetch_site = source_header_value(
        sec_fetch_site,
        MAX_SEC_FETCH_SITE_LEN,
        "Sec-Fetch-Site",
        ApiSubcode::ValidationRequestHeaderValueInvalid,
    )?
    .map(|value| value.to_ascii_lowercase());

    if let Some(fetch_site) = fetch_site.as_deref() {
        match fetch_site {
            "same-origin" => {}
            "same-site" => {}
            "cross-site" | "none" => {
                return Err(auth_forbidden_with_subcode(
                    ApiSubcode::AuthRequestSourceUntrusted,
                    "untrusted request source for cookie-authenticated action",
                ));
            }
            _ => {}
        }
    }
    let same_site_fetch = fetch_site.as_deref() == Some("same-site");

    if let Some(origin) = source_header_value(
        origin,
        MAX_SOURCE_HEADER_LEN,
        "Origin",
        ApiSubcode::ValidationRequestOriginInvalid,
    )?
    .map(|value| cors::normalize_origin(value, false))
    .transpose()
    .map_aster_err_with(|| {
        validation_error_with_subcode(
            ApiSubcode::ValidationRequestOriginInvalid,
            "invalid Origin header",
        )
    })? {
        if origin_is_trusted(&origin, request_origin, public_site_origins) {
            return Ok(());
        }
        return Err(auth_forbidden_with_subcode(
            ApiSubcode::AuthRequestOriginUntrusted,
            "untrusted request origin for cookie-authenticated action",
        ));
    }

    if let Some(referer) = trimmed_header_value(referer) {
        let referer_origin = origin_from_url(referer)?;
        if origin_is_trusted(&referer_origin, request_origin, public_site_origins) {
            return Ok(());
        }
        return Err(auth_forbidden_with_subcode(
            ApiSubcode::AuthRequestRefererUntrusted,
            "untrusted request referer for cookie-authenticated action",
        ));
    }

    if same_site_fetch {
        return Err(auth_forbidden_with_subcode(
            ApiSubcode::AuthRequestSourceUntrusted,
            "missing trusted request source for same-site cookie-authenticated action",
        ));
    }

    match mode {
        RequestSourceMode::OptionalWhenPresent => Ok(()),
        RequestSourceMode::Required => Err(auth_forbidden_with_subcode(
            ApiSubcode::AuthRequestSourceMissing,
            "missing request source for cookie-authenticated action",
        )),
    }
}

fn header_value(req: &HttpRequest, name: header::HeaderName) -> Option<&str> {
    req.headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
}

fn request_origin(scheme: &str, host: &str) -> Result<String> {
    ensure_value_len(
        scheme,
        MAX_REQUEST_SCHEME_LEN,
        "request scheme",
        ApiSubcode::ValidationRequestSchemeInvalid,
    )?;
    ensure_value_len(
        host,
        MAX_REQUEST_HOST_LEN,
        "request host",
        ApiSubcode::ValidationRequestHostInvalid,
    )?;
    cors::normalize_origin(&format!("{scheme}://{host}"), false).map_aster_err_with(|| {
        validation_error_with_subcode(
            ApiSubcode::ValidationRequestHostInvalid,
            "invalid request host",
        )
    })
}

fn origin_is_trusted(origin: &str, request_origin: &str, public_site_origins: &[String]) -> bool {
    origin == request_origin || public_site_origins.iter().any(|allowed| allowed == origin)
}

fn source_header_value<'a>(
    value: Option<&'a str>,
    max_len: usize,
    label: &str,
    subcode: ApiSubcode,
) -> Result<Option<&'a str>> {
    let Some(value) = trimmed_header_value(value) else {
        return Ok(None);
    };
    ensure_value_len(value, max_len, label, subcode)?;
    Ok(Some(value))
}

fn trimmed_header_value(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn ensure_value_len(value: &str, max_len: usize, label: &str, subcode: ApiSubcode) -> Result<()> {
    if value.len() > max_len {
        return Err(validation_error_with_subcode(
            subcode,
            format!("{label} exceeds {max_len} bytes"),
        ));
    }
    Ok(())
}

fn origin_from_url(url: &str) -> Result<String> {
    let scheme_end = url.find("://").ok_or_else(|| {
        validation_error_with_subcode(
            ApiSubcode::ValidationRequestSchemeInvalid,
            "invalid Referer header",
        )
    })?;
    let scheme = &url[..scheme_end];
    ensure_value_len(
        scheme,
        MAX_REQUEST_SCHEME_LEN,
        "Referer scheme",
        ApiSubcode::ValidationRequestSchemeInvalid,
    )?;

    let authority_start = scheme_end + 3;
    let authority_tail = &url[authority_start..];
    let authority_end = authority_tail
        .char_indices()
        .find_map(|(idx, ch)| matches!(ch, '/' | '?' | '#').then_some(authority_start + idx))
        .unwrap_or(url.len());
    let authority = &url[authority_start..authority_end];
    ensure_value_len(
        authority,
        MAX_REFERER_AUTHORITY_LEN,
        "Referer authority",
        ApiSubcode::ValidationRequestRefererInvalid,
    )?;

    cors::normalize_origin(
        &format!("{}://{}", scheme.to_ascii_lowercase(), authority),
        false,
    )
    .map_aster_err_with(|| {
        validation_error_with_subcode(
            ApiSubcode::ValidationRequestRefererInvalid,
            "invalid Referer header",
        )
    })
}
