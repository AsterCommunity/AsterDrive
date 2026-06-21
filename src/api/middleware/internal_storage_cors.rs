//! Follower internal storage 的浏览器 presigned CORS 放行。

use actix_web::{
    Error, HttpResponse,
    body::{EitherBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    http::{
        Method,
        header::{self, HeaderMap, HeaderValue},
    },
    web,
};
use futures::future::{LocalBoxFuture, Ready, ok};
use reqwest::Url;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, MapAsterErr, Result as AsterResult, validation_error_with_code};
use crate::runtime::FollowerAppState;
use crate::storage::remote_protocol::{
    PRESIGNED_AUTH_ACCESS_KEY_QUERY, REMOTE_BROWSER_PRESIGNED_CORS_ALLOWED_HEADERS,
    REMOTE_BROWSER_PRESIGNED_CORS_GET_EXPOSE_HEADERS,
    REMOTE_BROWSER_PRESIGNED_CORS_PUT_EXPOSE_HEADERS,
};

const PRESIGNED_OBJECTS_PATH_PREFIX: &str = "/api/v1/internal/storage/objects/";
const PREFLIGHT_ALLOWED_METHODS: &str = "GET, PUT, OPTIONS";

pub struct PresignedInternalStorageCors;

impl<S, B> Transform<S, ServiceRequest> for PresignedInternalStorageCors
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = PresignedInternalStorageCorsMiddleware<S>;
    type Future = Ready<std::result::Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(PresignedInternalStorageCorsMiddleware {
            service: Rc::new(service),
        })
    }
}

pub struct PresignedInternalStorageCorsMiddleware<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for PresignedInternalStorageCorsMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, std::result::Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let svc = self.service.clone();

        Box::pin(async move {
            let state = req
                .app_data::<web::Data<FollowerAppState>>()
                .ok_or_else(|| AsterError::internal_error("FollowerAppState not found"))?;
            let Some(cors_context) = resolve_presigned_cors_context(&req, state.get_ref())
                .map_err(Into::<Error>::into)?
            else {
                return Ok(svc.call(req).await?.map_into_left_body());
            };

            if !cors_context.allowed {
                return Ok(forbidden(req).map_into_right_body());
            }

            if is_preflight_request(&req) {
                if !requested_method_is_allowed(&req)
                    || !requested_headers_are_allowed(&req).map_err(Into::<Error>::into)?
                {
                    return Ok(forbidden(req).map_into_right_body());
                }

                let mut response = HttpResponse::NoContent().finish();
                apply_origin_headers(response.headers_mut(), &cors_context.origin)
                    .map_err(Into::<Error>::into)?;
                apply_preflight_headers(response.headers_mut());
                return Ok(req.into_response(response).map_into_right_body());
            }

            let request_method = req.method().clone();
            let mut response = svc.call(req).await?.map_into_left_body();
            apply_origin_headers(response.headers_mut(), &cors_context.origin)
                .map_err(Into::<Error>::into)?;
            apply_actual_headers(response.headers_mut(), &request_method);
            Ok(response)
        })
    }
}

#[derive(Debug)]
struct PresignedCorsContext {
    origin: String,
    allowed: bool,
}

fn resolve_presigned_cors_context(
    req: &ServiceRequest,
    state: &FollowerAppState,
) -> AsterResult<Option<PresignedCorsContext>> {
    if !matches!(req.method(), &Method::GET | &Method::PUT | &Method::OPTIONS) {
        return Ok(None);
    }
    if !req.path().starts_with(PRESIGNED_OBJECTS_PATH_PREFIX) {
        return Ok(None);
    }

    let Some(origin_header) = req.headers().get(header::ORIGIN) else {
        return Ok(None);
    };
    let origin = crate::config::cors::normalize_origin(
        origin_header.to_str().map_aster_err_with(|| {
            validation_error_with_code(
                ApiErrorCode::ValidationRequestOriginInvalid,
                "invalid Origin header",
            )
        })?,
        false,
    )
    .map_err(|error| error.with_api_error_code(ApiErrorCode::ValidationRequestOriginInvalid))?;

    let access_key = web::Query::<HashMap<String, String>>::from_query(req.query_string())
        .map_err(|_| AsterError::validation_error("invalid query string"))?
        .get(PRESIGNED_AUTH_ACCESS_KEY_QUERY)
        .cloned()
        .filter(|value| !value.is_empty());
    let Some(access_key) = access_key else {
        return Ok(None);
    };

    let allowed_origin = state
        .driver_registry
        .find_master_binding_by_access_key(&access_key)
        .filter(|binding| binding.is_enabled)
        .and_then(|binding| browser_origin_from_master_url(&binding.master_url).ok());

    Ok(Some(PresignedCorsContext {
        allowed: allowed_origin.as_deref() == Some(origin.as_str()),
        origin,
    }))
}

fn browser_origin_from_master_url(master_url: &str) -> AsterResult<String> {
    let trimmed = master_url.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(
            "master binding master_url cannot be empty for browser CORS",
        ));
    }

    let url = Url::parse(trimmed).map_err(|e| {
        AsterError::validation_error(format!("invalid master binding master_url: {e}"))
    })?;
    let host = url.host_str().ok_or_else(|| {
        AsterError::validation_error("master binding master_url must include a host")
    })?;
    let scheme = url.scheme().to_ascii_lowercase();
    let mut authority = host.to_ascii_lowercase();
    if let Some(port) = url.port() {
        let is_default_port =
            (scheme == "http" && port == 80) || (scheme == "https" && port == 443);
        if !is_default_port {
            authority.push(':');
            authority.push_str(&port.to_string());
        }
    }

    Ok(format!("{scheme}://{authority}"))
}

fn is_preflight_request(req: &ServiceRequest) -> bool {
    req.method() == Method::OPTIONS
        && req
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
}

fn requested_method_is_allowed(req: &ServiceRequest) -> bool {
    req.headers()
        .get(header::ACCESS_CONTROL_REQUEST_METHOD)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.eq_ignore_ascii_case("GET") || value.eq_ignore_ascii_case("PUT"))
        .unwrap_or(false)
}

fn requested_headers_are_allowed(req: &ServiceRequest) -> AsterResult<bool> {
    let Some(request_headers) = req.headers().get(header::ACCESS_CONTROL_REQUEST_HEADERS) else {
        return Ok(true);
    };

    let request_headers = request_headers
        .to_str()
        .map_aster_err_with(invalid_access_control_request_headers)?;
    let allowed_headers = REMOTE_BROWSER_PRESIGNED_CORS_ALLOWED_HEADERS
        .split(',')
        .map(str::trim)
        .filter(|header| !header.is_empty())
        .map(|header| {
            let normalized = header.to_ascii_lowercase();
            normalized
                .parse::<header::HeaderName>()
                .map(|_| normalized)
                .map_aster_err_with(invalid_access_control_request_headers)
        })
        .collect::<AsterResult<HashSet<_>>>()?;

    for requested in request_headers.split(',') {
        let requested = requested.trim().to_ascii_lowercase();
        if requested.is_empty() {
            continue;
        }

        let _: header::HeaderName = requested
            .parse()
            .map_aster_err_with(invalid_access_control_request_headers)?;

        if !allowed_headers.contains(&requested) {
            return Ok(false);
        }
    }

    Ok(true)
}

fn invalid_access_control_request_headers() -> AsterError {
    validation_error_with_code(
        ApiErrorCode::ValidationRequestHeaderValueInvalid,
        "invalid Access-Control-Request-Headers",
    )
}

fn apply_origin_headers(headers: &mut HeaderMap, origin: &str) -> AsterResult<()> {
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_str(origin).map_aster_err_with(|| {
            AsterError::internal_error("failed to serialize Access-Control-Allow-Origin")
        })?,
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    ensure_vary(headers, "Origin")?;
    Ok(())
}

fn apply_preflight_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static(PREFLIGHT_ALLOWED_METHODS),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(REMOTE_BROWSER_PRESIGNED_CORS_ALLOWED_HEADERS),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("3600"),
    );
    let _ = ensure_vary(headers, "Access-Control-Request-Method");
    let _ = ensure_vary(headers, "Access-Control-Request-Headers");
}

fn apply_actual_headers(headers: &mut HeaderMap, method: &Method) {
    let exposed = match *method {
        Method::GET => REMOTE_BROWSER_PRESIGNED_CORS_GET_EXPOSE_HEADERS,
        Method::PUT => REMOTE_BROWSER_PRESIGNED_CORS_PUT_EXPOSE_HEADERS,
        _ => return,
    };
    headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static(exposed),
    );
}

fn ensure_vary(headers: &mut HeaderMap, value: &str) -> AsterResult<()> {
    let mut vary_values = BTreeSet::new();

    if let Some(existing) = headers.get(header::VARY) {
        let existing = existing
            .to_str()
            .map_aster_err_ctx("invalid Vary header", AsterError::internal_error)?;
        for item in existing.split(',') {
            let item = item.trim();
            if !item.is_empty() {
                vary_values.insert(item.to_string());
            }
        }
    }

    vary_values.insert(value.to_string());
    let joined = vary_values.into_iter().collect::<Vec<_>>().join(", ");
    let header_value = HeaderValue::from_str(&joined).map_aster_err_ctx(
        "failed to serialize Vary header",
        AsterError::internal_error,
    )?;
    headers.insert(header::VARY, header_value);
    Ok(())
}

fn forbidden(req: ServiceRequest) -> ServiceResponse {
    let mut response = HttpResponse::Forbidden().finish();
    let _ = ensure_vary(response.headers_mut(), "Origin");
    let _ = ensure_vary(response.headers_mut(), "Access-Control-Request-Method");
    let _ = ensure_vary(response.headers_mut(), "Access-Control-Request-Headers");
    req.into_response(response)
}

#[cfg(test)]
mod tests {
    use super::{apply_actual_headers, browser_origin_from_master_url};
    use actix_web::http::{
        Method, header,
        header::{HeaderMap, HeaderValue},
    };

    #[test]
    fn apply_actual_headers_exposes_content_range_for_get() {
        let mut headers = HeaderMap::new();
        apply_actual_headers(&mut headers, &Method::GET);

        let exposed = headers
            .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .and_then(|value| value.to_str().ok())
            .expect("GET responses should expose download headers");
        assert!(
            exposed
                .split(',')
                .any(|header| header.trim() == "Content-Range"),
            "GET expose headers should include Content-Range"
        );
        assert!(
            exposed
                .split(',')
                .any(|header| header.trim() == "Accept-Ranges"),
            "GET expose headers should include Accept-Ranges"
        );
    }

    #[test]
    fn apply_actual_headers_keeps_existing_put_expose_headers() {
        let mut headers = HeaderMap::new();
        apply_actual_headers(&mut headers, &Method::PUT);

        assert_eq!(
            headers.get(header::ACCESS_CONTROL_EXPOSE_HEADERS),
            Some(&HeaderValue::from_static("ETag"))
        );
    }

    #[test]
    fn browser_origin_from_master_url_drops_default_https_port() {
        assert_eq!(
            browser_origin_from_master_url(" HTTPS://Example.COM:443/admin/settings ")
                .expect("default https port should normalize"),
            "https://example.com"
        );
    }

    #[test]
    fn browser_origin_from_master_url_keeps_non_default_port() {
        assert_eq!(
            browser_origin_from_master_url("http://Example.COM:8085/api/v1")
                .expect("non-default port should be preserved"),
            "http://example.com:8085"
        );
    }
}
