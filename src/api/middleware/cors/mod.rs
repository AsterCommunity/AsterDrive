//! CORS 中间件模块入口。

mod constants;

#[cfg(test)]
mod tests;

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
use std::collections::BTreeSet;
use std::rc::Rc;

use self::constants::{
    ALLOWED_HEADERS, ALLOWED_HEADERS_VALUE, ALLOWED_METHODS, ALLOWED_METHODS_VALUE,
    EXPOSE_HEADERS_VALUE,
};
use crate::config::cors::RuntimeCorsPolicy;
use crate::errors::{AsterError, MapAsterErr};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};

pub struct RuntimeCors;

impl<S, B> Transform<S, ServiceRequest> for RuntimeCors
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = RuntimeCorsMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(RuntimeCorsMiddleware {
            service: Rc::new(service),
        })
    }
}

pub struct RuntimeCorsMiddleware<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for RuntimeCorsMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let svc = self.service.clone();

        Box::pin(async move {
            let state = req
                .app_data::<web::Data<PrimaryAppState>>()
                .ok_or_else(|| AsterError::internal_error("PrimaryAppState not found"))?;
            let policy = RuntimeCorsPolicy::from_runtime_config(state.runtime_config());

            // Static assets and public pages don't need CORS enforcement
            if is_cors_exempt_path(req.path()) {
                return Ok(svc.call(req).await?.map_into_left_body());
            }

            let Some(origin_header) = req.headers().get(header::ORIGIN).cloned() else {
                return Ok(svc.call(req).await?.map_into_left_body());
            };

            let origin = crate::config::cors::normalize_origin(
                origin_header
                    .to_str()
                    .map_aster_err_with(|| AsterError::validation_error("invalid Origin header"))?,
                false,
            )?;

            if !policy.enforces_requests() {
                return Ok(svc.call(req).await?.map_into_left_body());
            }

            if request_is_same_origin(&req, &origin) {
                return Ok(svc.call(req).await?.map_into_left_body());
            }

            if !policy.allows_origin(&origin) {
                return Ok(forbidden(req).map_into_right_body());
            }

            if is_preflight_request(&req) {
                if !requested_method_is_allowed(&req) || !requested_headers_are_allowed(&req)? {
                    return Ok(forbidden(req).map_into_right_body());
                }

                let mut response = HttpResponse::NoContent().finish();
                apply_origin_headers(response.headers_mut(), &policy, &origin)?;
                apply_preflight_headers(response.headers_mut(), &policy);
                return Ok(req.into_response(response).map_into_right_body());
            }

            let mut response = svc.call(req).await?.map_into_left_body();
            apply_origin_headers(response.headers_mut(), &policy, &origin)?;
            apply_actual_headers(response.headers_mut(), &policy);
            Ok(response)
        })
    }
}

fn is_preflight_request(req: &ServiceRequest) -> bool {
    req.method() == Method::OPTIONS
        && req
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
}

/// Paths that serve static assets or public pages — no CORS enforcement needed.
fn is_cors_exempt_path(path: &str) -> bool {
    matches!(
        path,
        "/" | "/index.html" | "/favicon.svg" | "/manifest.webmanifest" | "/sw.js"
    ) || path.starts_with("/workbox-")
        || path.starts_with("/assets/")
        || path.starts_with("/static/")
        || path.starts_with("/pdfjs/")
}

fn request_is_same_origin(req: &ServiceRequest, origin: &str) -> bool {
    let conn = req.connection_info();
    let request_origin = format!(
        "{}://{}",
        conn.scheme().to_ascii_lowercase(),
        conn.host().to_ascii_lowercase()
    );
    request_origin == origin
}

fn requested_method_is_allowed(req: &ServiceRequest) -> bool {
    let Some(method) = req.headers().get(header::ACCESS_CONTROL_REQUEST_METHOD) else {
        return false;
    };

    let Ok(method) = method.to_str() else {
        return false;
    };

    ALLOWED_METHODS.contains(&method)
}

fn requested_headers_are_allowed(req: &ServiceRequest) -> Result<bool, AsterError> {
    let Some(request_headers) = req.headers().get(header::ACCESS_CONTROL_REQUEST_HEADERS) else {
        return Ok(true);
    };

    let request_headers = request_headers.to_str().map_aster_err_with(|| {
        AsterError::validation_error("invalid Access-Control-Request-Headers")
    })?;

    let allowed_headers = ALLOWED_HEADERS
        .iter()
        .copied()
        .collect::<BTreeSet<&'static str>>();

    for requested in request_headers.split(',') {
        let requested = requested.trim().to_ascii_lowercase();
        if requested.is_empty() {
            continue;
        }

        let _: header::HeaderName = requested.parse().map_aster_err_with(|| {
            AsterError::validation_error("invalid Access-Control-Request-Headers")
        })?;

        if !allowed_headers.contains(requested.as_str()) {
            return Ok(false);
        }
    }

    Ok(true)
}

fn apply_origin_headers(
    headers: &mut HeaderMap,
    policy: &RuntimeCorsPolicy,
    origin: &str,
) -> Result<(), AsterError> {
    if !headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN) {
        let value = if policy.sends_wildcard_origin() {
            HeaderValue::from_static("*")
        } else {
            HeaderValue::from_str(origin).map_aster_err_with(|| {
                AsterError::internal_error("failed to serialize Access-Control-Allow-Origin")
            })?
        };

        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }

    if policy.allow_credentials && !headers.contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS) {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
            HeaderValue::from_static("true"),
        );
    }

    ensure_vary(headers, "Origin")?;
    Ok(())
}

fn apply_preflight_headers(headers: &mut HeaderMap, policy: &RuntimeCorsPolicy) {
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_str(ALLOWED_METHODS_VALUE.as_str()).unwrap_or_else(|error| {
            tracing::warn!(%error, "failed to serialize CORS allow methods header");
            HeaderValue::from_static("")
        }),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_str(ALLOWED_HEADERS_VALUE.as_str()).unwrap_or_else(|error| {
            tracing::warn!(%error, "failed to serialize CORS allow headers header");
            HeaderValue::from_static("")
        }),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_str(&policy.max_age_secs.to_string()).unwrap_or_else(|error| {
            tracing::warn!(%error, "failed to serialize CORS max-age header");
            HeaderValue::from_static("0")
        }),
    );
    if let Err(error) = ensure_vary(headers, "Access-Control-Request-Method") {
        tracing::warn!(
            error = error.message(),
            "failed to add CORS preflight method Vary header"
        );
    }
    if let Err(error) = ensure_vary(headers, "Access-Control-Request-Headers") {
        tracing::warn!(
            error = error.message(),
            "failed to add CORS preflight headers Vary header"
        );
    }
}

fn apply_actual_headers(headers: &mut HeaderMap, _policy: &RuntimeCorsPolicy) {
    headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_str(EXPOSE_HEADERS_VALUE.as_str()).unwrap_or_else(|error| {
            tracing::warn!(%error, "failed to serialize CORS expose headers header");
            HeaderValue::from_static("")
        }),
    );
}

fn ensure_vary(headers: &mut HeaderMap, value: &str) -> Result<(), AsterError> {
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
